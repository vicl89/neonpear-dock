use bytemuck::{Pod, Zeroable};
use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Weight};
use dock_core::menu::{MenuLayout, MENU_ARROW_HEIGHT, MENU_PAD_X, MENU_SEPARATOR_HEIGHT};
use wgpu::util::DeviceExt;

const MENU_SHADER: &str = r#"
struct MenuUniforms {
    rect: vec4<f32>,
    resolution: vec2<f32>,
    opacity: f32,
    hover_radius: f32,
    hover_rect: vec4<f32>,
    hover_color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: MenuUniforms;
@group(0) @binding(1) var menu_tex: texture_2d<f32>;
@group(0) @binding(2) var menu_samp: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
    );
    let uv = corners[idx];
    let px = u.rect.xy + uv * u.rect.zw;
    let ndc = vec2<f32>(
        (px.x / max(u.resolution.x, 1.0)) * 2.0 - 1.0,
        1.0 - (px.y / max(u.resolution.y, 1.0)) * 2.0,
    );
    var out: VsOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = uv;
    return out;
}

fn sd_rounded_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r, r);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0, 0.0))) - r;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var col = textureSample(menu_tex, menu_samp, in.uv);

    if (u.hover_rect.z > 0.0 && u.hover_rect.w > 0.0 && u.hover_color.a > 0.001) {
        let local_px = in.uv * u.rect.zw;
        let center = u.hover_rect.xy + u.hover_rect.zw * 0.5;
        let half_size = u.hover_rect.zw * 0.5;
        let d = sd_rounded_box(local_px - center, half_size, u.hover_radius);
        let mask = 1.0 - smoothstep(-0.75, 0.75, d);
        if (mask > 0.001) {
            let sa = u.hover_color.a * mask;
            let inv = 1.0 - sa;
            col = vec4<f32>(
                u.hover_color.rgb * sa + col.rgb * inv,
                sa + col.a * inv,
            );
        }
    }

    return col * u.opacity;
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MenuUniforms {
    rect: [f32; 4],
    resolution: [f32; 2],
    opacity: f32,
    hover_radius: f32,
    hover_rect: [f32; 4],
    hover_color: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MenuCacheKey {
    items: Vec<(String, bool, bool, bool)>,
    width_px: u32,
    box_height_px: u32,
    arrow_rel_x_px: i32,
    dark: bool,
    glass_active: bool,
    scale_key: u32,
}

struct CachedMenu {
    key: MenuCacheKey,
    _texture: wgpu::Texture,
    _view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    width_px: f32,
    height_px: f32,
    shadow_pad: f32,
}

pub struct MenuPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    cached: Option<CachedMenu>,
}

impl MenuPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let init_uniform = MenuUniforms {
            rect: [0.0; 4],
            resolution: [1.0, 1.0],
            opacity: 0.0,
            hover_radius: 0.0,
            hover_rect: [0.0; 4],
            hover_color: [0.0; 4],
        };

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("menu-uniform"),
            contents: bytemuck::bytes_of(&init_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("menu-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("menu-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("menu-shader"),
            source: wgpu::ShaderSource::Wgsl(MENU_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("menu-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("menu-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            uniform_buffer,
            bind_group_layout,
            sampler,
            cached: None,
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        resolution: [f32; 2],
        is_dark: f32,
        glass_active: bool,
        scale_factor: f32,
        menu: Option<&MenuLayout>,
    
        font_system: &mut FontSystem,
        swash_cache: &mut SwashCache,
    ) -> bool {
        let Some(layout) = menu else {
            return false;
        };

        if layout.opacity <= 0.005 || layout.items.is_empty() {
            return false;
        }

        let s = scale_factor.max(1.0);
        let dark = is_dark >= 0.5;
        let box_w_px = (layout.width * s).round().max(1.0) as u32;
        let box_h_px = (layout.box_height * s).round().max(1.0) as u32;
        let arrow_rel_x_px = ((layout.arrow_x - layout.x) * s).round() as i32;

        let key = MenuCacheKey {
            items: layout
                .items
                .iter()
                .map(|it| (it.label.clone(), it.enabled, it.destructive, it.separator_before))
                .collect(),
            width_px: box_w_px,
            box_height_px: box_h_px,
            arrow_rel_x_px,
            dark,
            glass_active,
            scale_key: (s * 100.0).round() as u32,
        };

        let needs_rebuild = self
            .cached
            .as_ref()
            .map(|c| c.key != key)
            .unwrap_or(true);

        if needs_rebuild {
            let raster = rasterize_menu(
                font_system,
                swash_cache,
                layout,
                dark,
                glass_active,
                s,
            );

            let tex_size = wgpu::Extent3d {
                width: raster.width,
                height: raster.height,
                depth_or_array_layers: 1,
            };

            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("menu-texture"),
                size: tex_size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &raster.rgba_premul,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * raster.width),
                    rows_per_image: Some(raster.height),
                },
                tex_size,
            );

            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("menu-bg"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });

            self.cached = Some(CachedMenu {
                key,
                _texture: texture,
                _view: view,
                bind_group,
                width_px: raster.width as f32,
                height_px: raster.height as f32,
                shadow_pad: raster.shadow_pad,
            });
        }

        let Some(cached) = &self.cached else {
            return false;
        };

        // Ligero desplazamiento vertical al aparecer/desaparecer para sensación elástica
        let lift = (1.0 - layout.opacity) * 5.0 * s;
        let x = (layout.x * s - cached.shadow_pad).round();
        let y = (layout.y * s - cached.shadow_pad + lift).round();

        let (hover_rect, hover_color) = if let Some(h_idx) = layout.hovered_item {
            if let (Some(item), Some(rect)) = (layout.items.get(h_idx), layout.item_rects.get(h_idx)) {
                if item.enabled {
                    let rx0 = cached.shadow_pad + (rect[0] - layout.x) * s;
                    let ry0 = cached.shadow_pad + (rect[1] - layout.y) * s;
                    let rw = rect[2] * s;
                    let rh = rect[3] * s;
                    let color = if item.destructive {
                        if dark {
                            [0.95, 0.28, 0.32, 0.26]
                        } else {
                            [0.88, 0.18, 0.22, 0.18]
                        }
                    } else if dark {
                        [1.0, 1.0, 1.0, 0.14]
                    } else {
                        [0.0, 0.0, 0.0, 0.09]
                    };
                    ([rx0, ry0, rw, rh], color)
                } else {
                    ([0.0; 4], [0.0; 4])
                }
            } else {
                ([0.0; 4], [0.0; 4])
            }
        } else {
            ([0.0; 4], [0.0; 4])
        };

        let uniform = MenuUniforms {
            rect: [x, y, cached.width_px, cached.height_px],
            resolution,
            opacity: layout.opacity,
            hover_radius: 7.0 * s,
            hover_rect,
            hover_color,
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
        true
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, visible: bool) {
        if !visible {
            return;
        }
        let Some(cached) = &self.cached else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &cached.bind_group, &[]);
        pass.draw(0..6, 0..1);
    }
}

struct RasterizedMenu {
    width: u32,
    height: u32,
    shadow_pad: f32,
    rgba_premul: Vec<u8>,
}

fn rasterize_menu(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    layout: &MenuLayout,
    dark: bool,
    glass_active: bool,
    scale: f32,
) -> RasterizedMenu {
    let box_w = (layout.width * scale).round().max(1.0);
    let box_h = (layout.box_height * scale).round().max(1.0);
    let arrow_h = (MENU_ARROW_HEIGHT * scale).round().max(1.0);
    let arrow_half_w = 6.5 * scale;
    let radius = 12.0 * scale;
    let _item_radius = 7.0 * scale;
    let shadow_pad = (6.0 * scale).ceil();
    let border_w = (1.0 * scale).max(1.0);

    let width = (box_w + shadow_pad * 2.0).ceil() as u32;
    let height = (box_h + arrow_h + shadow_pad * 2.0).ceil() as u32;

    let mut canvas = vec![[0.0f32; 4]; (width * height) as usize];

    let bx0 = shadow_pad;
    let by0 = shadow_pad;
    let bx1 = bx0 + box_w;
    let by1 = by0 + box_h;
    let center = [(bx0 + bx1) * 0.5, (by0 + by1) * 0.5];
    let half_size = [box_w * 0.5, box_h * 0.5];

    let arrow_rel_x = ((layout.arrow_x - layout.x) * scale)
        .clamp(16.0 * scale, (box_w - 16.0 * scale).max(16.0 * scale));
    let ax = bx0 + arrow_rel_x;
    let p0 = [ax - arrow_half_w, by1 - 1.0 * scale];
    let p1 = [ax + arrow_half_w, by1 - 1.0 * scale];
    let p2 = [ax, by1 + arrow_h];

    let (fill_col, border_col, sep_col, _hover_col, _hover_destructive_col) = if dark {
        let bg_alpha = if glass_active { 0.78 } else { 0.94 };
        (
            [0.094, 0.094, 0.145, bg_alpha], // Catppuccin Mocha Mantle (#181825)
            [1.0, 1.0, 1.0, 0.16],
            [1.0, 1.0, 1.0, 0.11],
            [0.796, 0.651, 0.969, 0.24],     // Mauve translúcido
            [0.953, 0.545, 0.659, 0.22],     // Red suave
        )
    } else {
        let bg_alpha = if glass_active { 0.84 } else { 0.96 };
        (
            [0.965, 0.969, 0.984, bg_alpha],
            [0.0, 0.0, 0.0, 0.11],
            [0.0, 0.0, 0.0, 0.09],
            [0.24, 0.48, 0.94, 0.14],
            [0.86, 0.20, 0.27, 0.13],
        )
    };

    let aa = (0.75 * scale).max(0.75);
    let shadow_shift = 2.0 * scale;
    let shadow_spread = 5.0 * scale;

    // 1. Fondo de tarjeta + flecha + sombra + borde
    for py in 0..height {
        for px in 0..width {
            let p = [px as f32 + 0.5, py as f32 + 0.5];
            let d_box = sd_rounded_box([p[0] - center[0], p[1] - center[1]], half_size, radius);
            let d_tri = sd_triangle(p, p0, p1, p2);
            let d = d_box.min(d_tri);

            let mut dst = [0.0f32; 4];

            let ps = [p[0], p[1] - shadow_shift];
            let ds_box =
                sd_rounded_box([ps[0] - center[0], ps[1] - center[1]], half_size, radius);
            let ds_tri = sd_triangle(ps, p0, p1, p2);
            let ds = ds_box.min(ds_tri);
            if ds < shadow_spread {
                let s_alpha = (1.0 - smoothstep(0.0, shadow_spread, ds.max(0.0))) * 0.30;
                dst = blend_over(dst, [0.0, 0.0, 0.0, s_alpha]);
            }

            let mask = 1.0 - smoothstep(-aa, aa, d);
            if mask > 0.001 {
                dst = blend_over(
                    dst,
                    [fill_col[0], fill_col[1], fill_col[2], fill_col[3] * mask],
                );
                let border_mask = smoothstep(-border_w - aa, -border_w + aa, d) * mask;
                if border_mask > 0.001 {
                    dst = blend_over(
                        dst,
                        [
                            border_col[0],
                            border_col[1],
                            border_col[2],
                            border_col[3] * border_mask,
                        ],
                    );
                }
            }

            canvas[(py * width + px) as usize] = dst;
        }
    }

    // 2. Separadores horizontales
    let sep_x0 = (bx0 + (MENU_PAD_X + 6.0) * scale).round().max(0.0) as u32;
    let sep_x1 = (bx1 - (MENU_PAD_X + 6.0) * scale)
        .round()
        .min(width as f32) as u32;
    let sep_h = (1.0 * scale).round().max(1.0) as u32;

    for (i, item) in layout.items.iter().enumerate() {
        if i == 0 || !item.separator_before {
            continue;
        }
        let Some(rect) = layout.item_rects.get(i) else {
            continue;
        };
        let item_y = shadow_pad + (rect[1] - layout.y) * scale;
        let sep_y = (item_y - MENU_SEPARATOR_HEIGHT * scale * 0.5 - sep_h as f32 * 0.5).round();
        let y0 = sep_y.max(0.0) as u32;
        let y1 = (y0 + sep_h).min(height);
        for py in y0..y1 {
            for px in sep_x0..sep_x1 {
                let idx = (py * width + px) as usize;
                canvas[idx] = blend_over(canvas[idx], sep_col);
            }
        }
    }

    // 4. Etiquetas de texto con cosmic-text
    let font_size = 13.0 * scale;
    let line_height = (17.0 * scale).round();
    let metrics = Metrics::new(font_size, line_height);

    for (i, item) in layout.items.iter().enumerate() {
        let Some(rect) = layout.item_rects.get(i) else {
            continue;
        };
        let rx0 = shadow_pad + (rect[0] - layout.x) * scale;
        let ry0 = shadow_pad + (rect[1] - layout.y) * scale;
        let rw = rect[2] * scale;
        let rh = rect[3] * scale;
        let text_pad_x = 10.0 * scale;
        let clip_x1 = (rx0 + rw - text_pad_x).floor() as i32;

        let text_color = if !item.enabled {
            if dark {
                Color::rgba(205, 214, 244, 96)
            } else {
                Color::rgba(28, 30, 42, 92)
            }
        } else if item.destructive {
            if dark {
                Color::rgba(243, 139, 168, 255) // Catppuccin Mocha Red (#f38ba8)
            } else {
                Color::rgba(210, 35, 58, 255)
            }
        } else if dark {
            Color::rgba(205, 214, 244, 255) // Catppuccin Mocha Text (#cdd6f4)
        } else {
            Color::rgba(28, 30, 42, 255)
        };

        let mut buffer = Buffer::new(font_system, metrics);
        buffer.set_size(font_system, Some((rw - text_pad_x * 2.0).max(20.0)), Some(line_height));
        let attrs = Attrs::new().family(Family::SansSerif).weight(Weight::MEDIUM);
        buffer.set_text(font_system, &item.label, attrs, Shaping::Advanced);
        buffer.shape_until_scroll(font_system, false);

        let ox = (rx0 + text_pad_x).round() as i32;
        let oy = (ry0 + (rh - line_height) * 0.5).round() as i32;

        buffer.draw(font_system, swash_cache, text_color, |gx, gy, gw, gh, col| {
            let a = col.a() as f32 / 255.0;
            if a <= 0.001 {
                return;
            }
            let src = [
                col.r() as f32 / 255.0,
                col.g() as f32 / 255.0,
                col.b() as f32 / 255.0,
                a,
            ];
            for dy in 0..gh {
                let py = oy + gy + dy as i32;
                if py < 0 || py >= height as i32 {
                    continue;
                }
                for dx in 0..gw {
                    let px = ox + gx + dx as i32;
                    if px < 0 || px >= clip_x1 || px >= width as i32 {
                        continue;
                    }
                    let idx = (py as u32 * width + px as u32) as usize;
                    canvas[idx] = blend_over(canvas[idx], src);
                }
            }
        });
    }

    let mut rgba_premul = Vec::with_capacity((width * height * 4) as usize);
    for px in canvas {
        rgba_premul.push((px[0].clamp(0.0, 1.0) * 255.0).round() as u8);
        rgba_premul.push((px[1].clamp(0.0, 1.0) * 255.0).round() as u8);
        rgba_premul.push((px[2].clamp(0.0, 1.0) * 255.0).round() as u8);
        rgba_premul.push((px[3].clamp(0.0, 1.0) * 255.0).round() as u8);
    }

    RasterizedMenu {
        width,
        height,
        shadow_pad,
        rgba_premul,
    }
}

fn blend_over(dst_pm: [f32; 4], src_straight: [f32; 4]) -> [f32; 4] {
    let sa = src_straight[3];
    let inv = 1.0 - sa;
    [
        src_straight[0] * sa + dst_pm[0] * inv,
        src_straight[1] * sa + dst_pm[1] * inv,
        src_straight[2] * sa + dst_pm[2] * inv,
        sa + dst_pm[3] * inv,
    ]
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(1e-5)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn sd_rounded_box(p: [f32; 2], b: [f32; 2], r: f32) -> f32 {
    let qx = p[0].abs() - b[0] + r;
    let qy = p[1].abs() - b[1] + r;
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    qx.max(qy).min(0.0) + outside - r
}

fn dot2(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

fn sub2(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn sd_triangle(p: [f32; 2], p0: [f32; 2], p1: [f32; 2], p2: [f32; 2]) -> f32 {
    let e0 = sub2(p1, p0);
    let e1 = sub2(p2, p1);
    let e2 = sub2(p0, p2);

    let v0 = sub2(p, p0);
    let v1 = sub2(p, p1);
    let v2 = sub2(p, p2);

    let t0 = (dot2(v0, e0) / dot2(e0, e0)).clamp(0.0, 1.0);
    let t1 = (dot2(v1, e1) / dot2(e1, e1)).clamp(0.0, 1.0);
    let t2 = (dot2(v2, e2) / dot2(e2, e2)).clamp(0.0, 1.0);

    let pq0 = [v0[0] - e0[0] * t0, v0[1] - e0[1] * t0];
    let pq1 = [v1[0] - e1[0] * t1, v1[1] - e1[1] * t1];
    let pq2 = [v2[0] - e2[0] * t2, v2[1] - e2[1] * t2];

    let s = (e0[0] * e2[1] - e0[1] * e2[0]).signum();
    let d0 = [dot2(pq0, pq0), s * (v0[0] * e0[1] - v0[1] * e0[0])];
    let d1 = [dot2(pq1, pq1), s * (v1[0] * e1[1] - v1[1] * e1[0])];
    let d2 = [dot2(pq2, pq2), s * (v2[0] * e2[1] - v2[1] * e2[0])];

    let d_dist_sq = d0[0].min(d1[0]).min(d2[0]);
    let d_sign = d0[1].min(d1[1]).min(d2[1]);
    -d_dist_sq.sqrt() * d_sign.signum()
}
