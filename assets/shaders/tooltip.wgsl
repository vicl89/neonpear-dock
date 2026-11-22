struct TooltipUniforms {
    rect: vec4<f32>,       // x, y, ancho, alto en píxeles físicos
    resolution: vec2<f32>, // ancho, alto del framebuffer
    opacity: f32,
    _pad: f32,
};

@group(0) @binding(0)
var<uniform> u: TooltipUniforms;

@group(0) @binding(1)
var tooltip_tex: texture_2d<f32>;

@group(0) @binding(2)
var tooltip_sampler: sampler;

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0)
    );

    let uv = corners[idx];
    let px = u.rect.xy + uv * u.rect.zw;
    let ndc = vec2<f32>(
        (px.x / max(u.resolution.x, 1.0)) * 2.0 - 1.0,
        1.0 - (px.y / max(u.resolution.y, 1.0)) * 2.0
    );

    var out: VertexOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let sample_col = textureSample(tooltip_tex, tooltip_sampler, in.uv);
    return sample_col * u.opacity;
}
