use std::path::Path;
use std::sync::Arc;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use winit::window::Window;

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
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    proj_buffer: wgpu::Buffer,
    proj_bind_group: wgpu::BindGroup,
    pub texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
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
}

impl GpuState {
    pub async fn new(window: Arc<Window>, game_w: f32, game_h: f32) -> Self {
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

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("RusticV2"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
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
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
            vertices: Vec::with_capacity(MAX_VERTICES),
            indices: Vec::with_capacity(MAX_INDICES),
            white_texture,
            text_system,
            game_w,
            game_h,
            frame_output: None,
            frame_view: None,
            frame_cleared: false,
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn load_texture_from_path(&self, path: &Path) -> GpuTexture {
        let img = image::open(path)
            .unwrap_or_else(|e| panic!("Failed to load image {:?}: {}", path, e))
            .to_rgba8();
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

    fn push_quad(
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

    /// Begin a multi-batch frame. Call draw_batch() for each texture layer, then end_frame().
    pub fn begin_frame(&mut self) -> bool {
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
        let view = output.texture.create_view(&Default::default());
        self.frame_output = Some(output);
        self.frame_view = Some(view);
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

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let (vp_x, vp_y, vp_w, vp_h) = self.viewport_rect();
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

    /// Finish the multi-batch frame: draw any remaining colored quads, render text, present.
    pub fn end_frame(&mut self) {
        let view = self.frame_view.take().expect("call begin_frame first");
        let output = self.frame_output.take().expect("call begin_frame first");

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

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let (vp_x, vp_y, vp_w, vp_h) = self.viewport_rect();
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
        output.present();
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

