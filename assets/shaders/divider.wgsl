struct DividerGlobals {
    resolution: vec2<f32>,
    is_dark: f32,
    scale_factor: f32,
};

@group(0) @binding(0)
var<uniform> g: DividerGlobals;

struct DividerInstance {
    @location(0) rect: vec4<f32>, // x, y, ancho, alto en píxeles físicos
};

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) center: vec2<f32>,
    @location(1) half_size: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32, inst: DividerInstance) -> VertexOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0)
    );

    let pad = max(1.5, g.scale_factor * 1.5);
    let min_pt = inst.rect.xy - vec2<f32>(pad);
    let size = inst.rect.zw + vec2<f32>(pad * 2.0);
    let px = min_pt + corners[idx] * size;

    let ndc = vec2<f32>(
        (px.x / max(g.resolution.x, 1.0)) * 2.0 - 1.0,
        1.0 - (px.y / max(g.resolution.y, 1.0)) * 2.0
    );

    var out: VertexOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.center = inst.rect.xy + inst.rect.zw * 0.5;
    out.half_size = inst.rect.zw * 0.5;
    return out;
}

fn sd_rounded_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - r;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let r = min(in.half_size.x, in.half_size.y);
    let dist = sd_rounded_box(in.pos.xy - in.center, in.half_size, r);
    let aa = max(0.75, g.scale_factor * 0.75);
    let mask = 1.0 - smoothstep(-aa, aa, dist);
    if mask <= 0.001 {
        discard;
    }

    let light_col = vec4<f32>(0.05, 0.06, 0.08, 0.22);
    let dark_col  = vec4<f32>(1.00, 1.00, 1.00, 0.24);
    let col = mix(light_col, dark_col, g.is_dark);

    let a = col.a * mask;
    return vec4<f32>(col.rgb * a, a);
}
