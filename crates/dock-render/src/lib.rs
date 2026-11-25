pub mod icons;
mod dividers;
mod badges;
mod dots;
mod menu;
mod tooltip;
pub mod stack;

use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;

use bytemuck::{Pod, Zeroable};
use dock_core::desktop::DockEntry;
use dock_core::physics::FrameLayout;
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use wgpu::util::DeviceExt;

const PILL_SHADER: &str = include_str!("../../../assets/shaders/pill.wgsl");
const ICON_SHADER: &str = include_str!("../../../assets/shaders/icon.wgsl");

#[derive(Clone, Copy, Debug)]
pub struct GlassStyle {
    pub is_dark: f32,
    pub opacity: f32,
    pub border_strength: f32,
    pub custom_tint: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PillUniforms {
    resolution: [f32; 2],
    pill_center: [f32; 2],
    pill_size: [f32; 2],
    corner_radius: f32,
    is_dark: f32,
    scale_factor: f32,
    blur_active: f32,
    opacity: f32,
    border_strength: f32,
    custom_tint: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct IconUniforms {
    rect: [f32; 4],
    resolution: [f32; 2],
    pressed: f32,
    opacity: f32,
}

struct CachedIconTexture {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

struct GpuIcon {
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

pub struct DockRenderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    scale_factor: f32,

    // Pipeline de la píldora
    pill_pipeline: wgpu::RenderPipeline,
    pill_uniform_buffer: wgpu::Buffer,
    pill_bind_group: wgpu::BindGroup,

    // Pipeline de iconos
    icon_pipeline: wgpu::RenderPipeline,
    icon_bind_group_layout: wgpu::BindGroupLayout,
    icon_sampler: wgpu::Sampler,
    gpu_icons: Vec<GpuIcon>,
    dot_pipeline: dots::DotPipeline,
    divider_pipeline: dividers::DividerPipeline,
    font_system: cosmic_text::FontSystem,
    swash_cache: cosmic_text::SwashCache,
    tooltip_pipeline: tooltip::TooltipPipeline,
    menu_pipeline: menu::MenuPipeline,
    stack_pipeline: stack::FanStackPipeline,
    fan_stack_opacity: f32,
    badge_pipeline: badges::BadgePipeline,
    texture_cache: HashMap<String, CachedIconTexture>,
    cached_icon_px: u32,
    active_icon_keys: Vec<String>,
}

fn detect_kde_font() -> Option<(String, Option<std::path::PathBuf>)> {
    let home = std::env::var("HOME").ok()?;
    let content = std::fs::read_to_string(
        std::path::PathBuf::from(home).join(".config").join("kdeglobals"),
    )
    .ok()?;
    for line in content.lines() {
        if let Some(val) = line.strip_prefix("font=") {
            let parts: Vec<&str> = val.split(',').collect();
            let family = parts.first()?.trim();
            if family.is_empty() {
                continue;
            }
            let style = parts.get(16).map(|s| s.trim()).unwrap_or("");
            let pattern = if style.is_empty() {
                family.to_string()
            } else {
                format!("{family}:style={style}")
            };
            let file_path = std::process::Command::new("fc-match")
                .args(["-f", "%{file}", &pattern])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|p| !p.is_empty())
                .map(std::path::PathBuf::from);
            return Some((family.to_string(), file_path));
        }
    }
    None
}

impl DockRenderer {
    /// # Safety
    /// `display_ptr` y `surface_ptr` deben ser punteros Wayland válidos que permanezcan
    /// vivos mientras exista `DockRenderer`.
    pub unsafe fn new(
        display_ptr: *mut c_void,
        surface_ptr: *mut c_void,
        phys_width: u32,
        phys_height: u32,
        scale_factor: f32,
    ) -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN | wgpu::Backends::GL,
            ..Default::default()
        });

        let raw_display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
            NonNull::new(display_ptr).ok_or("wl_display nulo")?,
        ));
        let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(
            NonNull::new(surface_ptr).ok_or("wl_surface nulo")?,
        ));

        let surface = instance
        .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle,
            raw_window_handle,
        })
        .map_err(|e| format!("Error creando superficie wgpu: {e}"))?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
                                                                  force_fallback_adapter: false,
        }))
        .ok_or("No se encontró un adaptador GPU compatible con Wayland")?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("neonpear-dock-device"),
                                                                        required_features: wgpu::Features::empty(),
                                                                        required_limits: wgpu::Limits::default(),
                                                                        memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ))
        .map_err(|e| format!("Error solicitando dispositivo GPU: {e}"))?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
        .formats
        .iter()
        .copied()
        .find(|f| !f.is_srgb())
        .unwrap_or(caps.formats[0]);

        let alpha_mode = if caps
        .alpha_modes
        .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else if caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
            {
                wgpu::CompositeAlphaMode::PostMultiplied
            } else {
                caps.alpha_modes[0]
            };

        tracing::info!(?format, ?alpha_mode, "Superficie wgpu inicializada");

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
                width: phys_width.max(1),
                height: phys_height.max(1),
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // --- Pipeline de la píldora ---
        let init_pill = PillUniforms {
            resolution: [config.width as f32, config.height as f32],
            pill_center: [config.width as f32 * 0.5, config.height as f32 * 0.5],
            pill_size: [config.width as f32, config.height as f32],
            corner_radius: 20.0 * scale_factor,
            is_dark: 0.0,
            scale_factor,
            blur_active: 1.0,
            opacity: 0.50,
            border_strength: 1.0,
            custom_tint: [0.0, 0.0, 0.0, 0.0],
        };

        let pill_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pill-uniforms"),
                                                            contents: bytemuck::bytes_of(&init_pill),
                                                            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let pill_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pill-bgl"),
                                                       entries: &[wgpu::BindGroupLayoutEntry {
                                                           binding: 0,
                                                           visibility: wgpu::ShaderStages::FRAGMENT,
                                                           ty: wgpu::BindingType::Buffer {
                                                               ty: wgpu::BufferBindingType::Uniform,
                                                               has_dynamic_offset: false,
                                                               min_binding_size: None,
                                                           },
                                                           count: None,
                                                       }],
        });

        let pill_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pill-bg"),
                                                       layout: &pill_bgl,
                                                       entries: &[wgpu::BindGroupEntry {
                                                           binding: 0,
                                                           resource: pill_uniform_buffer.as_entire_binding(),
                                                       }],
        });

        let pill_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pill-shader"),
                                                      source: wgpu::ShaderSource::Wgsl(PILL_SHADER.into()),
        });

        let pill_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pill-pipeline-layout"),
                                                                 bind_group_layouts: &[&pill_bgl],
                                                                 push_constant_ranges: &[],
        });

        let pill_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pill-pipeline"),
                                                          layout: Some(&pill_pipeline_layout),
                                                          vertex: wgpu::VertexState {
                                                              module: &pill_shader,
                                                              entry_point: Some("vs_main"),
                                                          compilation_options: Default::default(),
                                                          buffers: &[],
                                                          },
                                                          fragment: Some(wgpu::FragmentState {
                                                              module: &pill_shader,
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

        // --- Pipeline de iconos ---
        let icon_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("icon-bgl"),
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
                                                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                                    view_dimension: wgpu::TextureViewDimension::D2,
                                                    multisampled: false,
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

        let icon_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("icon-sampler"),
                                                 address_mode_u: wgpu::AddressMode::ClampToEdge,
                                                 address_mode_v: wgpu::AddressMode::ClampToEdge,
                                                 address_mode_w: wgpu::AddressMode::ClampToEdge,
                                                 mag_filter: wgpu::FilterMode::Linear,
                                                 min_filter: wgpu::FilterMode::Linear,
                                                 mipmap_filter: wgpu::FilterMode::Nearest,
                                                 ..Default::default()
        });

        let icon_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("icon-shader"),
                                                      source: wgpu::ShaderSource::Wgsl(ICON_SHADER.into()),
        });

        let icon_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("icon-pipeline-layout"),
                                                                 bind_group_layouts: &[&icon_bind_group_layout],
                                                                 push_constant_ranges: &[],
        });

        let icon_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("icon-pipeline"),
                                                          layout: Some(&icon_pipeline_layout),
                                                          vertex: wgpu::VertexState {
                                                              module: &icon_shader,
                                                              entry_point: Some("vs_main"),
                                                          compilation_options: Default::default(),
                                                          buffers: &[],
                                                          },
                                                          fragment: Some(wgpu::FragmentState {
                                                              module: &icon_shader,
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

        let dot_pipeline = dots::DotPipeline::new(&device, format);
        let divider_pipeline = dividers::DividerPipeline::new(&device, format);
        let tooltip_pipeline = tooltip::TooltipPipeline::new(&device, format);
        let menu_pipeline = menu::MenuPipeline::new(&device, format);
        let stack_pipeline = stack::FanStackPipeline::new(&device, format);
        let badge_pipeline = badges::BadgePipeline::new(&device, format);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            scale_factor,
            pill_pipeline,
            pill_uniform_buffer,
            pill_bind_group,
            icon_pipeline,
            icon_bind_group_layout,
            icon_sampler,
            gpu_icons: Vec::new(),
            dot_pipeline,
            divider_pipeline,
            font_system: {
                let mut fs = cosmic_text::FontSystem::new();
                if let Some((kde_family, file_path)) = detect_kde_font() {
                    let mut resolved_family = kde_family.clone();
                    if let Some(path) = &file_path {
                        if let Ok(bytes) = std::fs::read(path) {
                            let ids = fs
                                .db_mut()
                                .load_font_source(cosmic_text::fontdb::Source::Binary(
                                    std::sync::Arc::new(bytes),
                                ));
                            if let Some(face) = ids.first().and_then(|id| fs.db().face(*id)) {
                                if let Some((db_name, _)) = face.families.first() {
                                    resolved_family = db_name.clone();
                                }
                            }
                        } else {
                            tracing::warn!(?path, "No se pudo leer archivo de fuente KDE");
                        }
                    }
                    fs.db_mut().load_fonts_dir("/usr/local/share/fonts");
                    fs.db_mut().set_sans_serif_family(&resolved_family);
                    let matched = fs
                        .db()
                        .query(&cosmic_text::fontdb::Query {
                            families: &[cosmic_text::Family::SansSerif],
                            weight: cosmic_text::Weight::MEDIUM,
                            ..Default::default()
                        })
                        .and_then(|id| fs.db().face(id))
                        .map(|f| format!("{:?} (weight={:?})", f.families.first().map(|x| &x.0), f.weight));
                    tracing::info!(
                        kde_font = %kde_family,
                        resolved_font = %resolved_family,
                        ?file_path,
                        ?matched,
                        "Tipografía KDE aplicada a tooltips, menús y badges"
                    );
                }
                fs
            },
            swash_cache: cosmic_text::SwashCache::new(),
            tooltip_pipeline,
            menu_pipeline,
            stack_pipeline,
            fan_stack_opacity: 0.0,
            badge_pipeline,
            texture_cache: HashMap::new(),
            cached_icon_px: 0,
            active_icon_keys: Vec::new(),
        })
    }

    /// Sincroniza las texturas de iconos con caché GPU para actualizaciones instantáneas.
    pub fn sync_icons(&mut self, entries: &[DockEntry], max_icon_px: u32) {
        let target_px = max_icon_px.max(64);
        let keys: Vec<String> = entries.iter().map(|e| e.icon_cache_key()).collect();

        if self.cached_icon_px == target_px && self.active_icon_keys == keys {
            return;
        }

        if self.cached_icon_px != target_px {
            self.texture_cache.clear();
            self.cached_icon_px = target_px;
        }

        for (entry, cache_key) in entries.iter().zip(keys.iter()) {
            if self.texture_cache.contains_key(cache_key) {
                continue;
            }

            let decoded = if matches!(entry.kind, dock_core::desktop::DockEntryKind::Spacer { .. }) {
                icons::DecodedIcon {
                    width: 1,
                    height: 1,
                    rgba_premul: vec![0, 0, 0, 0],
                }
            } else {
                icons::load_icon_rgba(entry.icon_path.as_deref(), target_px)
            };
            let tex_size = wgpu::Extent3d {
                width: decoded.width,
                height: decoded.height,
                depth_or_array_layers: 1,
            };

            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&entry.key),
                size: tex_size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            self.queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &decoded.rgba_premul,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * decoded.width),
                    rows_per_image: Some(decoded.height),
                },
                tex_size,
            );

            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.texture_cache.insert(
                cache_key.clone(),
                CachedIconTexture {
                    _texture: texture,
                    view,
                },
            );
        }

        self.gpu_icons.clear();
        for cache_key in &keys {
            let Some(cached) = self.texture_cache.get(cache_key) else {
                continue;
            };

            let init_uniform = IconUniforms {
                rect: [0.0; 4],
                resolution: [self.config.width as f32, self.config.height as f32],
                pressed: 0.0,
                opacity: 1.0,
            };

            let uniform_buffer = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("icon-uniform"),
                    contents: bytemuck::bytes_of(&init_uniform),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });

            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("icon-bg"),
                layout: &self.icon_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&cached.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.icon_sampler),
                    },
                ],
            });

            self.gpu_icons.push(GpuIcon {
                uniform_buffer,
                bind_group,
            });
        }

        self.active_icon_keys = keys;
        tracing::debug!(
            visible = self.gpu_icons.len(),
            cached = self.texture_cache.len(),
            "Iconos sincronizados en GPU"
        );
    }

    pub fn resize(&mut self, phys_width: u32, phys_height: u32, scale_factor: f32) {
        self.scale_factor = scale_factor;
        self.config.width = phys_width.max(1);
        self.config.height = phys_height.max(1);
        self.surface.configure(&self.device, &self.config);
    }

    /// Actualiza la textura del Fan Stack en abanico y lo marca visible.
    pub fn set_fan_stack(
        &mut self,
        data: &dock_core::stack::FanStackData,
        layout: &dock_core::stack::FanLayout,
        hovered_slot: Option<usize>,
        hover_more: bool,
        is_dark: bool,
        opacity: f32,
    ) {
        self.stack_pipeline.update_fan(
            &self.device,
            &self.queue,
            data,
            layout,
            self.config.width,
            self.config.height,
            self.scale_factor,
            hovered_slot,
            hover_more,
            is_dark,
            opacity,
        );
        self.fan_stack_opacity = opacity;
    }

    /// Actualiza y muestra la vista Cuadrícula (Grid) de una carpeta fijada.
    #[allow(clippy::too_many_arguments)]
    pub fn set_grid_stack(
        &mut self,
        data: &dock_core::stack::FanStackData,
        layout: &dock_core::stack::GridLayout,
        hovered_slot: Option<usize>,
        hover_more: bool,
        is_dark: bool,
        custom_tint: Option<[u8; 3]>,
        glass_opacity: f32,
        opacity: f32,
    ) {
        self.stack_pipeline.update_grid(
            &self.device,
            &self.queue,
            data,
            layout,
            self.config.width,
            self.config.height,
            self.scale_factor,
            hovered_slot,
            hover_more,
            is_dark,
            custom_tint,
            glass_opacity,
            opacity,
        );
        self.fan_stack_opacity = opacity;
    }

    /// Actualiza y muestra la vista Lista (List) de una carpeta fijada.
    #[allow(clippy::too_many_arguments)]
    pub fn set_list_stack(
        &mut self,
        data: &dock_core::stack::FanStackData,
        layout: &dock_core::stack::ListLayout,
        hovered_slot: Option<usize>,
        hover_more: bool,
        is_dark: bool,
        custom_tint: Option<[u8; 3]>,
        glass_opacity: f32,
        opacity: f32,
    ) {
        self.stack_pipeline.update_list(
            &self.device,
            &self.queue,
            data,
            layout,
            self.config.width,
            self.config.height,
            self.scale_factor,
            hovered_slot,
            hover_more,
            is_dark,
            custom_tint,
            glass_opacity,
            opacity,
        );
        self.fan_stack_opacity = opacity;
    }

    /// Oculta el Fan Stack actual.
    pub fn clear_fan_stack(&mut self) {
        self.fan_stack_opacity = 0.0;
        self.stack_pipeline.clear();
    }

    pub fn render(&mut self,
        layout: &FrameLayout,
        logical_corner_radius: f32,
        style: GlassStyle,
        blur_active: bool,
    ) {
        let is_dark = style.is_dark;
        let s = self.scale_factor;
        let res = [self.config.width as f32, self.config.height as f32];
        let pill_center = layout.pill.center();

        let pill_uniforms = PillUniforms {
            resolution: res,
            pill_center: [pill_center[0] * s, pill_center[1] * s],
            pill_size: [layout.pill.width * s, layout.pill.height * s],
            corner_radius: logical_corner_radius * s,
            is_dark: style.is_dark,
            scale_factor: s,
            blur_active: if blur_active { 1.0 } else { 0.0 },
            opacity: style.opacity,
            border_strength: style.border_strength,
            custom_tint: style.custom_tint,
        };
        self.queue.write_buffer(
            &self.pill_uniform_buffer,
            0,
            bytemuck::bytes_of(&pill_uniforms),
        );

        for (gpu_icon, slot) in self.gpu_icons.iter().zip(layout.icons.iter()) {
            let icon_uniforms = IconUniforms {
                rect: [slot.x * s, slot.y * s, slot.size * s, slot.size * s],
                resolution: res,
                pressed: if slot.pressed { 1.0 } else { 0.0 },
                opacity: slot.opacity,
            };
            self.queue.write_buffer(
                &gpu_icon.uniform_buffer,
                0,
                bytemuck::bytes_of(&icon_uniforms),
            );
        }

        let dot_count = self.dot_pipeline.prepare(&self.queue, res, is_dark, s, &layout.dots);
        let divider_count = self.divider_pipeline.prepare(&self.queue, res, is_dark, s, &layout.dividers);
        let tooltip_visible = self.tooltip_pipeline.prepare(&self.device, &self.queue, res, is_dark, s, layout.tooltip.as_ref(),
        &mut self.font_system,
            &mut self.swash_cache,
    );
        let menu_visible = self.menu_pipeline.prepare(&self.device, &self.queue, res, is_dark, blur_active, s, layout.menu.as_ref(),
        &mut self.font_system,
            &mut self.swash_cache,
    );
        let badge_count = self.badge_pipeline.prepare(&self.device, &self.queue, res, s, &layout.badges,
        &mut self.font_system,
            &mut self.swash_cache,
    );

        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            Err(e) => {
                tracing::warn!("Error obteniendo textura de superficie: {e}");
                return;
            }
        };

        let view = frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("dock-encoder"),
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("dock-pass"),
                                                     color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                                         view: &view,
                                                         resolve_target: None,
                                                         ops: wgpu::Operations {
                                                             load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                                                               store: wgpu::StoreOp::Store,
                                                         },
                                                     })],
                                                     depth_stencil_attachment: None,
                                                     timestamp_writes: None,
                                                     occlusion_query_set: None,
            });

            // 1. Píldora de cristal
            pass.set_pipeline(&self.pill_pipeline);
            pass.set_bind_group(0, &self.pill_bind_group, &[]);
            pass.draw(0..3, 0..1);

            // 2. Iconos sobre la píldora
            self.divider_pipeline.draw(&mut pass, divider_count);

            pass.set_pipeline(&self.icon_pipeline);
            for gpu_icon in self.gpu_icons.iter().take(layout.icons.len()) {
                pass.set_bind_group(0, &gpu_icon.bind_group, &[]);
                pass.draw(0..6, 0..1);
            }

            // 3. Puntos indicadores de aplicaciones abiertas
            self.dot_pipeline.draw(&mut pass, dot_count);

            // 3.5 Badges de notificaciones
            self.badge_pipeline.draw(&mut pass, badge_count);

            // 4. Tooltip flotante
            self.tooltip_pipeline.draw(&mut pass, tooltip_visible);

            // 5. Menú contextual
            self.menu_pipeline.draw(&mut pass, menu_visible);

            // 6. Fan Stack (abanico de carpeta)
            self.stack_pipeline.draw(&mut pass);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }
}
