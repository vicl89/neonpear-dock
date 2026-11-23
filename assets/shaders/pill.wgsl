struct PillUniforms {
    resolution: vec2<f32>,
    pill_center: vec2<f32>,
    pill_size: vec2<f32>,
    corner_radius: f32,
    is_dark: f32,
    scale_factor: f32,
    blur_active: f32,
    opacity: f32,
    border_strength: f32,
    custom_tint: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> u: PillUniforms;

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0)
    );
    var out: VertexOut;
    out.pos = vec4<f32>(positions[idx], 0.0, 1.0);
    return out;
}

fn sd_rounded_box(p: vec2<f32>, half_size: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half_size + vec2<f32>(r);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - r;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let p = in.pos.xy - u.pill_center;
    let half_size = u.pill_size * 0.5;

    let dist = sd_rounded_box(p, half_size, u.corner_radius);
    let aa = max(1.0, u.scale_factor);

    let fill_mask = 1.0 - smoothstep(-aa * 0.5, aa * 0.5, dist);
    if fill_mask <= 0.001 {
        discard;
    }

    let border_width = 1.0 * u.scale_factor;
    let border_dist = abs(dist + border_width * 0.5) - border_width * 0.5;
    let border_mask = (1.0 - smoothstep(-aa * 0.5, aa * 0.5, border_dist)) * fill_mask;

    let auto_rgb = mix(vec3<f32>(0.95, 0.97, 1.00), vec3<f32>(0.10, 0.11, 0.14), u.is_dark);
    let base_rgb = mix(auto_rgb, u.custom_tint.rgb, u.custom_tint.a);
    let base_alpha = clamp(u.opacity, 0.0, 1.0);
    let base_fill = vec4<f32>(base_rgb, base_alpha);

    // Reflejo especular relativo al borde superior real de la píldora
    let pill_top = u.pill_center.y - half_size.y;
    let top_glow = clamp(1.0 - ((in.pos.y - pill_top) / max(u.pill_size.y, 1.0)), 0.0, 1.0);
    let light_border = vec4<f32>(1.0, 1.0, 1.0, mix(0.40, 0.85, top_glow));
    let dark_border  = vec4<f32>(1.0, 1.0, 1.0, mix(0.12, 0.30, top_glow));
    var border_col   = mix(light_border, dark_border, u.is_dark);
    border_col.a     = clamp(border_col.a * u.border_strength, 0.0, 1.0);

    let rgb = mix(base_fill.rgb, border_col.rgb, border_mask * border_col.a);
    let a = clamp(base_fill.a + border_mask * border_col.a * (1.0 - base_fill.a), 0.0, 1.0) * fill_mask;

    return vec4<f32>(rgb * a, a);
}
