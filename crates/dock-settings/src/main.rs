use dock_core::{BlurMode, DockConfig, DockPosition, GlassTheme, HideMode};
use eframe::egui;

#[derive(PartialEq, Eq, Clone, Copy)]
enum Tab {
    Appearance,
    Behavior,
    Pinned,
    About,
}

struct SettingsApp {
    config: DockConfig,
    tab: Tab,
    new_app_id: String,
    autostart: bool,
    dirty: bool,
    copied_email: bool,
}

impl SettingsApp {
    fn new() -> Self {
        Self {
            config: DockConfig::load_or_create(),
            tab: Tab::Appearance,
            new_app_id: String::new(),
            autostart: is_autostart_enabled(),
            dirty: false,
            copied_email: false,
        }
    }

    fn save_if_dirty(&mut self) {
        if self.dirty {
            // Si no estamos editando la pestaña de Apps fijadas, preservar los cambios
            // hechos desde el dock (Drag & Drop o menú contextual)
            if self.tab != Tab::Pinned {
                let on_disk = DockConfig::load_or_create();
                if !on_disk.pinned_apps.is_empty() {
                    self.config.pinned_apps = on_disk.pinned_apps;
                }
            }
            self.config.auto_hide = self.config.hide_mode == HideMode::AutoHide;
            self.config.save();
            sync_autostart_file(self.autostart);
            self.dirty = false;
        }
    }
}

fn autostart_desktop_path() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").ok()?;
    if home.is_empty() {
        return None;
    }
    Some(
        std::path::PathBuf::from(home)
            .join(".config")
            .join("autostart")
            .join("neonpear-dock.desktop"),
    )
}

fn is_autostart_enabled() -> bool {
    autostart_desktop_path()
        .map(|path| path.exists())
        .unwrap_or(false)
}

fn resolve_dock_binary_path() -> String {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("neonpear-dock");
            if sibling.exists() {
                return sibling.to_string_lossy().into_owned();
            }
        }
    }
    for candidate in ["/usr/local/bin/neonpear-dock", "/usr/bin/neonpear-dock"] {
        if std::path::Path::new(candidate).exists() {
            return candidate.to_string();
        }
    }
    std::env::current_exe()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "neonpear-dock".to_string())
}

fn sync_autostart_file(enabled: bool) {
    let Some(path) = autostart_desktop_path() else {
        return;
    };
    if enabled {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let exec_path = resolve_dock_binary_path();
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName=NeonPear Dock\nComment=Wayland Dock for KDE Plasma\nExec={}\nIcon=neonpear-dock\nTerminal=false\nX-KDE-autostart-after=panel\n",
            exec_path
        );
        let _ = std::fs::write(&path, entry);
    } else if path.exists() {
        let _ = std::fs::remove_file(&path);
    }
}

fn detect_kde_font_pattern() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let kdeglobals = std::path::PathBuf::from(home)
        .join(".config")
        .join("kdeglobals");
    if let Ok(content) = std::fs::read_to_string(kdeglobals) {
        for line in content.lines() {
            if let Some(val) = line.strip_prefix("font=") {
                let parts: Vec<&str> = val.split(',').collect();
                let family = parts.first().map(|s| s.trim()).unwrap_or("");
                if !family.is_empty() {
                    // parts[16] suele contener el estilo (ej. Regular, Medium)
                    let style = parts.get(16).map(|s| s.trim()).unwrap_or("");
                    if !style.is_empty() {
                        return format!("{family}:style={style}");
                    }
                    return family.to_string();
                }
            }
        }
    }
    "sans-serif".to_string()
}

fn configure_system_font(ctx: &egui::Context) {
    let pattern = detect_kde_font_pattern();
    let Ok(output) = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", &pattern])
        .output()
    else {
        return;
    };
    if !output.status.success() {
        return;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return;
    }
    if let Ok(bytes) = std::fs::read(&path) {
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "system_sans".to_owned(),
            egui::FontData::from_owned(bytes),
        );
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.insert(0, "system_sans".to_owned());
        }
        ctx.set_fonts(fonts);
    }
}

fn apply_dark_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = egui::Color32::from_rgb(24, 24, 37);
    visuals.panel_fill = egui::Color32::from_rgb(24, 24, 37);
    visuals.extreme_bg_color = egui::Color32::from_rgb(17, 17, 27);
    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(36, 36, 54);
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(49, 50, 68);
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(203, 166, 247);
    visuals.selection.bg_fill = egui::Color32::from_rgb(203, 166, 247);
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(17, 17, 27));
    visuals.window_rounding = egui::Rounding::same(12.0);
    ctx.set_visuals(visuals);
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Appearance, "Apariencia");
                ui.selectable_value(&mut self.tab, Tab::Behavior, "Comportamiento");
                ui.selectable_value(&mut self.tab, Tab::Pinned, "Apps fijadas");
                ui.selectable_value(&mut self.tab, Tab::About, "Acerca de");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Restaurar por defecto").clicked() {
                        let pinned = self.config.pinned_apps.clone();
                        self.config = DockConfig::default();
                        self.config.pinned_apps = pinned;
                        self.dirty = true;
                    }
                });
            });
            ui.add_space(4.0);
        });

        egui::TopBottomPanel::bottom("settings_footer").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Aceptar").clicked() {
                        self.save_if_dirty();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui.add_enabled(self.dirty, egui::Button::new("Aplicar")).clicked() {
                        self.save_if_dirty();
                    }
                });
            });
            ui.add_space(6.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 10.0;
                match self.tab {
                    Tab::Appearance => {
                        ui.heading("Tamaño y magnificación");
                        if ui
                            .add(egui::Slider::new(&mut self.config.icon_size, 28.0..=96.0).text("Tamaño base de iconos (px)"))
                            .changed()
                        {
                            if self.config.max_icon_size < self.config.icon_size {
                                self.config.max_icon_size = self.config.icon_size;
                            }
                            self.dirty = true;
                        }
                        if ui
                            .checkbox(&mut self.config.magnification_enabled, "Habilitar magnificación al pasar el cursor")
                            .changed()
                        {
                            self.dirty = true;
                        }
                        ui.add_enabled_ui(self.config.magnification_enabled, |ui| {
                            if ui
                                .add(
                                    egui::Slider::new(
                                        &mut self.config.max_icon_size,
                                        self.config.icon_size..=128.0,
                                    )
                                    .text("Tamaño máximo al magnificar (px)"),
                                )
                                .changed()
                            {
                                self.dirty = true;
                            }
                        });

                        ui.separator();
                        ui.heading("Geometría y cristal");
                        if ui
                            .add(egui::Slider::new(&mut self.config.icon_spacing, 2.0..=24.0).text("Espaciado entre iconos (px)"))
                            .changed()
                        {
                            self.dirty = true;
                        }
                        if ui
                            .add(egui::Slider::new(&mut self.config.pill_padding_x, 4.0..=32.0).text("Margen interno horizontal (px)"))
                            .changed()
                        {
                            self.dirty = true;
                        }
                        if ui
                            .add(egui::Slider::new(&mut self.config.pill_padding_y, 4.0..=28.0).text("Margen interno vertical (px)"))
                            .changed()
                        {
                            self.dirty = true;
                        }
                        if ui
                            .add(egui::Slider::new(&mut self.config.corner_radius, 0.0..=36.0).text("Radio de esquinas (px)"))
                            .changed()
                        {
                            self.dirty = true;
                        }
                        if ui
                            .add(egui::Slider::new(&mut self.config.bottom_margin, 0..=40).text("Margen inferior (px)"))
                            .changed()
                        {
                            self.dirty = true;
                        }

                        ui.horizontal(|ui| {
                            ui.label("Tema del cristal:");
                            let theme_label = match self.config.glass_theme {
                                GlassTheme::Auto => "Automático (Plasma)",
                                GlassTheme::Dark => "Oscuro",
                                GlassTheme::Light => "Claro",
                                GlassTheme::Custom => "Personalizado",
                            };
                            egui::ComboBox::from_id_salt("glass_theme")
                                .selected_text(theme_label)
                                .show_ui(ui, |ui| {
                                    for (val, text) in [
                                        (GlassTheme::Auto, "Automático (Plasma)"),
                                        (GlassTheme::Dark, "Oscuro"),
                                        (GlassTheme::Light, "Claro"),
                                        (GlassTheme::Custom, "Personalizado"),
                                    ] {
                                        if ui.selectable_value(&mut self.config.glass_theme, val, text).changed() {
                                            self.dirty = true;
                                        }
                                    }
                                });
                        });
                        if self.config.glass_theme == GlassTheme::Custom {
                            ui.horizontal(|ui| {
                                ui.label("Color de tinte:");
                                if ui.color_edit_button_srgb(&mut self.config.custom_tint).changed() {
                                    self.dirty = true;
                                }
                            });
                        }
                        if ui
                            .add(egui::Slider::new(&mut self.config.glass_opacity, 0.0..=1.0).text("Opacidad del cristal"))
                            .changed()
                        {
                            self.dirty = true;
                        }
                        if ui
                            .add(egui::Slider::new(&mut self.config.border_intensity, 0.0..=2.0).text("Intensidad del borde"))
                            .changed()
                        {
                            self.dirty = true;
                        }

                        ui.horizontal(|ui| {
                            ui.label("Modo de desenfoque (blur):");
                            egui::ComboBox::from_id_salt("blur_mode")
                                .selected_text(format!("{:?}", self.config.blur_mode))
                                .show_ui(ui, |ui| {
                                    for mode in [BlurMode::Auto, BlurMode::Kwin, BlurMode::External, BlurMode::None] {
                                        if ui
                                            .selectable_value(&mut self.config.blur_mode, mode, format!("{mode:?}"))
                                            .changed()
                                        {
                                            self.dirty = true;
                                        }
                                    }
                                });
                        });
                    }
                    Tab::Behavior => {
                        ui.heading("Ocultamiento");
                        ui.horizontal(|ui| {
                            ui.label("Modo de ocultamiento:");
                            let label = match self.config.hide_mode {
                                HideMode::None => "Siempre visible",
                                HideMode::AutoHide => "Ocultamiento automático",
                                HideMode::DodgeWindows => "Esquivar ventanas",
                                HideMode::DodgeActive => "Esquivar ventana activa",
                            };
                            egui::ComboBox::from_id_salt("hide_mode")
                                .selected_text(label)
                                .show_ui(ui, |ui| {
                                    let options = [
                                        (HideMode::None, "Siempre visible"),
                                        (HideMode::AutoHide, "Ocultamiento automático"),
                                        (HideMode::DodgeWindows, "Esquivar ventanas"),
                                        (HideMode::DodgeActive, "Esquivar ventana activa"),
                                    ];
                                    for (val, text) in options {
                                        if ui.selectable_value(&mut self.config.hide_mode, val, text).changed() {
                                            self.dirty = true;
                                        }
                                    }
                                });
                        });
                        if ui
                            .add(egui::Slider::new(&mut self.config.auto_hide_delay_ms, 0..=2000).text("Retraso al ocultar (ms)"))
                            .changed()
                        {
                            self.dirty = true;
                        }

                        ui.separator();
                        ui.heading("Indicadores y animaciones");
                        for (val, text) in [
                            (&mut self.config.show_indicators, "Mostrar indicadores de apps abiertas"),
                            (&mut self.config.show_badges, "Mostrar contadores (badges)"),
                            (&mut self.config.animate_opening, "Rebote al abrir aplicación"),
                            (&mut self.config.bounce_on_attention, "Rebote al pedir atención"),
                            (&mut self.config.dim_minimized, "Atenuar iconos de ventanas minimizadas"),
                            (&mut self.config.show_tooltips, "Mostrar etiquetas (tooltips)"),
                        ] {
                            if ui.checkbox(val, text).changed() {
                                self.dirty = true;
                            }
                        }

                        ui.separator();
                        ui.heading("Secciones del dock");
                        ui.horizontal(|ui| {
                            ui.label("Posición en pantalla:");
                            let pos_label = match self.config.position {
                                DockPosition::Bottom => "Abajo",
                                DockPosition::Top => "Arriba",
                                DockPosition::Left => "Izquierda",
                                DockPosition::Right => "Derecha",
                            };
                            egui::ComboBox::from_id_salt("dock_position")
                                .selected_text(pos_label)
                                .show_ui(ui, |ui| {
                                    for (val, text) in [
                                        (DockPosition::Bottom, "Abajo"),
                                        (DockPosition::Top, "Arriba"),
                                        (DockPosition::Left, "Izquierda"),
                                        (DockPosition::Right, "Derecha"),
                                    ] {
                                        if ui.selectable_value(&mut self.config.position, val, text).changed() {
                                            self.dirty = true;
                                        }
                                    }
                                });
                        });
                        ui.horizontal(|ui| {
                            ui.label("Monitor (opcional, ej. eDP-1 / HDMI-A-1):");
                            let mut mon = self.config.monitor.clone().unwrap_or_default();
                            if ui.text_edit_singleline(&mut mon).changed() {
                                let trimmed = mon.trim().to_string();
                                self.config.monitor = if trimmed.is_empty() { None } else { Some(trimmed) };
                                self.dirty = true;
                            }
                        });
                        ui.separator();
                        if ui.checkbox(&mut self.autostart, "Iniciar automáticamente al iniciar sesión").changed() {
                            self.dirty = true;
                        }
                        if ui.checkbox(&mut self.config.show_launchpad, "Mostrar botón Launchpad").changed() {
                            self.dirty = true;
                        }
                        if ui.checkbox(&mut self.config.show_trash, "Mostrar Papelera").changed() {
                            self.dirty = true;
                        }
                        if ui.checkbox(&mut self.config.active_apps_only, "Solo aplicaciones activas").changed() {
                            self.dirty = true;
                        }
                        ui.add_enabled_ui(!self.config.active_apps_only, |ui| {
                            if ui.checkbox(&mut self.config.show_recent_apps, "Mostrar aplicaciones recientes").changed() {
                                self.dirty = true;
                            }
                        });
                        ui.add_enabled_ui(self.config.show_recent_apps && !self.config.active_apps_only, |ui| {
                            if ui
                                .add(egui::Slider::new(&mut self.config.max_recent_apps, 0..=10).text("Máximo de apps recientes"))
                                .changed()
                            {
                                self.dirty = true;
                            }
                        });
                    }
                    Tab::Pinned => {
                        ui.heading("Aplicaciones fijadas");
                        ui.label("Los cambios se reflejan en el dock al instante.");

                        let mut swap_op: Option<(usize, usize)> = None;
                        let mut remove_idx: Option<usize> = None;
                        let len = self.config.pinned_apps.len();

                        for (idx, app_id) in self.config.pinned_apps.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(format!("{}. {app_id}", idx + 1));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button("Quitar").clicked() {
                                        remove_idx = Some(idx);
                                    }
                                    if ui.add_enabled(idx + 1 < len, egui::Button::new("⬇")).clicked() {
                                        swap_op = Some((idx, idx + 1));
                                    }
                                    if ui.add_enabled(idx > 0, egui::Button::new("⬆")).clicked() {
                                        swap_op = Some((idx, idx - 1));
                                    }
                                });
                            });
                        }

                        if let Some((a, b)) = swap_op {
                            self.config.pinned_apps.swap(a, b);
                            self.dirty = true;
                        }
                        if let Some(idx) = remove_idx {
                            self.config.pinned_apps.remove(idx);
                            self.dirty = true;
                        }

                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label("Añadir ID .desktop:");
                            let resp = ui.text_edit_singleline(&mut self.new_app_id);
                            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if (ui.button("Añadir").clicked() || enter) && !self.new_app_id.trim().is_empty() {
                                self.config.pinned_apps.push(self.new_app_id.trim().to_string());
                                self.new_app_id.clear();
                                self.dirty = true;
                            }
                        });
                    }
                
                    Tab::About => {
                        ui.add_space(6.0);
                        ui.heading("NeonPear Dock");
                        ui.label(
                            egui::RichText::new(format!(
                                "Versión v{} — Dock nativo para Wayland y KDE Plasma 6",
                                env!("CARGO_PKG_VERSION")
                            ))
                            .weak(),
                        );

                        ui.add_space(10.0);
                        ui.separator();
                        ui.add_space(10.0);

                        egui::Grid::new("about_grid")
                            .num_columns(2)
                            .spacing([16.0, 8.0])
                            .show(ui, |ui| {
                                ui.strong("Autor:");
                                ui.label("FVidriales");
                                ui.end_row();

                                ui.strong("Licencia:");
                                ui.label("MIT");
                                ui.end_row();

                                ui.strong("Tecnología:");
                                ui.label("Rust · Wayland (wlr-layer-shell) · wgpu");
                                ui.end_row();

                                ui.strong("Contacto:");
                                ui.horizontal(|ui| {
                                    ui.label("fernando.vidriales.89@gmail.com");
                                    if ui.small_button("Copiar").clicked() {
                                        ctx.copy_text("fernando.vidriales.89@gmail.com".to_owned());
                                        self.copied_email = true;
                                    }
                                    if self.copied_email {
                                        ui.label(
                                            egui::RichText::new("¡Copiado!")
                                                .small()
                                                .color(egui::Color32::from_rgb(166, 227, 161)),
                                        );
                                    }
                                });
                                ui.end_row();
                            });

                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            ui.hyperlink_to(
                                "Repositorio en GitHub",
                                "https://github.com/vicl89/neonpear-dock",
                            );
                            ui.label("·");
                            ui.hyperlink_to(
                                "Apoyar en PayPal",
                                "https://paypal.me/fvidriales",
                            );
                        });

                        ui.add_space(12.0);
                        ui.label(
                            egui::RichText::new(
                                "Anteriormente conocido como Glassdock. Gracias por usar NeonPear Dock.",
                            )
                            .small()
                            .weak(),
                        );
                    }
}
            });
        });

    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Configuración de NeonPear Dock")
            .with_app_id("neonpear-dock-settings")
            .with_inner_size([520.0, 540.0])
            .with_min_inner_size([440.0, 420.0]),
        ..Default::default()
    };
    eframe::run_native(
        "neonpear-dock-settings",
        options,
        Box::new(|cc| {
            configure_system_font(&cc.egui_ctx);
            apply_dark_theme(&cc.egui_ctx);
            Ok(Box::new(SettingsApp::new()))
        }),
    )
}
