struct DotGlobals {
    resolution: vec2<f32>,
    is_dark: f32,
    scale_factor: f32,
};

@group(0) @binding(0)
var<uniform> g: DotGlobals;

struct DotInstance {
    @location(0) center: vec2<f32>,
    @location(1) radius: f32,
    @location(2) opacity: f32,
};

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) center: vec2<f32>,
    @location(1) radius: f32,
    @location(2) opacity: f32,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32, inst: DotInstance) -> VertexOut {
    var offsets = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0,  1.0)
    );

    let half_extent = inst.radius + max(1.5, g.scale_factor * 2.0);
    let px = inst.center + offsets[idx] * half_extent;
    let ndc = vec2<f32>(
        (px.x / max(g.resolution.x, 1.0)) * 2.0 - 1.0,
        1.0 - (px.y / max(g.resolution.y, 1.0)) * 2.0
    );

    var out: VertexOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.center = inst.center;
    out.radius = inst.radius;
    out.opacity = inst.opacity;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let dist = length(in.pos.xy - in.center) - in.radius;
    let aa = max(1.0, g.scale_factor) * 0.75;
    let mask = 1.0 - smoothstep(-aa, aa, dist);
    if mask <= 0.001 || in.opacity <= 0.001 {
        discard;
    }

    // Punto oscuro sobre vidrio claro, punto claro sobre vidrio oscuro
    let light_dot = vec4<f32>(0.07, 0.08, 0.10, 0.88);
    let dark_dot  = vec4<f32>(0.94, 0.96, 1.00, 0.90);
    let col = mix(light_dot, dark_dot, g.is_dark);

    let a = col.a * mask * in.opacity;
    return vec4<f32>(col.rgb * a, a);
}
