use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Weight};
use dock_core::physics::BadgeSlot;
use wgpu::util::DeviceExt;

const MAX_CACHED_BADGES: usize = 64;

const BADGE_SHADER: &str = r#"
struct BadgeUniforms {
    rect: vec4<f32>,
    resolution: vec2<f32>,
    opacity: f32,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> u: BadgeUniforms;
@group(1) @binding(0) var badge_tex: texture_2d<f32>;
@group(1) @binding(1) var badge_samp: sampler;

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
        (px.x / u.resolution.x) * 2.0 - 1.0,
        1.0 - (px.y / u.resolution.y) * 2.0,
    );
    var out: VsOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let sample = textureSample(badge_tex, badge_samp, in.uv);
    return sample * u.opacity;
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BadgeUniforms {
    rect: [f32; 4],
    resolution: [f32; 2],
    opacity: f32,
    _pad: f32,
}

struct CachedBadgeTex {
    _texture: wgpu::Texture,
    _view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    width_px: f32,
    height_px: f32,
}

struct UniformSlot {
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

pub struct BadgePipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_bgl: wgpu::BindGroupLayout,
    tex_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform_slots: Vec<UniformSlot>,
    tex_cache: HashMap<String, CachedBadgeTex>,
    cached_scale_key: u32,
    active_draws: Vec<(usize, String)>,
}

impl BadgePipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("badge-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("badge-uniform-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let tex_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("badge-tex-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
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

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("badge-shader"),
            source: wgpu::ShaderSource::Wgsl(BADGE_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("badge-pipeline-layout"),
            bind_group_layouts: &[&uniform_bgl, &tex_bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("badge-pipeline"),
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
            uniform_bgl,
            tex_bgl,
            sampler,
            uniform_slots: Vec::new(),
            tex_cache: HashMap::new(),
            cached_scale_key: 0,
            active_draws: Vec::new(),
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        resolution: [f32; 2],
        scale_factor: f32,
        badges: &[BadgeSlot],
    
        font_system: &mut FontSystem,
        swash_cache: &mut SwashCache,
    ) -> usize {
        self.active_draws.clear();
        if badges.is_empty() {
            return 0;
        }

        let s = scale_factor.max(1.0);
        let scale_key = (s * 100.0).round() as u32;
        if self.cached_scale_key != scale_key {
            self.tex_cache.clear();
            self.cached_scale_key = scale_key;
        }

        for badge in badges {
            if badge.opacity <= 0.005 || badge.count == 0 {
                continue;
            }

            let label = format_badge_count(badge.count);
            if !self.tex_cache.contains_key(&label) {
                if self.tex_cache.len() >= MAX_CACHED_BADGES {
                    self.tex_cache.clear();
                }
                let raster_scale = s * 1.5;
                let raster = rasterize_badge(
                    font_system,
                    swash_cache,
                    &label,
                    raster_scale,
                );
                let tex_size = wgpu::Extent3d {
                    width: raster.width,
                    height: raster.height,
                    depth_or_array_layers: 1,
                };
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("badge-texture"),
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
                    label: Some("badge-tex-bg"),
                    layout: &self.tex_bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                });
                self.tex_cache.insert(
                    label.clone(),
                    CachedBadgeTex {
                        _texture: texture,
                        _view: view,
                        bind_group,
                        width_px: raster.width as f32 / 1.5,
                        height_px: raster.height as f32 / 1.5,
                    },
                );
            }

            let Some(cached) = self.tex_cache.get(&label) else {
                continue;
            };

            let slot_idx = self.active_draws.len();
            if slot_idx >= self.uniform_slots.len() {
                let init = BadgeUniforms {
                    rect: [0.0; 4],
                    resolution,
                    opacity: 0.0,
                    _pad: 0.0,
                };
                let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("badge-uniform"),
                    contents: bytemuck::bytes_of(&init),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("badge-uniform-bg"),
                    layout: &self.uniform_bgl,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer.as_entire_binding(),
                    }],
                });
                self.uniform_slots.push(UniformSlot { buffer, bind_group });
            }

            let w = (cached.width_px * badge.scale).round();
            let h = (cached.height_px * badge.scale).round();
            let x = (badge.x * s - w * 0.5).round();
            let y = (badge.y * s - h * 0.5).round();
            let uniform = BadgeUniforms {
                rect: [x, y, w, h],
                resolution,
                opacity: badge.opacity,
                _pad: 0.0,
            };
            queue.write_buffer(
                &self.uniform_slots[slot_idx].buffer,
                0,
                bytemuck::bytes_of(&uniform),
            );
            self.active_draws.push((slot_idx, label));
        }

        self.active_draws.len()
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, count: usize) {
        if count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        for (slot_idx, label) in self.active_draws.iter().take(count) {
            let Some(slot) = self.uniform_slots.get(*slot_idx) else {
                continue;
            };
            let Some(tex) = self.tex_cache.get(label) else {
                continue;
            };
            pass.set_bind_group(0, &slot.bind_group, &[]);
            pass.set_bind_group(1, &tex.bind_group, &[]);
            pass.draw(0..6, 0..1);
        }
    }
}

fn format_badge_count(count: u64) -> String {
    if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    }
}

struct RasterizedBadge {
    width: u32,
    height: u32,
    rgba_premul: Vec<u8>,
}

fn rasterize_badge(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    label: &str,
    scale: f32,
) -> RasterizedBadge {
    let font_size = 11.0 * scale;
    let line_height = (13.0 * scale).round();
    let metrics = Metrics::new(font_size, line_height);

    let mut buffer = Buffer::new(font_system, metrics);
    buffer.set_size(font_system, None, None);
    let attrs = Attrs::new().family(Family::SansSerif).weight(Weight::BOLD);
    buffer.set_text(font_system, label, attrs, Shaping::Advanced);
    buffer.shape_until_scroll(font_system, false);

    let mut text_w = 0.0f32;
    for run in buffer.layout_runs() {
        text_w = text_w.max(run.line_w);
    }

    let box_h = (18.0 * scale).round();
    let pad_x = (5.5 * scale).round();
    let box_w = (text_w.ceil() + pad_x * 2.0).max(box_h);
    let radius = box_h * 0.5;
    let shadow_pad = (3.0 * scale).ceil();
    let border_w = (1.0 * scale).max(1.0);

    let width = (box_w + shadow_pad * 2.0).ceil() as u32;
    let height = (box_h + shadow_pad * 2.0).ceil() as u32;
    let mut canvas = vec![[0.0f32; 4]; (width * height) as usize];

    let center = [width as f32 * 0.5, height as f32 * 0.5];
    let half_size = [box_w * 0.5, box_h * 0.5];

    let fill_col = [0.953, 0.239, 0.282, 0.98]; // Rojo vivo estilo macOS
    let border_col = [1.0, 1.0, 1.0, 0.24];
    let aa = (0.75 * scale).max(0.75);
    let shadow_shift = 1.2 * scale;
    let shadow_spread = 2.6 * scale;

    for py in 0..height {
        for px in 0..width {
            let p = [px as f32 + 0.5, py as f32 + 0.5];
            let d = sd_rounded_box([p[0] - center[0], p[1] - center[1]], half_size, radius);
            let mut dst = [0.0f32; 4];

            let ds = sd_rounded_box(
                [p[0] - center[0], p[1] - shadow_shift - center[1]],
                half_size,
                radius,
            );
            if ds < shadow_spread {
                let s_alpha = (1.0 - smoothstep(0.0, shadow_spread, ds.max(0.0))) * 0.32;
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

    let ox = (center[0] - text_w * 0.5).round() as i32;
    let oy = (center[1] - line_height * 0.5).round() as i32;
    let text_color = Color::rgba(255, 255, 255, 255);

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

    RasterizedBadge {
        width,
        height,
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
