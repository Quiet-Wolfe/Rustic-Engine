/// WGSL shader for the sprite pipeline: textured quads with per-vertex color and alpha blending.
pub const SHADER_SRC: &str = r#"
struct Projection {
    matrix: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> proj: Projection;

@group(1) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(1) @binding(1)
var s_diffuse: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = proj.matrix * vec4<f32>(in.position, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let c = textureSample(t_diffuse, s_diffuse, in.uv) * in.color;
    return vec4<f32>(c.rgb * c.a, c.a);
}
"#;
