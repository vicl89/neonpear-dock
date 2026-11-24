use dock_core::i18n_settings::settings_strings;
use dock_core::{BlurMode, DockConfig, DockPosition, GlassTheme, HideMode, Language};
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

fn resolve_font_path(pattern: &str) -> Option<String> {
    let output = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", pattern])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() { None } else { Some(path) }
}

fn configure_system_font(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut loaded_paths = std::collections::HashSet::new();
    let mut loaded_names = Vec::new();

    let primary_pattern = detect_kde_font_pattern();
    let candidates = [
        ("system_sans", primary_pattern.as_str()),
        ("fallback_cyrillic", "sans-serif:lang=ru"),
        ("fallback_cjk_zh", "sans-serif:lang=zh"),
        ("fallback_cjk_ja", "sans-serif:lang=ja"),
        ("fallback_cjk_ko", "sans-serif:lang=ko"),
    ];

    for (name, pattern) in candidates {
        if let Some(path) = resolve_font_path(pattern) {
            if loaded_paths.insert(path.clone()) {
                if let Ok(bytes) = std::fs::read(&path) {
                    fonts
                        .font_data
                        .insert(name.to_owned(), egui::FontData::from_owned(bytes));
                    loaded_names.push(name.to_owned());
                }
            }
        }
    }

    if !loaded_names.is_empty() {
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            for (i, name) in loaded_names.into_iter().enumerate() {
                family.insert(i, name);
            }
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
        let t = settings_strings(self.config.language);
        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Appearance, t.tab_appearance);
                ui.selectable_value(&mut self.tab, Tab::Behavior, t.tab_behavior);
                ui.selectable_value(&mut self.tab, Tab::Pinned, t.tab_pinned);
                ui.selectable_value(&mut self.tab, Tab::About, t.tab_about);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t.restore_defaults).clicked() {
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
                    if ui.button(t.ok).clicked() {
                        self.save_if_dirty();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui.add_enabled(self.dirty, egui::Button::new(t.apply)).clicked() {
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
                        ui.heading(t.sec_size_mag);
                        if ui
                            .add(egui::Slider::new(&mut self.config.icon_size, 28.0..=96.0).text(t.icon_base_size))
                            .changed()
                        {
                            if self.config.max_icon_size < self.config.icon_size {
                                self.config.max_icon_size = self.config.icon_size;
                            }
                            self.dirty = true;
                        }
                        if ui
                            .checkbox(&mut self.config.magnification_enabled, t.enable_magnification)
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
                                    .text(t.icon_max_size),
                                )
                                .changed()
                            {
                                self.dirty = true;
                            }
                        });

                        ui.separator();
                        ui.heading(t.sec_geometry_glass);
                        if ui
                            .add(egui::Slider::new(&mut self.config.icon_spacing, 2.0..=24.0).text(t.icon_spacing))
                            .changed()
                        {
                            self.dirty = true;
                        }
                        if ui
                            .add(egui::Slider::new(&mut self.config.pill_padding_x, 4.0..=32.0).text(t.pill_padding_x))
                            .changed()
                        {
                            self.dirty = true;
                        }
                        if ui
                            .add(egui::Slider::new(&mut self.config.pill_padding_y, 4.0..=28.0).text(t.pill_padding_y))
                            .changed()
                        {
                            self.dirty = true;
                        }
                        if ui
                            .add(egui::Slider::new(&mut self.config.corner_radius, 0.0..=36.0).text(t.corner_radius))
                            .changed()
                        {
                            self.dirty = true;
                        }
                        if ui
                            .add(egui::Slider::new(&mut self.config.bottom_margin, 0..=40).text(t.bottom_margin))
                            .changed()
                        {
                            self.dirty = true;
                        }

                        ui.horizontal(|ui| {
                            ui.label(t.glass_theme);
                            let theme_label = match self.config.glass_theme {
                                GlassTheme::Auto => t.theme_auto,
                                GlassTheme::Dark => t.theme_dark,
                                GlassTheme::Light => t.theme_light,
                                GlassTheme::Custom => t.theme_custom,
                            };
                            egui::ComboBox::from_id_salt("glass_theme")
                                .selected_text(theme_label)
                                .show_ui(ui, |ui| {
                                    for (val, text) in [
                                        (GlassTheme::Auto, t.theme_auto),
                                        (GlassTheme::Dark, t.theme_dark),
                                        (GlassTheme::Light, t.theme_light),
                                        (GlassTheme::Custom, t.theme_custom),
                                    ] {
                                        if ui.selectable_value(&mut self.config.glass_theme, val, text).changed() {
                                            self.dirty = true;
                                        }
                                    }
                                });
                        });
                        if self.config.glass_theme == GlassTheme::Custom {
                            ui.horizontal(|ui| {
                                ui.label(t.tint_color);
                                if ui.color_edit_button_srgb(&mut self.config.custom_tint).changed() {
                                    self.dirty = true;
                                }
                            });
                        }
                        if ui
                            .add(egui::Slider::new(&mut self.config.glass_opacity, 0.0..=1.0).text(t.glass_opacity))
                            .changed()
                        {
                            self.dirty = true;
                        }
                        if ui
                            .add(egui::Slider::new(&mut self.config.border_intensity, 0.0..=2.0).text(t.border_intensity))
                            .changed()
                        {
                            self.dirty = true;
                        }

                        ui.horizontal(|ui| {
                            ui.label(t.blur_mode);
                            egui::ComboBox::from_id_salt("blur_mode")
                                .selected_text(format!("{:?}", self.config.blur_mode))
                                .show_ui(ui, |ui| {
                                    for mode in [BlurMode::Auto, BlurMode::Kwin, BlurMode::External, BlurMode::None] {
                                        if ui
                                            .selectable_value(&mut self.config.blur_mode, mode, format!("{mode:?}"))
                                            .changed()
                                        {
                                            self.dirty = true;
                                            let new_t = settings_strings(self.config.language);
                                            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                                                new_t.window_title.to_owned(),
                                            ));
                                        }
                                    }
                                });
                        });
                    }
                    Tab::Behavior => {
                        ui.heading(t.sec_language);
                        ui.horizontal(|ui| {
                            ui.label(t.ui_language);
                            let current_lang = self.config.language;
                            egui::ComboBox::from_id_salt("language_selector")
                                .selected_text(current_lang.native_name(current_lang))
                                .show_ui(ui, |ui| {
                                    for &lang in Language::ALL {
                                        if ui
                                            .selectable_value(
                                                &mut self.config.language,
                                                lang,
                                                lang.native_name(current_lang),
                                            )
                                            .changed()
                                        {
                                            self.dirty = true;
                                        }
                                    }
                                });
                        });
                        ui.add_space(10.0);
                        ui.separator();
                        ui.add_space(10.0);

                        ui.heading(t.sec_hiding);
                        ui.horizontal(|ui| {
                            ui.label(t.hide_mode);
                            let label = match self.config.hide_mode {
                                HideMode::None => t.hide_none,
                                HideMode::AutoHide => t.hide_auto,
                                HideMode::DodgeWindows => t.hide_dodge_windows,
                                HideMode::DodgeActive => t.hide_dodge_active,
                            };
                            egui::ComboBox::from_id_salt("hide_mode")
                                .selected_text(label)
                                .show_ui(ui, |ui| {
                                    let options = [
                                        (HideMode::None, t.hide_none),
                                        (HideMode::AutoHide, t.hide_auto),
                                        (HideMode::DodgeWindows, t.hide_dodge_windows),
                                        (HideMode::DodgeActive, t.hide_dodge_active),
                                    ];
                                    for (val, text) in options {
                                        if ui.selectable_value(&mut self.config.hide_mode, val, text).changed() {
                                            self.dirty = true;
                                        }
                                    }
                                });
                        });
                        if ui
                            .add(egui::Slider::new(&mut self.config.auto_hide_delay_ms, 0..=2000).text(t.auto_hide_delay))
                            .changed()
                        {
                            self.dirty = true;
                        }

                        ui.separator();
                        ui.heading(t.sec_indicators);
                        for (val, text) in [
                            (&mut self.config.show_indicators, t.show_indicators),
                            (&mut self.config.show_badges, t.show_badges),
                            (&mut self.config.animate_opening, t.animate_opening),
                            (&mut self.config.bounce_on_attention, t.bounce_on_attention),
                            (&mut self.config.dim_minimized, t.dim_minimized),
                            (&mut self.config.show_tooltips, t.show_tooltips),
                        ] {
                            if ui.checkbox(val, text).changed() {
                                self.dirty = true;
                            }
                        }

                        ui.separator();
                        ui.heading(t.sec_dock_sections);
                        ui.horizontal(|ui| {
                            ui.label(t.screen_position);
                            let pos_label = match self.config.position {
                                DockPosition::Bottom => t.pos_bottom,
                                DockPosition::Top => t.pos_top,
                                DockPosition::Left => t.pos_left,
                                DockPosition::Right => t.pos_right,
                            };
                            egui::ComboBox::from_id_salt("dock_position")
                                .selected_text(pos_label)
                                .show_ui(ui, |ui| {
                                    for (val, text) in [
                                        (DockPosition::Bottom, t.pos_bottom),
                                        (DockPosition::Top, t.pos_top),
                                        (DockPosition::Left, t.pos_left),
                                        (DockPosition::Right, t.pos_right),
                                    ] {
                                        if ui.selectable_value(&mut self.config.position, val, text).changed() {
                                            self.dirty = true;
                                        }
                                    }
                                });
                        });
                        ui.horizontal(|ui| {
                            ui.label(t.monitor_optional);
                            let mut mon = self.config.monitor.clone().unwrap_or_default();
                            if ui.text_edit_singleline(&mut mon).changed() {
                                let trimmed = mon.trim().to_string();
                                self.config.monitor = if trimmed.is_empty() { None } else { Some(trimmed) };
                                self.dirty = true;
                            }
                        });
                        ui.separator();
                        if ui.checkbox(&mut self.autostart, t.autostart).changed() {
                            self.dirty = true;
                        }
                        if ui.checkbox(&mut self.config.show_launchpad, t.show_launchpad).changed() {
                            self.dirty = true;
                        }
                        if ui.checkbox(&mut self.config.show_trash, t.show_trash).changed() {
                            self.dirty = true;
                        }
                        if ui.checkbox(&mut self.config.active_apps_only, t.active_apps_only).changed() {
                            self.dirty = true;
                        }
                        ui.add_enabled_ui(!self.config.active_apps_only, |ui| {
                            if ui.checkbox(&mut self.config.show_recent_apps, t.show_recent_apps).changed() {
                                self.dirty = true;
                            }
                        });
                        ui.add_enabled_ui(self.config.show_recent_apps && !self.config.active_apps_only, |ui| {
                            if ui
                                .add(egui::Slider::new(&mut self.config.max_recent_apps, 0..=10).text(t.max_recent_apps))
                                .changed()
                            {
                                self.dirty = true;
                            }
                        });
                    }
                    Tab::Pinned => {
                        ui.heading(t.sec_pinned);
                        ui.label(t.pinned_instant_hint);

                        let mut swap_op: Option<(usize, usize)> = None;
                        let mut remove_idx: Option<usize> = None;
                        let len = self.config.pinned_apps.len();

                        for (idx, app_id) in self.config.pinned_apps.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(format!("{}. {app_id}", idx + 1));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button(t.remove).clicked() {
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
                            ui.label(t.add_desktop_id);
                            let resp = ui.text_edit_singleline(&mut self.new_app_id);
                            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if (ui.button(t.add).clicked() || enter) && !self.new_app_id.trim().is_empty() {
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
                                "{}{}{}",
                                t.about_subtitle_prefix,
                                env!("CARGO_PKG_VERSION"),
                                t.about_subtitle_suffix
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
                                ui.strong(t.about_author);
                                ui.label("FVidriales");
                                ui.end_row();

                                ui.strong(t.about_license);
                                ui.label("MIT");
                                ui.end_row();

                                ui.strong(t.about_tech);
                                ui.label("Rust · Wayland (wlr-layer-shell) · wgpu");
                                ui.end_row();

                                ui.strong(t.about_contact);
                                ui.horizontal(|ui| {
                                    ui.label("fernando.vidriales.89@gmail.com");
                                    if ui.small_button(t.copy).clicked() {
                                        ctx.copy_text("fernando.vidriales.89@gmail.com".to_owned());
                                        self.copied_email = true;
                                    }
                                    if self.copied_email {
                                        ui.label(
                                            egui::RichText::new(t.copied)
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
                                t.github_repo,
                                "https://github.com/vicl89/neonpear-dock",
                            );
                            ui.label("·");
                            ui.hyperlink_to(
                                t.support_paypal,
                                "https://paypal.me/fvidriales",
                            );
                        });

                        ui.add_space(12.0);
                        ui.label(
                            egui::RichText::new(
                                t.about_footer,
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
            .with_title(settings_strings(DockConfig::load_or_create().language).window_title)
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
