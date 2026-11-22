struct IconUniforms {
    rect: vec4<f32>,       // x, y, ancho, alto en píxeles físicos
    resolution: vec2<f32>, // resolución total de la superficie
    pressed: f32,          // 0.0 = normal, 1.0 = clic mantenido
    opacity: f32,
};

@group(0) @binding(0)
var<uniform> u: IconUniforms;

@group(0) @binding(1)
var icon_tex: texture_2d<f32>;

@group(0) @binding(2)
var icon_sampler: sampler;

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
    var col = textureSample(icon_tex, icon_sampler, in.uv);
    // Las texturas ya vienen con alfa premultiplicado; atenuar RGB mantiene intacta la transparencia
    let shade = 1.0 - u.pressed * 0.28;
    return vec4<f32>(col.rgb * shade, col.a) * u.opacity;
}
