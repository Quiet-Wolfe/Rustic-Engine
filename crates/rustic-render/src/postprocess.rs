//! Post-processing pipeline: render scene to offscreen texture, then apply fullscreen effects.
//!
//! Supports both built-in effects (VCR/CRT) and runtime-loaded GLSL shaders from mods.

use std::collections::HashMap;
use wgpu::util::DeviceExt;
use bytemuck::{Pod, Zeroable};

/// Uniform data passed to every post-processing shader.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct PostProcessUniforms {
    /// Screen resolution (width, height) in pixels.
    pub resolution: [f32; 2],
    /// Elapsed time in seconds (for animated effects).
    pub time: f32,
    /// VCR scanline intensity (0 = off).
    pub scanline_intensity: f32,
    /// NTSC distortion multiplier (0 = off).
    pub distortion_mult: f32,
    /// Chromatic aberration strength (0 = off).
    pub chromatic_aberration: f32,
    /// Vignette darkening at edges (0 = off).
    pub vignette_intensity: f32,
    /// Master enable flag (0 = passthrough, 1 = apply effects).
    pub enabled: u32,
}

impl Default for PostProcessUniforms {
    fn default() -> Self {
        Self {
            resolution: [1280.0, 720.0],
            time: 0.0,
            scanline_intensity: 0.0,
            distortion_mult: 0.0,
            chromatic_aberration: 0.0,
            vignette_intensity: 0.0,
            enabled: 0,
        }
    }
}

/// The fullscreen vertex shader — generates a full-screen triangle from vertex_index alone.
const FULLSCREEN_VERT: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Full-screen triangle trick: 3 vertices cover the entire screen
    var out: VertexOutput;
    let x = f32(i32(vertex_index & 1u) * 4 - 1);
    let y = f32(i32(vertex_index & 2u) * 2 - 1);
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    // UV: (0,0) top-left to (1,1) bottom-right (flip Y for wgpu)
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}
"#;

/// Built-in VCR/CRT post-processing fragment shader.
const VCR_FRAG: &str = r#"
struct Params {
    resolution: vec2<f32>,
    time: f32,
    scanline_intensity: f32,
    distortion_mult: f32,
    chromatic_aberration: f32,
    vignette_intensity: f32,
    enabled: u32,
};

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(1) @binding(0) var<uniform> params: Params;

// Simple hash-based noise
fn hash(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453);
}

// Barrel distortion
fn barrel_distort(uv: vec2<f32>, amt: f32) -> vec2<f32> {
    let centered = uv - 0.5;
    let r2 = dot(centered, centered);
    let distorted = centered * (1.0 + amt * r2);
    return distorted + 0.5;
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let original = textureSample(screen_texture, screen_sampler, uv);

    if (params.enabled == 0u) {
        return original;
    }

    // Barrel distortion
    let dist_uv = barrel_distort(uv, params.distortion_mult * 0.15);

    // Check bounds after distortion
    if (dist_uv.x < 0.0 || dist_uv.x > 1.0 || dist_uv.y < 0.0 || dist_uv.y > 1.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    // Chromatic aberration — offset R and B channels
    let ca = params.chromatic_aberration * 0.003;
    let dir = normalize(dist_uv - 0.5) * ca;
    let r = textureSample(screen_texture, screen_sampler, dist_uv + dir).r;
    let g = textureSample(screen_texture, screen_sampler, dist_uv).g;
    let b = textureSample(screen_texture, screen_sampler, dist_uv - dir).b;
    var color = vec3<f32>(r, g, b);

    // Scanlines
    let scan_freq = params.resolution.y * 0.75;
    let scan_line = sin(dist_uv.y * scan_freq * 3.14159) * 0.5 + 0.5;
    let scan_dark = 1.0 - params.scanline_intensity * 0.3 * (1.0 - scan_line);
    color *= scan_dark;

    // Rolling scanline band (VHS tracking artifact)
    let roll_pos = fract(params.time * 0.05);
    let roll_dist = abs(dist_uv.y - roll_pos);
    let roll_band = smoothstep(0.0, 0.02, roll_dist);
    color *= mix(0.92, 1.0, roll_band);

    // Noise/grain
    let noise_uv = dist_uv * params.resolution + vec2<f32>(params.time * 1000.0, 0.0);
    let grain = hash(noise_uv) * 0.06 - 0.03;
    color += grain * params.scanline_intensity;

    // Vignette
    let vig_centered = dist_uv - 0.5;
    let vig_dist = dot(vig_centered, vig_centered);
    let vig = 1.0 - vig_dist * params.vignette_intensity * 2.0;
    color *= clamp(vig, 0.0, 1.0);

    // Slight color bleed (horizontal smear, VHS-like)
    let bleed = textureSample(screen_texture, screen_sampler,
        dist_uv + vec2<f32>(1.5 / params.resolution.x, 0.0)).rgb;
    color = mix(color, bleed, 0.04 * params.chromatic_aberration);

    return vec4<f32>(color, 1.0);
}
"#;

/// Passthrough fragment shader — used when no post-processing is active.
const PASSTHROUGH_FRAG: &str = r#"
@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    return textureSample(screen_texture, screen_sampler, uv);
}
"#;

/// Post-processing system. Owns an offscreen render target and applies fullscreen effects.
pub struct PostProcessor {
    /// Offscreen texture the scene is rendered to.
    offscreen_texture: wgpu::Texture,
    offscreen_view: wgpu::TextureView,
    /// Bind group for the offscreen texture (used by post-process shader).
    texture_bind_group: wgpu::BindGroup,
    texture_bind_layout: wgpu::BindGroupLayout,
    /// Uniform buffer for shader parameters.
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    uniform_bind_layout: wgpu::BindGroupLayout,
    /// The VCR/CRT pipeline (built-in).
    vcr_pipeline: wgpu::RenderPipeline,
    /// Passthrough pipeline (when no effects active).
    passthrough_pipeline: wgpu::RenderPipeline,
    /// Runtime-loaded shader pipelines (name → pipeline).
    custom_pipelines: HashMap<String, wgpu::RenderPipeline>,
    /// Current shader parameters.
    pub uniforms: PostProcessUniforms,
    /// Which custom shader is active (empty = use built-in VCR or passthrough).
    pub active_shader: String,
    /// Sampler for screen texture.
    sampler: wgpu::Sampler,
    /// Surface format.
    format: wgpu::TextureFormat,
    /// Current offscreen dimensions.
    width: u32,
    height: u32,
}

impl PostProcessor {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Texture bind group layout: screen texture + sampler
        let texture_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("PostProcess Texture Layout"),
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

        // Uniform bind group layout
        let uniform_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("PostProcess Uniform Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let uniforms = PostProcessUniforms {
            resolution: [width as f32, height as f32],
            ..Default::default()
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("PostProcess Uniforms"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("PostProcess Uniform Bind Group"),
            layout: &uniform_bind_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Create offscreen render target
        let (offscreen_texture, offscreen_view) = create_offscreen_target(device, format, width, height);
        let texture_bind_group = create_texture_bind_group(
            device, &texture_bind_layout, &offscreen_view, &sampler,
        );

        // Build pipelines
        let vcr_pipeline = build_postprocess_pipeline(
            device, format, &texture_bind_layout, &uniform_bind_layout,
            &combine_shader(FULLSCREEN_VERT, VCR_FRAG), "VCR",
        );
        let passthrough_pipeline = build_postprocess_pipeline(
            device, format, &texture_bind_layout, &uniform_bind_layout,
            &combine_shader(FULLSCREEN_VERT, PASSTHROUGH_FRAG), "Passthrough",
        );

        Self {
            offscreen_texture,
            offscreen_view,
            texture_bind_group,
            texture_bind_layout,
            uniform_buffer,
            uniform_bind_group,
            uniform_bind_layout,
            vcr_pipeline,
            passthrough_pipeline,
            custom_pipelines: HashMap::new(),
            uniforms,
            active_shader: String::new(),
            sampler,
            format,
            width,
            height,
        }
    }

    /// Get the offscreen texture view to render the scene into.
    pub fn offscreen_view(&self) -> &wgpu::TextureView {
        &self.offscreen_view
    }

    /// Create a fresh view of the offscreen texture (for use as render target).
    pub fn create_offscreen_view(&self) -> wgpu::TextureView {
        self.offscreen_texture.create_view(&Default::default())
    }

    /// Resize the offscreen target if window/resolution changed.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == self.width && height == self.height {
            return;
        }
        self.width = width;
        self.height = height;
        self.uniforms.resolution = [width as f32, height as f32];

        let (tex, view) = create_offscreen_target(device, self.format, width, height);
        self.offscreen_texture = tex;
        self.offscreen_view = view;
        self.texture_bind_group = create_texture_bind_group(
            device, &self.texture_bind_layout, &self.offscreen_view, &self.sampler,
        );
    }

    /// Upload current uniforms to GPU.
    pub fn update_uniforms(&self, queue: &wgpu::Queue) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[self.uniforms]));
    }

    /// Apply post-processing: read from offscreen texture, write to the given surface view.
    pub fn apply(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_view: &wgpu::TextureView,
        viewport: (f32, f32, f32, f32), // (x, y, w, h)
    ) {
        let pipeline = if !self.active_shader.is_empty() {
            self.custom_pipelines.get(&self.active_shader)
                .unwrap_or(&self.passthrough_pipeline)
        } else if self.uniforms.enabled != 0 {
            &self.vcr_pipeline
        } else {
            &self.passthrough_pipeline
        };

        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("PostProcess Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                ..Default::default()
            });
            let (vp_x, vp_y, vp_w, vp_h) = viewport;
            pass.set_viewport(vp_x, vp_y, vp_w, vp_h, 0.0, 1.0);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.texture_bind_group, &[]);
            pass.set_bind_group(1, &self.uniform_bind_group, &[]);
            pass.draw(0..3, 0..1); // Fullscreen triangle
        }
        queue.submit(std::iter::once(encoder.finish()));
    }

    /// Load a GLSL fragment shader from source, convert to WGSL, and create a pipeline.
    /// Returns Ok(()) on success, Err with description on failure.
    pub fn load_glsl_shader(
        &mut self,
        device: &wgpu::Device,
        name: &str,
        frag_glsl: &str,
    ) -> Result<(), String> {
        let wgsl_frag = glsl_to_wgsl(frag_glsl)?;
        let combined = combine_shader(FULLSCREEN_VERT, &wgsl_frag);
        let pipeline = build_postprocess_pipeline(
            device, self.format, &self.texture_bind_layout, &self.uniform_bind_layout,
            &combined, name,
        );
        self.custom_pipelines.insert(name.to_string(), pipeline);
        Ok(())
    }

    /// Check if a custom shader is loaded.
    pub fn has_shader(&self, name: &str) -> bool {
        self.custom_pipelines.contains_key(name)
    }
}

/// Convert GLSL fragment shader source to WGSL using naga.
pub fn glsl_to_wgsl(glsl_src: &str) -> Result<String, String> {
    use naga::front::glsl;
    use naga::back::wgsl;

    let mut parser = glsl::Frontend::default();
    let options = glsl::Options::from(naga::ShaderStage::Fragment);
    let module = parser.parse(&options, glsl_src)
        .map_err(|errors| {
            let msgs: Vec<String> = errors.errors.iter().map(|e| format!("{e}")).collect();
            format!("GLSL parse errors: {}", msgs.join("; "))
        })?;

    // Validate
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .map_err(|e| format!("Shader validation error: {e}"))?;

    // Write WGSL
    let wgsl = wgsl::write_string(&module, &info, wgsl::WriterFlags::empty())
        .map_err(|e| format!("WGSL write error: {e}"))?;

    Ok(wgsl)
}

// --- Internal helpers ---

fn create_offscreen_target(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("PostProcess Offscreen"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    (texture, view)
}

fn create_texture_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("PostProcess Texture Bind Group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}

/// Combine vertex and fragment WGSL sources into one module.
fn combine_shader(vert: &str, frag: &str) -> String {
    format!("{vert}\n{frag}")
}

fn build_postprocess_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    texture_layout: &wgpu::BindGroupLayout,
    uniform_layout: &wgpu::BindGroupLayout,
    wgsl_src: &str,
    label: &str,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(wgsl_src.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&format!("{label} Pipeline Layout")),
        bind_group_layouts: &[texture_layout, uniform_layout],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(&format!("{label} Pipeline")),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[], // No vertex buffer — fullscreen triangle from vertex_index
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None, // Post-process writes final color directly
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
    })
}
