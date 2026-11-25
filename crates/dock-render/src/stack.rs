use std::path::Path;

use bytemuck::{Pod, Zeroable};
use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Weight};
use dock_core::stack::{FanLayout, FanStackData, GridLayout, ListLayout};
use resvg::tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};
use wgpu::util::DeviceExt;

use crate::icons::load_icon_rgba;

const QUAD_SHADER: &str = r#"
struct QuadUniform {
    center: vec2<f32>,
    size: vec2<f32>,
    resolution: vec2<f32>,
    angle_rad: f32,
    opacity: f32,
    tint: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: QuadUniform;
@group(0) @binding(1) var quad_tex: texture_2d<f32>;
@group(0) @binding(2) var quad_samp: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-0.5, -0.5),
        vec2<f32>( 0.5, -0.5),
        vec2<f32>(-0.5,  0.5),
        vec2<f32>( 0.5, -0.5),
        vec2<f32>( 0.5,  0.5),
        vec2<f32>(-0.5,  0.5),
    );
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
    );
    let local = corners[idx] * u.size;
    let c = cos(u.angle_rad);
    let s = sin(u.angle_rad);
    let rotated = vec2<f32>(
        local.x * c - local.y * s,
        local.x * s + local.y * c,
    );
    let px = u.center + rotated;
    let ndc = vec2<f32>(
        (px.x / max(u.resolution.x, 1.0)) * 2.0 - 1.0,
        1.0 - (px.y / max(u.resolution.y, 1.0)) * 2.0,
    );
    var out: VsOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = uvs[idx];
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var col = textureSample(quad_tex, quad_samp, in.uv);
    // Resalte de hover en GPU (mezcla premultiplicada)
    if (u.tint.a > 0.001 && col.a > 0.001) {
        let rgb = mix(col.rgb, u.tint.rgb * col.a, u.tint.a);
        col = vec4<f32>(rgb, col.a);
    }
    return col * u.opacity;
}
"#;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct QuadUniform {
    center: [f32; 2],
    size: [f32; 2],
    resolution: [f32; 2],
    angle_rad: f32,
    opacity: f32,
    tint: [f32; 4],
}

struct GpuSprite {
    _texture: wgpu::Texture,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

struct BakedSlot {
    icon: GpuSprite,
    label: GpuSprite,
}

pub struct FanStackPipeline {
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
    font_system: FontSystem,
    swash_cache: SwashCache,
    baked_slots: Vec<BakedSlot>,
    grid_bg: Option<GpuSprite>,
    more_circle: Option<GpuSprite>,
    more_label: Option<GpuSprite>,
    baked_key: Option<(String, u32, bool, u8, Option<[u8; 3]>, u8)>,
    visible: bool,
}

impl FanStackPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fan-quad-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fan-quad-bgl"),
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
            label: Some("fan-quad-shader"),
            source: wgpu::ShaderSource::Wgsl(QUAD_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fan-quad-pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fan-quad-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            sampler,
            bind_group_layout,
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            baked_slots: Vec::new(),
            grid_bg: None,
            more_circle: None,
            more_label: None,
            baked_key: None,
            visible: false,
        }
    }

    fn upload_sprite(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        w: u32,
        h: u32,
        rgba: &[u8],
    ) -> GpuSprite {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fan-sprite-tex"),
            size: wgpu::Extent3d {
                width: w.max(1),
                height: h.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
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
            rgba,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * w.max(1)),
                rows_per_image: Some(h.max(1)),
            },
            wgpu::Extent3d {
                width: w.max(1),
                height: h.max(1),
                depth_or_array_layers: 1,
            },
        );
        let init_u = QuadUniform {
            center: [0.0, 0.0],
            size: [0.0, 0.0],
            resolution: [1.0, 1.0],
            angle_rad: 0.0,
            opacity: 0.0,
            tint: [0.0; 4],
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("fan-sprite-ub"),
            contents: bytemuck::bytes_of(&init_u),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fan-sprite-bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
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
        GpuSprite {
            _texture: texture,
            uniform_buffer,
            bind_group,
        }
    }

    /// Hornea las texturas solo si cambió la carpeta, el tamaño de ícono o el tema.
    /// En animación y hover solo actualiza uniforms (0 CPU, 60+ fps).
    #[allow(clippy::too_many_arguments)]
    pub fn update_fan(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &FanStackData,
        layout: &FanLayout,
        canvas_w: u32,
        canvas_h: u32,
        scale: f32,
        hovered_slot: Option<usize>,
        hover_more: bool,
        is_dark: bool,
        opacity: f32,
    ) {
        let icon_px = layout
            .slots
            .first()
            .map(|s| (s.icon_size * scale).round().max(32.0) as u32)
            .unwrap_or(48);
        self.grid_bg = None;
        let key = (data.folder_path.clone(), icon_px, is_dark, 0u8, None, 0u8);

        if self.baked_key.as_ref() != Some(&key) || self.baked_slots.len() != data.items.len() {
            self.baked_slots.clear();
            let text_color = if is_dark {
                Color::rgba(242, 242, 248, 245)
            } else {
                Color::rgba(28, 30, 38, 245)
            };

            for (item, slot) in data.items.iter().zip(layout.slots.iter()) {
                let decoded = load_icon_rgba(item.icon_path.as_deref().map(Path::new), icon_px);
                let icon_sprite =
                    self.upload_sprite(device, queue, decoded.width, decoded.height, &decoded.rgba_premul);

                let pw = (slot.label_w * scale).ceil().max(16.0) as u32;
                let ph = (slot.label_h * scale).ceil().max(12.0) as u32;
                let mut pm = Pixmap::new(pw, ph).unwrap();
                draw_pill(&mut pm, 0.0, 0.0, pw as f32, ph as f32, is_dark);
                draw_text_centered(
                    &mut self.font_system,
                    &mut self.swash_cache,
                    &mut pm,
                    &truncate_single_line(&item.name, 34),
                    10.0 * scale,
                    0.0,
                    (pw as f32 - 16.0 * scale).max(12.0),
                    ph as f32,
                    15.4 * scale,
                    text_color,
                );
                let label_sprite = self.upload_sprite(device, queue, pw, ph, pm.data());
                self.baked_slots.push(BakedSlot {
                    icon: icon_sprite,
                    label: label_sprite,
                });
            }

            if let (Some([_, _]), Some([_, _, lw, lh]), Some(label)) = (
                layout.more_button_center,
                layout.more_label_rect,
                layout.more_label_text.as_deref(),
            ) {
                let d = (layout.more_button_radius * 2.0 * scale).ceil().max(24.0) as u32;
                let mut cpm = Pixmap::new(d, d).unwrap();
                draw_more_circle(&mut cpm, d as f32 * 0.5, d as f32 * 0.5, d as f32 * 0.48, is_dark);
                self.more_circle = Some(self.upload_sprite(device, queue, d, d, cpm.data()));

                let pw = (lw * scale).ceil().max(24.0) as u32;
                let ph = (lh * scale).ceil().max(12.0) as u32;
                let mut lpm = Pixmap::new(pw, ph).unwrap();
                draw_pill(&mut lpm, 0.0, 0.0, pw as f32, ph as f32, is_dark);
                draw_text_centered(
                    &mut self.font_system,
                    &mut self.swash_cache,
                    &mut lpm,
                    label,
                    8.0 * scale,
                    0.0,
                    (pw as f32 - 16.0 * scale).max(12.0),
                    ph as f32,
                    12.0 * scale,
                    text_color,
                );
                self.more_label = Some(self.upload_sprite(device, queue, pw, ph, lpm.data()));
            } else {
                self.more_circle = None;
                self.more_label = None;
            }
            self.baked_key = Some(key);
        }

        let res = [canvas_w.max(1) as f32, canvas_h.max(1) as f32];
        let hover_tint = [0.25, 0.52, 0.96, 0.65];
        let no_tint = [0.0; 4];

        for (i, (baked, slot)) in self.baked_slots.iter().zip(layout.slots.iter()).enumerate() {
            let is_hov = hovered_slot == Some(i);
            let zoom = if is_hov { 1.12 } else { 1.0 };

            let icon_u = QuadUniform {
                center: [
                    (slot.icon_x + slot.icon_size * 0.5) * scale,
                    (slot.icon_y + slot.icon_size * 0.5) * scale,
                ],
                size: [slot.icon_size * scale * zoom, slot.icon_size * scale * zoom],
                resolution: res,
                angle_rad: slot.angle_rad,
                opacity,
                tint: no_tint,
            };
            queue.write_buffer(&baked.icon.uniform_buffer, 0, bytemuck::bytes_of(&icon_u));

            let label_u = QuadUniform {
                center: [
                    (slot.label_x + slot.label_w * 0.5) * scale,
                    (slot.label_y + slot.label_h * 0.5) * scale,
                ],
                size: [slot.label_w * scale, slot.label_h * scale],
                resolution: res,
                angle_rad: slot.angle_rad,
                opacity,
                tint: if is_hov { hover_tint } else { no_tint },
            };
            queue.write_buffer(&baked.label.uniform_buffer, 0, bytemuck::bytes_of(&label_u));
        }

        if let (Some([cx, cy]), Some(ref circle)) = (layout.more_button_center, &self.more_circle) {
            let zoom = if hover_more { 1.1 } else { 1.0 };
            let d = layout.more_button_radius * 2.0 * scale * zoom;
            let u = QuadUniform {
                center: [cx * scale, cy * scale],
                size: [d, d],
                resolution: res,
                angle_rad: layout.more_angle_rad,
                opacity,
                tint: if hover_more { hover_tint } else { no_tint },
            };
            queue.write_buffer(&circle.uniform_buffer, 0, bytemuck::bytes_of(&u));
        }
        if let (Some([lx, ly, lw, lh]), Some(ref label)) = (layout.more_label_rect, &self.more_label) {
            let u = QuadUniform {
                center: [(lx + lw * 0.5) * scale, (ly + lh * 0.5) * scale],
                size: [lw * scale, lh * scale],
                resolution: res,
                angle_rad: layout.more_angle_rad,
                opacity,
                tint: if hover_more { hover_tint } else { no_tint },
            };
            queue.write_buffer(&label.uniform_buffer, 0, bytemuck::bytes_of(&u));
        }

        self.visible = opacity > 0.001;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_grid(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &FanStackData,
        layout: &GridLayout,
        canvas_w: u32,
        canvas_h: u32,
        scale: f32,
        hovered_slot: Option<usize>,
        hover_more: bool,
        is_dark: bool,
        custom_tint: Option<[u8; 3]>,
        glass_opacity: f32,
        opacity: f32,
    ) {
        let icon_px = layout
            .slots
            .first()
            .map(|s| (s.icon_rect[2] * scale).round().max(32.0) as u32)
            .unwrap_or((54.0 * scale).round() as u32);
        let alpha_u8 = (glass_opacity.clamp(0.35, 0.92) * 255.0).round() as u8;
        let key = (data.folder_path.clone(), icon_px, is_dark, 1u8, custom_tint, alpha_u8);

        if self.baked_key.as_ref() != Some(&key) || self.baked_slots.len() != data.items.len() {
            self.baked_slots.clear();
            let text_color = if is_dark {
                Color::rgba(242, 242, 248, 245)
            } else {
                Color::rgba(28, 30, 38, 245)
            };
            let title_color = if is_dark {
                Color::rgba(225, 228, 238, 235)
            } else {
                Color::rgba(45, 48, 58, 235)
            };

            // 1. Fondo del popover + título centrado
            let [bx, by, bw, bh] = layout.box_rect;
            let pad = 10.0_f32;
            let bg_w = ((bw + pad * 2.0) * scale).ceil().max(32.0) as u32;
            let bg_h = ((bh + pad * 2.0) * scale).ceil().max(32.0) as u32;
            let mut bg_pm = Pixmap::new(bg_w, bg_h).unwrap();
            draw_grid_panel(
                &mut bg_pm,
                pad * scale,
                pad * scale,
                bw * scale,
                bh * scale,
                (layout.arrow_tip[0] - bx + pad) * scale,
                (layout.arrow_tip[1] - by + pad) * scale,
                layout.position,
                14.0 * scale,
                is_dark,
                custom_tint,
                alpha_u8,
            );
            draw_text_centered(
                &mut self.font_system,
                &mut self.swash_cache,
                &mut bg_pm,
                &truncate_single_line(&data.folder_name, 36),
                (pad + 16.0) * scale,
                (pad + 4.0) * scale,
                (bw - 32.0).max(20.0) * scale,
                (layout.header_rect[3]).max(16.0) * scale,
                17.0 * scale,
                title_color,
            );
            self.grid_bg = Some(self.upload_sprite(device, queue, bg_w, bg_h, bg_pm.data()));

            // 2. Íconos y etiquetas de cada celda
            for (item, slot) in data.items.iter().zip(layout.slots.iter()) {
                let decoded = load_icon_rgba(item.icon_path.as_deref().map(Path::new), icon_px);
                let icon_sprite = self.upload_sprite(
                    device,
                    queue,
                    decoded.width,
                    decoded.height,
                    &decoded.rgba_premul,
                );

                let pw = (slot.label_rect[2] * scale).ceil().max(16.0) as u32;
                let ph = (slot.label_rect[3] * scale).ceil().max(12.0) as u32;
                let mut pm = Pixmap::new(pw, ph).unwrap();
                draw_text_centered(
                    &mut self.font_system,
                    &mut self.swash_cache,
                    &mut pm,
                    &truncate_single_line(&item.name, 16),
                    2.0 * scale,
                    0.0,
                    (pw as f32 - 4.0 * scale).max(10.0),
                    ph as f32,
                    14.2 * scale,
                    text_color,
                );
                let label_sprite = self.upload_sprite(device, queue, pw, ph, pm.data());
                self.baked_slots.push(BakedSlot {
                    icon: icon_sprite,
                    label: label_sprite,
                });
            }

            // 3. Botón final "Abrir en gestor de archivos"
            let d = (layout.open_button_slot.icon_rect[2] * scale).ceil().max(24.0) as u32;
            let mut cpm = Pixmap::new(d, d).unwrap();
            draw_more_circle(&mut cpm, d as f32 * 0.5, d as f32 * 0.5, d as f32 * 0.46, is_dark);
            self.more_circle = Some(self.upload_sprite(device, queue, d, d, cpm.data()));

            let pw = (layout.open_button_slot.label_rect[2] * scale).ceil().max(24.0) as u32;
            let ph = (layout.open_button_slot.label_rect[3] * scale).ceil().max(12.0) as u32;
            let mut lpm = Pixmap::new(pw, ph).unwrap();
            draw_text_centered(
                &mut self.font_system,
                &mut self.swash_cache,
                &mut lpm,
                &truncate_single_line(&layout.open_button_label, 16),
                2.0 * scale,
                0.0,
                (pw as f32 - 4.0 * scale).max(10.0),
                ph as f32,
                14.2 * scale,
                text_color,
            );
            self.more_label = Some(self.upload_sprite(device, queue, pw, ph, lpm.data()));
            self.baked_key = Some(key);
        }

        let res = [canvas_w.max(1) as f32, canvas_h.max(1) as f32];
        let hover_tint = [0.25, 0.52, 0.96, 0.55];
        let no_tint = [0.0; 4];

        if let Some(ref bg) = self.grid_bg {
            let [bx, by, bw, bh] = layout.box_rect;
            let pad = 10.0_f32;
            let u = QuadUniform {
                center: [(bx + bw * 0.5) * scale, (by + bh * 0.5) * scale],
                size: [(bw + pad * 2.0) * scale, (bh + pad * 2.0) * scale],
                resolution: res,
                angle_rad: 0.0,
                opacity,
                tint: no_tint,
            };
            queue.write_buffer(&bg.uniform_buffer, 0, bytemuck::bytes_of(&u));
        }

        for (i, (baked, slot)) in self.baked_slots.iter().zip(layout.slots.iter()).enumerate() {
            let is_hov = hovered_slot == Some(i);
            let zoom = if is_hov { 1.08 } else { 1.0 };

            let [ix, iy, iw, ih] = slot.icon_rect;
            let icon_u = QuadUniform {
                center: [(ix + iw * 0.5) * scale, (iy + ih * 0.5) * scale],
                size: [iw * scale * zoom, ih * scale * zoom],
                resolution: res,
                angle_rad: 0.0,
                opacity,
                tint: no_tint,
            };
            queue.write_buffer(&baked.icon.uniform_buffer, 0, bytemuck::bytes_of(&icon_u));

            let [lx, ly, lw, lh] = slot.label_rect;
            let label_u = QuadUniform {
                center: [(lx + lw * 0.5) * scale, (ly + lh * 0.5) * scale],
                size: [lw * scale, lh * scale],
                resolution: res,
                angle_rad: 0.0,
                opacity,
                tint: if is_hov { hover_tint } else { no_tint },
            };
            queue.write_buffer(&baked.label.uniform_buffer, 0, bytemuck::bytes_of(&label_u));
        }

        if let Some(ref circle) = self.more_circle {
            let zoom = if hover_more { 1.08 } else { 1.0 };
            let [ix, iy, iw, ih] = layout.open_button_slot.icon_rect;
            let u = QuadUniform {
                center: [(ix + iw * 0.5) * scale, (iy + ih * 0.5) * scale],
                size: [iw * scale * zoom, ih * scale * zoom],
                resolution: res,
                angle_rad: 0.0,
                opacity,
                tint: if hover_more { hover_tint } else { no_tint },
            };
            queue.write_buffer(&circle.uniform_buffer, 0, bytemuck::bytes_of(&u));
        }
        if let Some(ref label) = self.more_label {
            let [lx, ly, lw, lh] = layout.open_button_slot.label_rect;
            let u = QuadUniform {
                center: [(lx + lw * 0.5) * scale, (ly + lh * 0.5) * scale],
                size: [lw * scale, lh * scale],
                resolution: res,
                angle_rad: 0.0,
                opacity,
                tint: if hover_more { hover_tint } else { no_tint },
            };
            queue.write_buffer(&label.uniform_buffer, 0, bytemuck::bytes_of(&u));
        }

        self.visible = opacity > 0.001;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_list(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &FanStackData,
        layout: &ListLayout,
        canvas_w: u32,
        canvas_h: u32,
        scale: f32,
        hovered_slot: Option<usize>,
        hover_more: bool,
        is_dark: bool,
        custom_tint: Option<[u8; 3]>,
        glass_opacity: f32,
        opacity: f32,
    ) {
        let icon_px = (22.0 * scale).round().max(18.0) as u32;
        let alpha_u8 = (glass_opacity.clamp(0.35, 0.92) * 255.0).round() as u8;
        let key = (data.folder_path.clone(), icon_px, is_dark, 2u8, custom_tint, alpha_u8);

        if self.baked_key.as_ref() != Some(&key) || self.baked_slots.len() != data.items.len() {
            self.baked_slots.clear();
            let text_color = if is_dark {
                Color::rgba(242, 242, 248, 245)
            } else {
                Color::rgba(28, 30, 38, 245)
            };

            // 1. Fondo del popover + línea separadora inferior
            let [bx, by, bw, bh] = layout.box_rect;
            let pad = 10.0_f32;
            let bg_w = ((bw + pad * 2.0) * scale).ceil().max(32.0) as u32;
            let bg_h = ((bh + pad * 2.0) * scale).ceil().max(32.0) as u32;
            let mut bg_pm = Pixmap::new(bg_w, bg_h).unwrap();
            draw_grid_panel(
                &mut bg_pm,
                pad * scale,
                pad * scale,
                bw * scale,
                bh * scale,
                (layout.arrow_tip[0] - bx + pad) * scale,
                (layout.arrow_tip[1] - by + pad) * scale,
                layout.position,
                12.0 * scale,
                is_dark,
                custom_tint,
                alpha_u8,
            );

            // Separador horizontal sobre el botón inferior
            let sep_y = (layout.separator_y - by + pad) * scale;
            let mut sep_pb = resvg::tiny_skia::PathBuilder::new();
            sep_pb.move_to((pad + 10.0) * scale, sep_y);
            sep_pb.line_to((pad + bw - 10.0) * scale, sep_y);
            if let Some(sep_path) = sep_pb.finish() {
                let mut sep_paint = resvg::tiny_skia::Paint::default();
                sep_paint.anti_alias = true;
                sep_paint.set_color(if is_dark {
                    resvg::tiny_skia::Color::from_rgba8(255, 255, 255, 34)
                } else {
                    resvg::tiny_skia::Color::from_rgba8(0, 0, 0, 28)
                });
                let stroke = resvg::tiny_skia::Stroke {
                    width: (1.0 * scale).max(1.0),
                    ..Default::default()
                };
                bg_pm.stroke_path(
                    &sep_path,
                    &sep_paint,
                    &stroke,
                    resvg::tiny_skia::Transform::identity(),
                    None,
                );
            }
            self.grid_bg = Some(self.upload_sprite(device, queue, bg_w, bg_h, bg_pm.data()));

            // 2. Filas (ícono pequeño + etiqueta)
            for (item, slot) in data.items.iter().zip(layout.slots.iter()) {
                let decoded = load_icon_rgba(item.icon_path.as_deref().map(Path::new), icon_px);
                let icon_sprite = self.upload_sprite(
                    device,
                    queue,
                    decoded.width,
                    decoded.height,
                    &decoded.rgba_premul,
                );

                let pw = (slot.label_rect[2] * scale).ceil().max(16.0) as u32;
                let ph = (slot.label_rect[3] * scale).ceil().max(12.0) as u32;
                let mut pm = Pixmap::new(pw, ph).unwrap();
                let display_name = if item.is_dir {
                    format!("{}  ▸", truncate_single_line(&item.name, 28))
                } else {
                    truncate_single_line(&item.name, 32)
                };
                draw_text_centered(
                    &mut self.font_system,
                    &mut self.swash_cache,
                    &mut pm,
                    &display_name,
                    4.0 * scale,
                    0.0,
                    (pw as f32 - 8.0 * scale).max(10.0),
                    ph as f32,
                    12.0 * scale,
                    text_color,
                );
                let label_sprite = self.upload_sprite(device, queue, pw, ph, pm.data());
                self.baked_slots.push(BakedSlot {
                    icon: icon_sprite,
                    label: label_sprite,
                });
            }

            // 3. Fila inferior "Abrir en gestor de archivos"
            self.more_circle = None;
            let pw = (layout.open_row_rect[2] * scale).ceil().max(24.0) as u32;
            let ph = (layout.open_row_rect[3] * scale).ceil().max(12.0) as u32;
            let mut lpm = Pixmap::new(pw, ph).unwrap();
            draw_text_centered(
                &mut self.font_system,
                &mut self.swash_cache,
                &mut lpm,
                &truncate_single_line(&layout.open_button_label, 32),
                6.0 * scale,
                0.0,
                (pw as f32 - 12.0 * scale).max(10.0),
                ph as f32,
                15.4 * scale,
                text_color,
            );
            self.more_label = Some(self.upload_sprite(device, queue, pw, ph, lpm.data()));
            self.baked_key = Some(key);
        }

        let res = [canvas_w.max(1) as f32, canvas_h.max(1) as f32];
        let hover_tint = [0.25, 0.52, 0.96, 0.55];
        let no_tint = [0.0; 4];

        if let Some(ref bg) = self.grid_bg {
            let [bx, by, bw, bh] = layout.box_rect;
            let pad = 10.0_f32;
            let u = QuadUniform {
                center: [(bx + bw * 0.5) * scale, (by + bh * 0.5) * scale],
                size: [(bw + pad * 2.0) * scale, (bh + pad * 2.0) * scale],
                resolution: res,
                angle_rad: 0.0,
                opacity,
                tint: no_tint,
            };
            queue.write_buffer(&bg.uniform_buffer, 0, bytemuck::bytes_of(&u));
        }

        for (i, (baked, slot)) in self.baked_slots.iter().zip(layout.slots.iter()).enumerate() {
            let is_hov = hovered_slot == Some(i);

            let [ix, iy, iw, ih] = slot.icon_rect;
            let icon_u = QuadUniform {
                center: [(ix + iw * 0.5) * scale, (iy + ih * 0.5) * scale],
                size: [iw * scale, ih * scale],
                resolution: res,
                angle_rad: 0.0,
                opacity,
                tint: no_tint,
            };
            queue.write_buffer(&baked.icon.uniform_buffer, 0, bytemuck::bytes_of(&icon_u));

            let [lx, ly, lw, lh] = slot.label_rect;
            let label_u = QuadUniform {
                center: [(lx + lw * 0.5) * scale, (ly + lh * 0.5) * scale],
                size: [lw * scale, lh * scale],
                resolution: res,
                angle_rad: 0.0,
                opacity,
                tint: if is_hov { hover_tint } else { no_tint },
            };
            queue.write_buffer(&baked.label.uniform_buffer, 0, bytemuck::bytes_of(&label_u));
        }

        if let Some(ref label) = self.more_label {
            let [rx, ry, rw, rh] = layout.open_row_rect;
            let u = QuadUniform {
                center: [(rx + rw * 0.5) * scale, (ry + rh * 0.5) * scale],
                size: [rw * scale, rh * scale],
                resolution: res,
                angle_rad: 0.0,
                opacity,
                tint: if hover_more { hover_tint } else { no_tint },
            };
            queue.write_buffer(&label.uniform_buffer, 0, bytemuck::bytes_of(&u));
        }

        self.visible = opacity > 0.001;
    }

    pub fn clear(&mut self) {
        if !self.visible && self.baked_key.is_none() {
            return;
        }
        self.visible = false;
        self.baked_slots.clear();
        self.grid_bg = None;
        self.more_circle = None;
        self.more_label = None;
        self.baked_key = None;
    }

    pub fn draw<'a>(&'a self, rpass: &mut wgpu::RenderPass<'a>) {
        if !self.visible {
            return;
        }
        rpass.set_pipeline(&self.pipeline);
        if let Some(ref bg) = self.grid_bg {
            rpass.set_bind_group(0, &bg.bind_group, &[]);
            rpass.draw(0..6, 0..1);
        }
        for slot in &self.baked_slots {
            rpass.set_bind_group(0, &slot.label.bind_group, &[]);
            rpass.draw(0..6, 0..1);
            rpass.set_bind_group(0, &slot.icon.bind_group, &[]);
            rpass.draw(0..6, 0..1);
        }
        if let Some(ref label) = self.more_label {
            rpass.set_bind_group(0, &label.bind_group, &[]);
            rpass.draw(0..6, 0..1);
        }
        if let Some(ref circle) = self.more_circle {
            rpass.set_bind_group(0, &circle.bind_group, &[]);
            rpass.draw(0..6, 0..1);
        }
    }
}

fn draw_pill(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, is_dark: bool) {
    let r = (h * 0.5).min(w * 0.5);
    let Some(path) = rounded_rect_path(x, y, w, h, r) else {
        return;
    };
    let mut paint = Paint::default();
    paint.anti_alias = true;
    if is_dark {
        paint.set_color_rgba8(38, 40, 50, 210);
    } else {
        paint.set_color_rgba8(240, 242, 248, 220);
    }
    pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
}

fn draw_more_circle(pixmap: &mut Pixmap, cx: f32, cy: f32, r: f32, is_dark: bool) {
    let Some(circle) = PathBuilder::from_circle(cx, cy, r) else {
        return;
    };
    let mut bg = Paint::default();
    bg.anti_alias = true;
    if is_dark {
        bg.set_color_rgba8(50, 54, 66, 225);
    } else {
        bg.set_color_rgba8(235, 238, 245, 230);
    }
    pixmap.fill_path(&circle, &bg, FillRule::Winding, Transform::identity(), None);

    let mut pb = PathBuilder::new();
    let d = r * 0.42;
    pb.move_to(cx - d, cy + d * 0.25);
    pb.line_to(cx + d * 0.65, cy - d * 0.25);
    pb.move_to(cx + d * 0.05, cy - d * 0.7);
    pb.line_to(cx + d * 0.75, cy - d * 0.25);
    pb.line_to(cx + d * 0.2, cy + d * 0.45);
    if let Some(arrow) = pb.finish() {
        let mut fg = Paint::default();
        fg.anti_alias = true;
        if is_dark {
            fg.set_color_rgba8(245, 245, 250, 245);
        } else {
            fg.set_color_rgba8(40, 42, 50, 245);
        }
        let mut stroke = Stroke::default();
        stroke.width = (r * 0.14).max(1.8);
        pixmap.stroke_path(&arrow, &fg, &stroke, Transform::identity(), None);
    }
}

fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<resvg::tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    let x1 = x + w;
    let y1 = y + h;
    pb.move_to(x + r, y);
    pb.line_to(x1 - r, y);
    pb.quad_to(x1, y, x1, y + r);
    pb.line_to(x1, y1 - r);
    pb.quad_to(x1, y1, x1 - r, y1);
    pb.line_to(x + r, y1);
    pb.quad_to(x, y1, x, y1 - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}

#[allow(clippy::too_many_arguments)]
fn draw_text_centered(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    pixmap: &mut Pixmap,
    text: &str,
    x: f32,
    y: f32,
    max_w: f32,
    box_h: f32,
    font_size: f32,
    color: Color,
) {
    let metrics = Metrics::new(font_size, font_size * 1.25);
    let mut buffer = Buffer::new(font_system, metrics);
    let _ = max_w;
    buffer.set_size(font_system, None, Some(box_h));
    let attrs = Attrs::new().family(Family::SansSerif).weight(Weight::MEDIUM);
    buffer.set_text(font_system, text, attrs, Shaping::Advanced);
    buffer.shape_until_scroll(font_system, false);

    let y_offset = (box_h - font_size * 1.2) * 0.5;
    let pw = pixmap.width() as i32;
    let ph = pixmap.height() as i32;
    let data = pixmap.data_mut();

    buffer.draw(font_system, swash_cache, color, |gx, gy, w, h, c| {
        if c.a() == 0 || w == 0 || h == 0 {
            return;
        }
        let px = gx + x.round() as i32;
        let py = gy + (y + y_offset).round() as i32;
        if px < 0 || py < 0 || px >= pw || py >= ph {
            return;
        }
        let idx = ((py * pw + px) * 4) as usize;
        let sa = c.a() as f32 / 255.0;
        let inv = 1.0 - sa;
        let sr = (c.r() as f32) * sa;
        let sg = (c.g() as f32) * sa;
        let sb = (c.b() as f32) * sa;

        data[idx] = (sr + data[idx] as f32 * inv).min(255.0) as u8;
        data[idx + 1] = (sg + data[idx + 1] as f32 * inv).min(255.0) as u8;
        data[idx + 2] = (sb + data[idx + 2] as f32 * inv).min(255.0) as u8;
        data[idx + 3] = ((sa * 255.0) + data[idx + 3] as f32 * inv).min(255.0) as u8;
    });
}

fn truncate_single_line(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let mut out: String = text.chars().take(max_chars.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}


#[allow(clippy::too_many_arguments)]
fn draw_grid_panel(
    pm: &mut Pixmap,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    tip_x: f32,
    tip_y: f32,
    position: dock_core::DockPosition,
    r: f32,
    is_dark: bool,
    custom_tint: Option<[u8; 3]>,
    alpha_u8: u8,
) {
    let r = r.min(w * 0.5).min(h * 0.5);
    let mut pb = resvg::tiny_skia::PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();

    let mut fill = resvg::tiny_skia::Paint::default();
    fill.anti_alias = true;
    let bg_alpha = alpha_u8.saturating_add(64).clamp(165, 242);
    fill.set_color(if let Some([tr, tg, tb]) = custom_tint {
        resvg::tiny_skia::Color::from_rgba8(tr, tg, tb, bg_alpha)
    } else if is_dark {
        resvg::tiny_skia::Color::from_rgba8(24, 26, 38, bg_alpha)
    } else {
        resvg::tiny_skia::Color::from_rgba8(245, 246, 250, bg_alpha)
    });

    if let Some(path) = pb.finish() {
        pm.fill_path(&path, &fill, resvg::tiny_skia::FillRule::Winding, resvg::tiny_skia::Transform::identity(), None);

        let mut stroke_paint = resvg::tiny_skia::Paint::default();
        stroke_paint.anti_alias = true;
        stroke_paint.set_color(if is_dark {
            resvg::tiny_skia::Color::from_rgba8(255, 255, 255, 36)
        } else {
            resvg::tiny_skia::Color::from_rgba8(0, 0, 0, 28)
        });
        let stroke = resvg::tiny_skia::Stroke {
            width: 1.25,
            ..Default::default()
        };
        pm.stroke_path(&path, &stroke_paint, &stroke, resvg::tiny_skia::Transform::identity(), None);
    }

    // Flecha apuntando al ícono del dock
    let half = 8.0_f32;
    let mut apb = resvg::tiny_skia::PathBuilder::new();
    match position {
        dock_core::DockPosition::Bottom => {
            apb.move_to(tip_x - half, y + h - 1.0);
            apb.line_to(tip_x + half, y + h - 1.0);
            apb.line_to(tip_x, tip_y);
        }
        dock_core::DockPosition::Top => {
            apb.move_to(tip_x - half, y + 1.0);
            apb.line_to(tip_x + half, y + 1.0);
            apb.line_to(tip_x, tip_y);
        }
        dock_core::DockPosition::Left => {
            apb.move_to(x + 1.0, tip_y - half);
            apb.line_to(x + 1.0, tip_y + half);
            apb.line_to(tip_x, tip_y);
        }
        dock_core::DockPosition::Right => {
            apb.move_to(x + w - 1.0, tip_y - half);
            apb.line_to(x + w - 1.0, tip_y + half);
            apb.line_to(tip_x, tip_y);
        }
    }
    apb.close();
    if let Some(apath) = apb.finish() {
        pm.fill_path(&apath, &fill, resvg::tiny_skia::FillRule::Winding, resvg::tiny_skia::Transform::identity(), None);
    }
}
