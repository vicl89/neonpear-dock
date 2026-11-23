use dock_core::{BlurMode, DockConfig, HideMode};
use eframe::egui;

#[derive(PartialEq, Eq, Clone, Copy)]
enum Tab {
    Appearance,
    Behavior,
    Pinned,
}

struct SettingsApp {
    config: DockConfig,
    tab: Tab,
    new_app_id: String,
    dirty: bool,
}

impl SettingsApp {
    fn new() -> Self {
        Self {
            config: DockConfig::load_or_create(),
            tab: Tab::Appearance,
            new_app_id: String::new(),
            dirty: false,
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
            self.dirty = false;
        }
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
