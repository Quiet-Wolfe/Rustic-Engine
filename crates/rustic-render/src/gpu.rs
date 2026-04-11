use std::path::Path;
use std::sync::Arc;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::postprocess::PostProcessor;
use crate::shader::SHADER_SRC;
use crate::sprites::SpriteFrame;
use crate::text::TextSystem;

const MAX_SPRITES: usize = 4096;
const MAX_VERTICES: usize = MAX_SPRITES * 4;
const MAX_INDICES: usize = MAX_SPRITES * 6;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct SpriteVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Projection {
    matrix: [[f32; 4]; 4],
}

/// Handle to a GPU texture with its bind group.
pub struct GpuTexture {
    pub bind_group: wgpu::BindGroup,
    pub width: u32,
    pub height: u32,
}

/// Core GPU state: device, surface, sprite pipeline.
pub struct GpuState {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    proj_buffer: wgpu::Buffer,
    proj_bind_group: wgpu::BindGroup,
    pub texture_layout: wgpu::BindGroupLayout,
    pub sampler: wgpu::Sampler,
    nearest_sampler: wgpu::Sampler,
    // Batch
    vertices: Vec<SpriteVertex>,
    indices: Vec<u32>,
    // Default 1x1 white texture for colored quads
    white_texture: GpuTexture,
    // Text
    text_system: TextSystem,
    // Logical game resolution
    pub game_w: f32,
    pub game_h: f32,
    // Per-frame state for multi-batch rendering
    frame_output: Option<wgpu::SurfaceTexture>,
    frame_view: Option<wgpu::TextureView>,
    frame_cleared: bool,
    // Post-processing
    pub postprocess: PostProcessor,
    /// When true, scene renders to offscreen target and post-process applies to surface.
    pp_active: bool,
    /// Pending postprocess state change (deferred to next frame to avoid mid-frame state inconsistency).
    pp_pending: Option<bool>,
    surface_view_for_pp: Option<wgpu::TextureView>,
    surface_output_for_pp: Option<wgpu::SurfaceTexture>,
}

impl GpuState {
    pub async fn new(window: Arc<Window>, mut game_w: f32, game_h: f32) -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Failed to find a suitable GPU adapter");

        // Log adapter info for debugging
        let adapter_info = adapter.get_info();
        log::info!("GPU: {} ({:?})", adapter_info.name, adapter_info.backend);
        log::info!("Driver: {}", adapter_info.driver);

        let limits = wgpu::Limits::default();

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("RusticV2"),
                required_features: wgpu::Features::empty(),
                required_limits: limits,
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
                ..Default::default()
            })
            .await
            .expect("Failed to create GPU device");

        let size = window.inner_size();
        let surface_caps = surface.get_capabilities(&adapter);
        let format = surface_caps.formats.iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&device, &config);

        // On Android, expand game width to fill the screen's aspect ratio (no letterboxing).
        // Content designed for 1280x720 stays centered; extra width is usable space.
        #[cfg(target_os = "android")]
        {
            let aspect = config.width as f32 / config.height as f32;
            game_w = (game_h * aspect).max(game_w);
            log::info!("Android: game_w adjusted to {:.0} for {:.2} aspect ratio", game_w, aspect);
        }

        // Shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Sprite Shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });

        // Projection uniform
        let proj = ortho_projection(game_w, game_h);
        let proj_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Projection"),
            contents: bytemuck::cast_slice(&[proj]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let proj_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Projection Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let proj_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Projection Bind Group"),
            layout: &proj_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: proj_buffer.as_entire_binding(),
            }],
        });

        // Texture bind group layout (texture + sampler)
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Texture Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Pipeline
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Sprite Pipeline Layout"),
            bind_group_layouts: &[&proj_layout, &texture_layout],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sprite Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<SpriteVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x2 },
                        wgpu::VertexAttribute { offset: 8, shader_location: 1, format: wgpu::VertexFormat::Float32x2 },
                        wgpu::VertexAttribute { offset: 16, shader_location: 2, format: wgpu::VertexFormat::Float32x4 },
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });

        // Vertex/index buffers (pre-allocated)
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Sprite Vertices"),
            size: (MAX_VERTICES * std::mem::size_of::<SpriteVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Sprite Indices"),
            size: (MAX_INDICES * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // 1x1 white texture for colored quads
        let white_tex = device.create_texture_with_data(
            &queue,
            &wgpu::TextureDescriptor {
                label: Some("White 1x1"),
                size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &[255u8, 255, 255, 255],
        );
        let white_view = white_tex.create_view(&Default::default());
        let white_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("White Texture Bind Group"),
            layout: &texture_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&white_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        let white_texture = GpuTexture { bind_group: white_bind_group, width: 1, height: 1 };

        let text_system = TextSystem::new(&device, &queue, format);

        let postprocess = PostProcessor::new(&device, format, game_w as u32, game_h as u32);

        Self {
            device,
            queue,
            surface,
            config,
            pipeline,
            vertex_buffer,
            index_buffer,
            proj_buffer,
            proj_bind_group,
            texture_layout,
            sampler,
            nearest_sampler,
            vertices: Vec::with_capacity(MAX_VERTICES),
            indices: Vec::with_capacity(MAX_INDICES),
            white_texture,
            text_system,
            game_w,
            game_h,
            frame_output: None,
            frame_view: None,
            frame_cleared: false,
            postprocess,
            pp_active: false,
            pp_pending: None,
            surface_view_for_pp: None,
            surface_output_for_pp: None,
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    /// Resize image if it exceeds GPU max texture dimension.
    fn clamp_image_size(&self, img: &mut image::RgbaImage) {
        let max_dim = self.device.limits().max_texture_dimension_2d;
        let (w, h) = img.dimensions();
        if w > max_dim || h > max_dim {
            let scale = if w > h { max_dim as f32 / w as f32 } else { max_dim as f32 / h as f32 };
            let new_w = (w as f32 * scale) as u32;
            let new_h = (h as f32 * scale) as u32;
            log::warn!("Image {}x{} exceeds GPU max texture dimension {}, resizing to {}x{}", w, h, max_dim, new_w, new_h);
            *img = image::imageops::resize(img, new_w, new_h, image::imageops::FilterType::Triangle);
        }
    }

    pub fn load_texture_from_path(&self, path: &Path) -> GpuTexture {
        let mut img = image::open(path)
            .unwrap_or_else(|e| panic!("Failed to load image {:?}: {}", path, e))
            .to_rgba8();
        // Premultiply alpha to eliminate white fringing on transparent edges
        for pixel in img.pixels_mut() {
            let a = pixel[3] as f32 / 255.0;
            pixel[0] = (pixel[0] as f32 * a + 0.5) as u8;
            pixel[1] = (pixel[1] as f32 * a + 0.5) as u8;
            pixel[2] = (pixel[2] as f32 * a + 0.5) as u8;
        }
        self.clamp_image_size(&mut img);
        let (width, height) = img.dimensions();

        let texture = self.device.create_texture_with_data(
            &self.queue,
            &wgpu::TextureDescriptor {
                label: Some(path.to_str().unwrap_or("texture")),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &img,
        );

        let view = texture.create_view(&Default::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Texture Bind Group"),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });

        GpuTexture { bind_group, width, height }
    }

    /// Load a texture with nearest-neighbor (point) filtering — for pixel art.
    pub fn load_texture_from_path_nearest(&self, path: &Path) -> GpuTexture {
        let mut img = image::open(path)
            .unwrap_or_else(|e| panic!("Failed to load image {:?}: {}", path, e))
            .to_rgba8();
        for pixel in img.pixels_mut() {
            let a = pixel[3] as f32 / 255.0;
            pixel[0] = (pixel[0] as f32 * a + 0.5) as u8;
            pixel[1] = (pixel[1] as f32 * a + 0.5) as u8;
            pixel[2] = (pixel[2] as f32 * a + 0.5) as u8;
        }
        self.clamp_image_size(&mut img);
        let (width, height) = img.dimensions();

        let texture = self.device.create_texture_with_data(
            &self.queue,
            &wgpu::TextureDescriptor {
                label: Some(path.to_str().unwrap_or("texture")),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &img,
        );

        let view = texture.create_view(&Default::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Texture Bind Group (nearest)"),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.nearest_sampler) },
            ],
        });

        GpuTexture { bind_group, width, height }
    }

    /// Create a solid-color texture. `color_hex` is "RRGGBB" or "AARRGGBB".
    pub fn create_solid_texture(&self, width: u32, height: u32, color_hex: &str) -> GpuTexture {
        let hex = color_hex.trim_start_matches('#');
        let (r, g, b, a) = if hex.len() >= 8 {
            (
                u8::from_str_radix(&hex[2..4], 16).unwrap_or(255),
                u8::from_str_radix(&hex[4..6], 16).unwrap_or(255),
                u8::from_str_radix(&hex[6..8], 16).unwrap_or(255),
                u8::from_str_radix(&hex[0..2], 16).unwrap_or(255),
            )
        } else {
            (
                u8::from_str_radix(&hex[0..2], 16).unwrap_or(255),
                u8::from_str_radix(&hex[2..4], 16).unwrap_or(255),
                u8::from_str_radix(&hex[4..6], 16).unwrap_or(255),
                255,
            )
        };
        // Premultiply
        let af = a as f32 / 255.0;
        let pixel = [(r as f32 * af + 0.5) as u8, (g as f32 * af + 0.5) as u8, (b as f32 * af + 0.5) as u8, a];
        let w = width.max(1);
        let h = height.max(1);
        let data: Vec<u8> = pixel.iter().copied().cycle().take((w * h * 4) as usize).collect();

        let texture = self.device.create_texture_with_data(
            &self.queue,
            &wgpu::TextureDescriptor {
                label: Some("solid_color"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &data,
        );
        let view = texture.create_view(&Default::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Solid Texture Bind Group"),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        GpuTexture { bind_group, width: w, height: h }
    }

    /// Add a sprite frame to the current batch.
    pub fn draw_sprite_frame(
        &mut self,
        frame: &SpriteFrame,
        tex_w: f32,
        tex_h: f32,
        x: f32,
        y: f32,
        scale: f32,
        flip_x: bool,
        color: [f32; 4],
    ) {
        if frame.rotated {
            // Rotated 90deg CW in atlas — display size is (src.h, src.w)
            let display_w = frame.src.h * scale;
            let display_h = frame.src.w * scale;

            let draw_x = if flip_x {
                x + (frame.frame_w + frame.offset_x - frame.src.h) * scale
            } else {
                x - frame.offset_x * scale
            };
            let draw_y = y - frame.offset_y * scale;

            // UV corners of the source rect in the atlas
            let u0 = frame.src.x / tex_w;
            let v0 = frame.src.y / tex_h;
            let u1 = (frame.src.x + frame.src.w) / tex_w;
            let v1 = (frame.src.y + frame.src.h) / tex_h;

            // Remap UVs to un-rotate: display quad maps to rotated atlas rect
            // Display TL->TR->BR->BL maps to atlas (u1,v0)->(u1,v1)->(u0,v1)->(u0,v0)
            let (tl_u, tl_v, tr_u, tr_v, br_u, br_v, bl_u, bl_v) = if flip_x {
                (u0, v0, u0, v1, u1, v1, u1, v0)
            } else {
                (u1, v0, u1, v1, u0, v1, u0, v0)
            };

            self.push_quad(
                draw_x, draw_y, display_w, display_h,
                tl_u, tl_v, tr_u, tr_v, br_u, br_v, bl_u, bl_v,
                color,
            );
        } else {
            let w = frame.src.w * scale;
            let h = frame.src.h * scale;

            let draw_x = if flip_x {
                x + (frame.frame_w + frame.offset_x - frame.src.w) * scale
            } else {
                x - frame.offset_x * scale
            };
            let draw_y = y - frame.offset_y * scale;

            let u0 = frame.src.x / tex_w;
            let v0 = frame.src.y / tex_h;
            let u1 = (frame.src.x + frame.src.w) / tex_w;
            let v1 = (frame.src.y + frame.src.h) / tex_h;

            let (u_left, u_right) = if flip_x { (u1, u0) } else { (u0, u1) };

            self.push_quad(
                draw_x, draw_y, w, h,
                u_left, v0, u_right, v0, u_right, v1, u_left, v1,
                color,
            );
        }
    }

    /// Add a sprite frame to the current batch, flipped vertically (for reflections).
    pub fn draw_sprite_frame_flip_y(
        &mut self,
        frame: &SpriteFrame,
        tex_w: f32,
        tex_h: f32,
        x: f32,
        y: f32,
        scale: f32,
        flip_x: bool,
        color: [f32; 4],
    ) {
        // Same as draw_sprite_frame but V coordinates swapped (v0↔v1)
        if frame.rotated {
            let display_w = frame.src.h * scale;
            let display_h = frame.src.w * scale;
            let draw_x = if flip_x {
                x + (frame.frame_w + frame.offset_x - frame.src.h) * scale
            } else {
                x - frame.offset_x * scale
            };
            let draw_y = y - frame.offset_y * scale;
            let u0 = frame.src.x / tex_w;
            let v0 = frame.src.y / tex_h;
            let u1 = (frame.src.x + frame.src.w) / tex_w;
            let v1 = (frame.src.y + frame.src.h) / tex_h;
            // Rotated + flip_y: swap the V mapping
            let (tl_u, tl_v, tr_u, tr_v, br_u, br_v, bl_u, bl_v) = if flip_x {
                (u0, v1, u0, v0, u1, v0, u1, v1)
            } else {
                (u1, v1, u1, v0, u0, v0, u0, v1)
            };
            self.push_quad(
                draw_x, draw_y, display_w, display_h,
                tl_u, tl_v, tr_u, tr_v, br_u, br_v, bl_u, bl_v,
                color,
            );
        } else {
            let w = frame.src.w * scale;
            let h = frame.src.h * scale;
            let draw_x = if flip_x {
                x + (frame.frame_w + frame.offset_x - frame.src.w) * scale
            } else {
                x - frame.offset_x * scale
            };
            let draw_y = y - frame.offset_y * scale;
            let u0 = frame.src.x / tex_w;
            let v0 = frame.src.y / tex_h;
            let u1 = (frame.src.x + frame.src.w) / tex_w;
            let v1 = (frame.src.y + frame.src.h) / tex_h;
            // flip_y: swap v0 and v1
            let (u_left, u_right) = if flip_x { (u1, u0) } else { (u0, u1) };
            self.push_quad(
                draw_x, draw_y, w, h,
                u_left, v1, u_right, v1, u_right, v0, u_left, v0,
                color,
            );
        }
    }

    /// Draw a sprite frame with rotation (angle in degrees, around frame center).
    pub fn draw_sprite_frame_rotated(
        &mut self,
        frame: &SpriteFrame,
        tex_w: f32,
        tex_h: f32,
        x: f32,
        y: f32,
        scale: f32,
        flip_x: bool,
        angle_deg: f32,
        color: [f32; 4],
    ) {
        if angle_deg.abs() < 0.01 {
            return self.draw_sprite_frame(frame, tex_w, tex_h, x, y, scale, flip_x, color);
        }
        // Compute display size and draw position (same as draw_sprite_frame)
        let (w, h, draw_x, draw_y, u0, v0, u1, v1) = if frame.rotated {
            let dw = frame.src.h * scale;
            let dh = frame.src.w * scale;
            let dx = if flip_x {
                x + (frame.frame_w + frame.offset_x - frame.src.h) * scale
            } else {
                x - frame.offset_x * scale
            };
            let dy = y - frame.offset_y * scale;
            let _u0 = frame.src.x / tex_w;
            let _v0 = frame.src.y / tex_h;
            let _u1 = (frame.src.x + frame.src.w) / tex_w;
            let _v1 = (frame.src.y + frame.src.h) / tex_h;
            // For rotated atlas frames, we'd need special UV handling.
            // For now, approximate with the un-rotated UVs (most note skins aren't atlas-rotated).
            (dw, dh, dx, dy, _u0, _v0, _u1, _v1)
        } else {
            let w = frame.src.w * scale;
            let h = frame.src.h * scale;
            let dx = if flip_x {
                x + (frame.frame_w + frame.offset_x - frame.src.w) * scale
            } else {
                x - frame.offset_x * scale
            };
            let dy = y - frame.offset_y * scale;
            let u0 = frame.src.x / tex_w;
            let v0 = frame.src.y / tex_h;
            let u1 = (frame.src.x + frame.src.w) / tex_w;
            let v1 = (frame.src.y + frame.src.h) / tex_h;
            (w, h, dx, dy, u0, v0, u1, v1)
        };
        let cx = draw_x + w / 2.0;
        let cy = draw_y + h / 2.0;
        let angle_rad = angle_deg.to_radians();
        self.push_quad_rotated(cx, cy, w, h, u0, v0, u1, v1, angle_rad, flip_x, color);
    }

    pub fn push_quad(
        &mut self,
        x: f32, y: f32, w: f32, h: f32,
        tl_u: f32, tl_v: f32,
        tr_u: f32, tr_v: f32,
        br_u: f32, br_v: f32,
        bl_u: f32, bl_v: f32,
        color: [f32; 4],
    ) {
        let base = self.vertices.len() as u32;
        self.vertices.push(SpriteVertex { position: [x, y], uv: [tl_u, tl_v], color });
        self.vertices.push(SpriteVertex { position: [x + w, y], uv: [tr_u, tr_v], color });
        self.vertices.push(SpriteVertex { position: [x + w, y + h], uv: [br_u, br_v], color });
        self.vertices.push(SpriteVertex { position: [x, y + h], uv: [bl_u, bl_v], color });
        self.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Push a quad with 4 arbitrary vertex positions (for matrix-transformed sprites like Adobe Animate atlas).
    pub fn push_raw_quad(
        &mut self,
        positions: [[f32; 2]; 4],
        uvs: [[f32; 2]; 4],
        color: [f32; 4],
    ) {
        let base = self.vertices.len() as u32;
        for i in 0..4 {
            self.vertices.push(SpriteVertex {
                position: positions[i],
                uv: uvs[i],
                color,
            });
        }
        self.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Push a quad rotated by `angle` radians around its center. Coordinates in game-space pixels.
    pub fn push_quad_rotated(
        &mut self,
        cx: f32, cy: f32, w: f32, h: f32,
        u0: f32, v0: f32, u1: f32, v1: f32,
        angle: f32,
        flip_x: bool,
        color: [f32; 4],
    ) {
        let (sin, cos) = angle.sin_cos();
        let hw = w / 2.0;
        let hh = h / 2.0;
        // Corners relative to center, then rotate
        let corners = [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)];
        let mut verts = [[0.0f32; 2]; 4];
        for (i, (lx, ly)) in corners.iter().enumerate() {
            verts[i] = [cx + lx * cos - ly * sin, cy + lx * sin + ly * cos];
        }
        let (ul, ur) = if flip_x { (u1, u0) } else { (u0, u1) };
        let base = self.vertices.len() as u32;
        self.vertices.push(SpriteVertex { position: verts[0], uv: [ul, v0], color });
        self.vertices.push(SpriteVertex { position: verts[1], uv: [ur, v0], color });
        self.vertices.push(SpriteVertex { position: verts[2], uv: [ur, v1], color });
        self.vertices.push(SpriteVertex { position: verts[3], uv: [ul, v1], color });
        self.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Draw a solid-colored quad (no texture). Coordinates in game-space pixels.
    pub fn push_colored_quad(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.push_quad(
            x, y, w, h,
            0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0,
            color,
        );
    }

    /// Present using the built-in white texture (for screens that only use colored quads + text).
    pub fn present_no_texture(&mut self) -> bool {
        self.present_inner(None)
    }

    /// Queue text to be drawn this frame. Coordinates in game-space pixels.
    pub fn draw_text(&mut self, text: &str, x: f32, y: f32, size: f32, color: [f32; 4]) {
        self.text_system.draw_text(text, x, y, size, color);
    }

    /// Flush the batch and present with the given texture.
    pub fn present(&mut self, texture: &GpuTexture) -> bool {
        self.present_inner(Some(texture))
    }

    fn present_inner(&mut self, texture: Option<&GpuTexture>) -> bool {
        let tex_bind_group = texture
            .map(|t| &t.bind_group)
            .unwrap_or(&self.white_texture.bind_group);
        let output = match self.surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                self.vertices.clear();
                self.indices.clear();
                return false;
            }
            Err(e) => {
                log::error!("Surface error: {e}");
                self.vertices.clear();
                self.indices.clear();
                return false;
            }
        };

        let view = output.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());

        // Upload vertex/index data
        if !self.vertices.is_empty() {
            self.queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&self.vertices));
            self.queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&self.indices));
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Sprite Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                ..Default::default()
            });

            // Letterboxed viewport
            let win_w = self.config.width as f32;
            let win_h = self.config.height as f32;
            let scale = (win_w / self.game_w).min(win_h / self.game_h);
            let vp_w = self.game_w * scale;
            let vp_h = self.game_h * scale;
            let vp_x = (win_w - vp_w) / 2.0;
            let vp_y = (win_h - vp_h) / 2.0;
            pass.set_viewport(vp_x, vp_y, vp_w, vp_h, 0.0, 1.0);

            if !self.vertices.is_empty() {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.proj_bind_group, &[]);
                pass.set_bind_group(1, tex_bind_group, &[]);
                pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.indices.len() as u32, 0, 0..1);
            }

            // Text on top of sprites — rendered at native viewport resolution
            self.text_system.render(
                &self.device,
                &self.queue,
                &mut pass,
                self.game_w,
                self.game_h,
                vp_w,
                vp_h,
            );
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        self.vertices.clear();
        self.indices.clear();
        true
    }

    fn viewport_rect(&self) -> (f32, f32, f32, f32) {
        let win_w = self.config.width as f32;
        let win_h = self.config.height as f32;
        let scale = (win_w / self.game_w).min(win_h / self.game_h);
        let vp_w = self.game_w * scale;
        let vp_h = self.game_h * scale;
        let vp_x = (win_w - vp_w) / 2.0;
        let vp_y = (win_h - vp_h) / 2.0;
        (vp_x, vp_y, vp_w, vp_h)
    }

    /// Convert physical pixel coordinates to game-space coordinates (1280x720).
    /// Returns None if the point is outside the letterboxed viewport.
    pub fn physical_to_game(&self, px: f64, py: f64) -> Option<(f32, f32)> {
        let (vp_x, vp_y, vp_w, vp_h) = self.viewport_rect();
        let gx = (px as f32 - vp_x) / vp_w * self.game_w;
        let gy = (py as f32 - vp_y) / vp_h * self.game_h;
        if gx >= 0.0 && gx <= self.game_w && gy >= 0.0 && gy <= self.game_h {
            Some((gx, gy))
        } else {
            None
        }
    }

    /// Enable or disable post-processing for subsequent frames.
    /// Queue a postprocess active state change for the next frame (not the current one).
    pub fn set_postprocess_active(&mut self, active: bool) {
        self.pp_pending = Some(active);
    }

    /// Apply any pending postprocess state change before the next frame begins.
    fn apply_pending_pp(&mut self) {
        if let Some(active) = self.pp_pending.take() {
            self.pp_active = active;
        }
    }

    /// Begin a multi-batch frame. Call draw_batch() for each texture layer, then end_frame().
    pub fn begin_frame(&mut self) -> bool {
        // Apply deferred postprocess state change before acquiring surface
        self.apply_pending_pp();

        let output = match self.surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return false;
            }
            Err(e) => {
                log::error!("Surface error: {e}");
                return false;
            }
        };

        if self.pp_active {
            // Render scene to offscreen texture; surface is held for the post-process pass
            let surface_view = output.texture.create_view(&Default::default());
            self.surface_view_for_pp = Some(surface_view);
            self.surface_output_for_pp = Some(output);
            // Point frame_view at the offscreen target
            self.frame_view = Some(self.postprocess.create_offscreen_view());
            self.frame_output = None; // Not using surface directly
        } else {
            let view = output.texture.create_view(&Default::default());
            self.frame_output = Some(output);
            self.frame_view = Some(view);
            self.surface_view_for_pp = None;
            self.surface_output_for_pp = None;
        }
        self.frame_cleared = false;
        true
    }

    /// Draw accumulated vertices with the given texture (or white if None), then clear the batch.
    pub fn draw_batch(&mut self, texture: Option<&GpuTexture>) {
        if self.vertices.is_empty() {
            return;
        }
        let view = self.frame_view.as_ref().expect("call begin_frame first");
        let tex_bind_group = texture
            .map(|t| &t.bind_group)
            .unwrap_or(&self.white_texture.bind_group);

        let load_op = if self.frame_cleared {
            wgpu::LoadOp::Load
        } else {
            self.frame_cleared = true;
            wgpu::LoadOp::Clear(wgpu::Color::BLACK)
        };

        self.queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&self.vertices));
        self.queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&self.indices));

        let (vp_x, vp_y, vp_w, vp_h) = if self.pp_active {
            (0.0, 0.0, self.game_w, self.game_h)
        } else {
            self.viewport_rect()
        };

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Batch Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: load_op, store: wgpu::StoreOp::Store },
                    depth_slice: None,
                })],
                ..Default::default()
            });
            pass.set_viewport(vp_x, vp_y, vp_w, vp_h, 0.0, 1.0);
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.proj_bind_group, &[]);
            pass.set_bind_group(1, tex_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.indices.len() as u32, 0, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        self.vertices.clear();
        self.indices.clear();
    }

    /// Draw accumulated vertices with a raw wgpu bind group (e.g. for video textures).
    pub fn draw_batch_with_bind_group(&mut self, bind_group: &wgpu::BindGroup) {
        if self.vertices.is_empty() {
            return;
        }
        let view = self.frame_view.as_ref().expect("call begin_frame first");

        let load_op = if self.frame_cleared {
            wgpu::LoadOp::Load
        } else {
            self.frame_cleared = true;
            wgpu::LoadOp::Clear(wgpu::Color::BLACK)
        };

        self.queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&self.vertices));
        self.queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&self.indices));

        let (vp_x, vp_y, vp_w, vp_h) = if self.pp_active {
            (0.0, 0.0, self.game_w, self.game_h)
        } else {
            self.viewport_rect()
        };

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Batch Pass (bind group)"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: load_op, store: wgpu::StoreOp::Store },
                    depth_slice: None,
                })],
                ..Default::default()
            });
            pass.set_viewport(vp_x, vp_y, vp_w, vp_h, 0.0, 1.0);
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.proj_bind_group, &[]);
            pass.set_bind_group(1, bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.indices.len() as u32, 0, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        self.vertices.clear();
        self.indices.clear();
    }

    /// Finish the multi-batch frame: draw any remaining colored quads, render text, present.
    pub fn end_frame(&mut self) {
        let view = self.frame_view.take().expect("call begin_frame first");

        let load_op = if self.frame_cleared {
            wgpu::LoadOp::Load
        } else {
            self.frame_cleared = true;
            wgpu::LoadOp::Clear(wgpu::Color::BLACK)
        };

        // Upload remaining colored quads before creating render pass
        if !self.vertices.is_empty() {
            self.queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&self.vertices));
            self.queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&self.indices));
        }

        let (vp_x, vp_y, vp_w, vp_h) = if self.pp_active {
            // When rendering to offscreen, use full texture as viewport
            (0.0, 0.0, self.game_w, self.game_h)
        } else {
            self.viewport_rect()
        };

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Final Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: load_op, store: wgpu::StoreOp::Store },
                    depth_slice: None,
                })],
                ..Default::default()
            });
            pass.set_viewport(vp_x, vp_y, vp_w, vp_h, 0.0, 1.0);

            if !self.vertices.is_empty() {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.proj_bind_group, &[]);
                pass.set_bind_group(1, &self.white_texture.bind_group, &[]);
                pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.indices.len() as u32, 0, 0..1);
                self.vertices.clear();
                self.indices.clear();
            }

            self.text_system.render(
                &self.device, &self.queue, &mut pass,
                self.game_w, self.game_h, vp_w, vp_h,
            );
        }
        self.queue.submit(std::iter::once(encoder.finish()));

        if self.pp_active {
            // Apply post-processing: offscreen → surface
            if let Some(surface_view) = self.surface_view_for_pp.take() {
                let viewport = self.viewport_rect();
                self.postprocess.update_uniforms(&self.queue);
                self.postprocess.apply(&self.device, &self.queue, &surface_view, viewport);
                if let Some(output) = self.surface_output_for_pp.take() {
                    output.present();
                }
            }
        } else {
            if let Some(output) = self.frame_output.take() {
                output.present();
            }
        }
    }

    /// Draw a sub-region of a texture as a quad. UV coords are in pixels, converted internally.
    pub fn push_texture_region(
        &mut self, tex_w: f32, tex_h: f32,
        src_x: f32, src_y: f32, src_w: f32, src_h: f32,
        dst_x: f32, dst_y: f32, dst_w: f32, dst_h: f32,
        flip_x: bool,
        color: [f32; 4],
    ) {
        let u0 = src_x / tex_w;
        let v0 = src_y / tex_h;
        let u1 = (src_x + src_w) / tex_w;
        let v1 = (src_y + src_h) / tex_h;
        let (ul, ur) = if flip_x { (u1, u0) } else { (u0, u1) };
        self.push_quad(
            dst_x, dst_y, dst_w, dst_h,
            ul, v0, ur, v0, ur, v1, ul, v1,
            color,
        );
    }
}

fn ortho_projection(w: f32, h: f32) -> Projection {
    // Maps (0,0)-(w,h) to clip space with Y going down (screen convention)
    Projection {
        matrix: [
            [2.0 / w, 0.0, 0.0, 0.0],
            [0.0, -2.0 / h, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0, 1.0],
        ],
    }
}

