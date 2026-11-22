use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Weight};
use dock_core::physics::TooltipSlot;
use wgpu::util::DeviceExt;

const TOOLTIP_SHADER: &str = include_str!("../../../assets/shaders/tooltip.wgsl");
const MAX_CACHED_TOOLTIPS: usize = 64;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TooltipUniforms {
    rect: [f32; 4],
    resolution: [f32; 2],
    opacity: f32,
    _pad: f32,
}

struct CachedTooltip {
    _texture: wgpu::Texture,
    _view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    width_px: f32,
    height_px: f32,
    shadow_pad: f32,
}

pub struct TooltipPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    cache: HashMap<(String, bool), CachedTooltip>,
    cached_scale_key: u32,
    active_key: Option<(String, bool)>,
}

impl TooltipPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let init_uniform = TooltipUniforms {
            rect: [0.0; 4],
            resolution: [1.0, 1.0],
            opacity: 0.0,
            _pad: 0.0,
        };

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("tooltip-uniform"),
            contents: bytemuck::bytes_of(&init_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("tooltip-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tooltip-bgl"),
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
            label: Some("tooltip-shader"),
            source: wgpu::ShaderSource::Wgsl(TOOLTIP_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tooltip-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("tooltip-pipeline"),
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
            cache: HashMap::new(),
            cached_scale_key: 0,
            active_key: None,
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        resolution: [f32; 2],
        is_dark: f32,
        scale_factor: f32,
        tooltip: Option<&TooltipSlot>,
    
        font_system: &mut FontSystem,
        swash_cache: &mut SwashCache,
    ) -> bool {
        let Some(slot) = tooltip else {
            self.active_key = None;
            return false;
        };

        if slot.opacity <= 0.005 || slot.label.trim().is_empty() {
            self.active_key = None;
            return false;
        }

        let s = scale_factor.max(1.0);
        let scale_key = (s * 100.0).round() as u32;
        if self.cached_scale_key != scale_key {
            self.cache.clear();
            self.cached_scale_key = scale_key;
        }

        let dark = is_dark >= 0.5;
        let key = (slot.label.clone(), dark);

        if !self.cache.contains_key(&key) {
            if self.cache.len() >= MAX_CACHED_TOOLTIPS {
                self.cache.clear();
            }

            let raster = rasterize_tooltip(
                font_system,
                swash_cache,
                &slot.label,
                dark,
                s,
            );

            let tex_size = wgpu::Extent3d {
                width: raster.width,
                height: raster.height,
                depth_or_array_layers: 1,
            };

            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("tooltip-texture"),
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
                label: Some("tooltip-bg"),
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

            self.cache.insert(
                key.clone(),
                CachedTooltip {
                    _texture: texture,
                    _view: view,
                    bind_group,
                    width_px: raster.width as f32,
                    height_px: raster.height as f32,
                    shadow_pad: raster.shadow_pad,
                },
            );
        }

        let Some(cached) = self.cache.get(&key) else {
            self.active_key = None;
            return false;
        };

        let margin = 2.0 * s;
        let max_x = (resolution[0] - cached.width_px - margin).max(margin);
        let x = (slot.anchor_x * s - cached.width_px * 0.5).clamp(margin, max_x);
        let y = (slot.anchor_y * s - (cached.height_px - cached.shadow_pad)).max(margin);

        let uniform = TooltipUniforms {
            rect: [x.round(), y.round(), cached.width_px, cached.height_px],
            resolution,
            opacity: slot.opacity,
            _pad: 0.0,
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));

        self.active_key = Some(key);
        true
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, visible: bool) {
        if !visible {
            return;
        }
        let Some(key) = &self.active_key else {
            return;
        };
        let Some(cached) = self.cache.get(key) else {
            return;
        };

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &cached.bind_group, &[]);
        pass.draw(0..6, 0..1);
    }
}

struct RasterizedTooltip {
    width: u32,
    height: u32,
    shadow_pad: f32,
    rgba_premul: Vec<u8>,
}

fn rasterize_tooltip(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    label: &str,
    dark: bool,
    scale: f32,
) -> RasterizedTooltip {
    let font_size = 13.0 * scale;
    let line_height = (17.0 * scale).round();
    let metrics = Metrics::new(font_size, line_height);

    let mut buffer = Buffer::new(font_system, metrics);
    buffer.set_size(font_system, None, None);
    let attrs = Attrs::new().family(Family::SansSerif).weight(Weight::MEDIUM);
    buffer.set_text(font_system, label, attrs, Shaping::Advanced);
    buffer.shape_until_scroll(font_system, false);

    let mut text_w = 0.0f32;
    let mut lines = 0usize;
    for run in buffer.layout_runs() {
        text_w = text_w.max(run.line_w);
        lines += 1;
    }
    let text_w = text_w.ceil().max(12.0 * scale);
    let text_h = (lines.max(1) as f32 * line_height).ceil();

    let pad_x = (11.0 * scale).round();
    let pad_y = (5.0 * scale).round();
    let radius = 7.5 * scale;
    let arrow_h = (5.5 * scale).round();
    let arrow_half_w = 5.5 * scale;
    let shadow_pad = (4.0 * scale).ceil();
    let border_w = (1.0 * scale).max(1.0);

    let box_w = (text_w + pad_x * 2.0).ceil();
    let box_h = (text_h + pad_y * 2.0).ceil();

    let width = (box_w + shadow_pad * 2.0).ceil() as u32;
    let height = (box_h + arrow_h + shadow_pad * 2.0).ceil() as u32;

    let mut canvas = vec![[0.0f32; 4]; (width * height) as usize];

    let bx0 = shadow_pad;
    let by0 = shadow_pad;
    let bx1 = bx0 + box_w;
    let by1 = by0 + box_h;
    let center = [(bx0 + bx1) * 0.5, (by0 + by1) * 0.5];
    let half_size = [box_w * 0.5, box_h * 0.5];

    let p0 = [center[0] - arrow_half_w, by1 - 1.0 * scale];
    let p1 = [center[0] + arrow_half_w, by1 - 1.0 * scale];
    let p2 = [center[0], by1 + arrow_h];

    let (fill_col, border_col, text_color) = if dark {
        (
            [0.118, 0.118, 0.180, 0.92], // Catppuccin-friendly dark glass
            [1.0, 1.0, 1.0, 0.16],
            Color::rgba(235, 239, 250, 255),
        )
    } else {
        (
            [0.965, 0.969, 0.984, 0.95],
            [0.0, 0.0, 0.0, 0.12],
            Color::rgba(28, 30, 42, 255),
        )
    };

    let aa = (0.75 * scale).max(0.75);
    let shadow_shift = 1.5 * scale;
    let shadow_spread = 3.2 * scale;

    for py in 0..height {
        for px in 0..width {
            let p = [px as f32 + 0.5, py as f32 + 0.5];
            let d_box = sd_rounded_box([p[0] - center[0], p[1] - center[1]], half_size, radius);
            let d_tri = sd_triangle(p, p0, p1, p2);
            let d = d_box.min(d_tri);

            let mut dst = [0.0f32; 4];

            // Sombra sutil debajo de la burbuja
            let ps = [p[0], p[1] - shadow_shift];
            let ds_box =
                sd_rounded_box([ps[0] - center[0], ps[1] - center[1]], half_size, radius);
            let ds_tri = sd_triangle(ps, p0, p1, p2);
            let ds = ds_box.min(ds_tri);
            if ds < shadow_spread {
                let s_alpha = (1.0 - smoothstep(0.0, shadow_spread, ds.max(0.0))) * 0.28;
                dst = blend_over(dst, [0.0, 0.0, 0.0, s_alpha]);
            }

            // Relleno y borde de la burbuja
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

    // Dibujar texto centrado dentro de la caja
    let ox = (bx0 + pad_x).round() as i32;
    let oy = (by0 + pad_y).round() as i32;

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
                if px < 0 || px >= width as i32 {
                    continue;
                }
                let idx = (py as u32 * width + px as u32) as usize;
                canvas[idx] = blend_over(canvas[idx], src);
            }
        }
    });

    let mut rgba_premul = Vec::with_capacity((width * height * 4) as usize);
    for px in canvas {
        rgba_premul.push((px[0].clamp(0.0, 1.0) * 255.0).round() as u8);
        rgba_premul.push((px[1].clamp(0.0, 1.0) * 255.0).round() as u8);
        rgba_premul.push((px[2].clamp(0.0, 1.0) * 255.0).round() as u8);
        rgba_premul.push((px[3].clamp(0.0, 1.0) * 255.0).round() as u8);
    }

    RasterizedTooltip {
        width,
        height,
        shadow_pad,
        rgba_premul,
    }
}

/// Composición Porter-Duff sobre destino ya premultiplicado (src en alpha recto).
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
