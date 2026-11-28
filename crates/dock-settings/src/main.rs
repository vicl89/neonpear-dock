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
            let on_disk = DockConfig::load_or_create();
            if self.tab != Tab::Pinned && !on_disk.pinned_apps.is_empty() {
                self.config.pinned_apps = on_disk.pinned_apps;
            }
            self.config.pinned_folders = on_disk.pinned_folders;
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

fn read_kdeglobals() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let kdeglobals = std::path::PathBuf::from(home)
        .join(".config")
        .join("kdeglobals");
    std::fs::read_to_string(kdeglobals).ok()
}

/// Devuelve `(patrón fontconfig, tamaño en puntos pt)` desde `~/.config/kdeglobals`.
fn detect_kde_font_info() -> (String, f32) {
    if let Some(content) = read_kdeglobals() {
        for line in content.lines() {
            if let Some(val) = line.strip_prefix("font=") {
                let parts: Vec<&str> = val.split(',').collect();
                let family = parts.first().map(|s| s.trim()).unwrap_or("");
                let size_pt = parts
                    .get(1)
                    .and_then(|s| s.trim().parse::<f32>().ok())
                    .filter(|&pt| (7.0..=24.0).contains(&pt))
                    .unwrap_or(10.0);
                if !family.is_empty() {
                    let style = parts.get(16).map(|s| s.trim()).unwrap_or("");
                    let pattern = if style.is_empty() {
                        family.to_string()
                    } else {
                        format!("{family}:style={style}")
                    };
                    return (pattern, size_pt);
                }
            }
        }
    }
    ("sans-serif".to_string(), 10.0)
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

    let (primary_pattern, size_pt) = detect_kde_font_info();
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

    // Convertir tamaño en puntos (pt) de KDE a píxeles lógicos de egui (96 DPI base)
    let base_px = (size_pt * (96.0 / 72.0)).clamp(11.0, 22.0);
    ctx.style_mut(|style| {
        style.text_styles.insert(
            egui::TextStyle::Small,
            egui::FontId::proportional((base_px * 0.86).max(10.0)),
        );
        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::proportional(base_px),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::proportional(base_px),
        );
        style.text_styles.insert(
            egui::TextStyle::Monospace,
            egui::FontId::monospace(base_px * 0.95),
        );
        style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::proportional(base_px * 1.22),
        );
        style.spacing.item_spacing = egui::vec2(8.0, 7.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.interact_size.y = (base_px * 1.75).max(24.0);
    });
}

fn parse_rgb_csv(raw: &str) -> Option<egui::Color32> {
    let mut parts = raw.split(',').map(|s| s.trim().parse::<u8>().ok());
    let r = parts.next()??;
    let g = parts.next()??;
    let b = parts.next()??;
    Some(egui::Color32::from_rgb(r, g, b))
}

fn parse_kde_color(content: &str, section: &str, key: &str) -> Option<egui::Color32> {
    let header = format!("[{section}]");
    let mut in_section = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_section = trimmed == header;
            continue;
        }
        if in_section {
            if let Some(val) = trimmed.strip_prefix(&format!("{key}=")) {
                return parse_rgb_csv(val);
            }
        }
    }
    None
}

fn blend_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let mix = |x: u8, y: u8| ((x as f32) * (1.0 - t) + (y as f32) * t).round() as u8;
    egui::Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

fn is_color_dark(c: egui::Color32) -> bool {
    let lum = 0.2126 * (c.r() as f32) + 0.7152 * (c.g() as f32) + 0.0722 * (c.b() as f32);
    lum < 128.0
}

fn apply_kde_theme(ctx: &egui::Context) {
    let kde = read_kdeglobals();
    let window_bg = kde
        .as_deref()
        .and_then(|c| parse_kde_color(c, "Colors:Window", "BackgroundNormal"))
        .unwrap_or_else(|| egui::Color32::from_rgb(24, 24, 37));
    let view_bg = kde
        .as_deref()
        .and_then(|c| parse_kde_color(c, "Colors:View", "BackgroundNormal"))
        .unwrap_or_else(|| egui::Color32::from_rgb(30, 30, 46));
    let accent = kde
        .as_deref()
        .and_then(|c| parse_kde_color(c, "Colors:Selection", "BackgroundNormal"))
        .unwrap_or_else(|| egui::Color32::from_rgb(203, 166, 247));
    let dark = is_color_dark(window_bg);

    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    let fg = kde
        .as_deref()
        .and_then(|c| parse_kde_color(c, "Colors:Window", "ForegroundNormal"))
        .unwrap_or_else(|| {
            if dark {
                egui::Color32::from_rgb(205, 214, 244)
            } else {
                egui::Color32::from_rgb(35, 38, 41)
            }
        });
    let accent_fg = kde
        .as_deref()
        .and_then(|c| parse_kde_color(c, "Colors:Selection", "ForegroundNormal"))
        .unwrap_or_else(|| {
            if is_color_dark(accent) {
                egui::Color32::WHITE
            } else {
                egui::Color32::from_rgb(17, 17, 27)
            }
        });

    let control_bg = blend_color(view_bg, fg, if dark { 0.08 } else { 0.06 });
    let control_hover = blend_color(view_bg, fg, if dark { 0.15 } else { 0.12 });
    let border_col = blend_color(window_bg, fg, if dark { 0.16 } else { 0.18 });

    visuals.override_text_color = Some(fg);
    visuals.window_fill = window_bg;
    visuals.panel_fill = window_bg;
    visuals.faint_bg_color = view_bg;
    visuals.extreme_bg_color = blend_color(window_bg, egui::Color32::BLACK, if dark { 0.25 } else { 0.05 });

    visuals.widgets.noninteractive.bg_fill = view_bg;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, border_col);
    visuals.widgets.inactive.bg_fill = control_bg;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, border_col);
    visuals.widgets.inactive.rounding = egui::Rounding::same(6.0);
    visuals.widgets.hovered.bg_fill = control_hover;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, accent);
    visuals.widgets.hovered.rounding = egui::Rounding::same(6.0);
    visuals.widgets.active.bg_fill = accent;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, accent_fg);
    visuals.widgets.active.rounding = egui::Rounding::same(6.0);

    visuals.selection.bg_fill = accent;
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, accent_fg);
    visuals.window_rounding = egui::Rounding::same(10.0);

    ctx.set_visuals(visuals);
}

/// Contenedor tipo Kirigami `FormCard` para agrupar ajustes relacionados.
fn form_card(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    let visuals = ui.visuals().clone();
    let frame = egui::Frame::none()
        .fill(visuals.faint_bg_color)
        .stroke(visuals.widgets.noninteractive.bg_stroke)
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(14.0, 12.0));

    frame.show(ui, |ui| {
        let w = ui.available_width();
        ui.set_min_width(w);
        ui.set_max_width(w);
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        ui.label(egui::RichText::new(title.trim_end_matches(':')).strong());
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(6.0);
        add_contents(ui);
    });
    ui.add_space(10.0);
}

/// Deslizador con etiqueta superior y ancho adaptado a la tarjeta (evita empalmes).
fn labeled_slider<Num: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Num,
    range: std::ops::RangeInclusive<Num>,
) -> bool {
    ui.label(label);
    let prev = ui.spacing().slider_width;
    ui.spacing_mut().slider_width = (ui.available_width() - 78.0).max(90.0);
    let changed = ui.add(egui::Slider::new(value, range)).changed();
    ui.spacing_mut().slider_width = prev;
    ui.add_space(4.0);
    changed
}

/// Botón de navegación estilo lista lateral de Preferencias del Sistema de KDE.
fn sidebar_nav_item(ui: &mut egui::Ui, selected: bool, label: &str) -> bool {
    let row_h = (ui.text_style_height(&egui::TextStyle::Body) * 2.1).max(32.0);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), row_h), egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let bg = if selected {
            visuals.selection.bg_fill
        } else if response.hovered() {
            visuals.widgets.hovered.bg_fill
        } else {
            egui::Color32::TRANSPARENT
        };
        let text_color = if selected {
            visuals.selection.stroke.color
        } else {
            visuals.text_color()
        };

        ui.painter()
            .rect_filled(rect, egui::Rounding::same(6.0), bg);
        ui.painter().text(
            egui::pos2(rect.left() + 12.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::TextStyle::Body.resolve(ui.style()),
            text_color,
        );
    }

    response.clicked()
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let t = settings_strings(self.config.language);

        // Barra inferior estilo KDE (Restaurar a la izquierda; Aplicar / Aceptar a la derecha)
        egui::TopBottomPanel::bottom("settings_footer")
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .inner_margin(egui::Margin::symmetric(14.0, 10.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(t.restore_defaults).clicked() {
                        let pinned = self.config.pinned_apps.clone();
                        self.config = DockConfig::default();
                        self.config.pinned_apps = pinned;
                        self.dirty = true;
                    }
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
            });

        // Barra lateral izquierda estilo Preferencias del Sistema de KDE
        let sidebar_bg = blend_color(
            ctx.style().visuals.window_fill,
            ctx.style().visuals.extreme_bg_color,
            0.35,
        );
        egui::SidePanel::left("kde_sidebar")
            .resizable(false)
            .exact_width(185.0)
            .frame(
                egui::Frame::none()
                    .fill(sidebar_bg)
                    .stroke(ctx.style().visuals.widgets.noninteractive.bg_stroke)
                    .inner_margin(egui::Margin::symmetric(10.0, 14.0)),
            )
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("NeonPear Dock").heading().strong());
                ui.label(
                    egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                        .small()
                        .weak(),
                );
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);

                for (tab, label) in [
                    (Tab::Appearance, t.tab_appearance),
                    (Tab::Behavior, t.tab_behavior),
                    (Tab::Pinned, t.tab_pinned),
                    (Tab::About, t.tab_about),
                ] {
                    if sidebar_nav_item(ui, self.tab == tab, label) {
                        self.tab = tab;
                    }
                    ui.add_space(2.0);
                }
            });

        // Área principal con encabezado de sección y tarjetas en 2 columnas
        egui::CentralPanel::default()
            .frame(
                egui::Frame::central_panel(&ctx.style())
                    .inner_margin(egui::Margin::symmetric(18.0, 14.0)),
            )
            .show(ctx, |ui| {
                let page_title = match self.tab {
                    Tab::Appearance => t.tab_appearance,
                    Tab::Behavior => t.tab_behavior,
                    Tab::Pinned => t.tab_pinned,
                    Tab::About => t.tab_about,
                };
                ui.heading(page_title);
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(10.0);

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.tab {
                        Tab::Appearance => {
                            ui.columns(2, |cols| {
                                // Columna izquierda: Tamaño/Magnificación + Geometría
                                form_card(&mut cols[0], t.sec_size_mag, |ui| {
                                    if labeled_slider(
                                        ui,
                                        t.icon_base_size,
                                        &mut self.config.icon_size,
                                        28.0..=96.0,
                                    )
                                    {
                                        if self.config.max_icon_size < self.config.icon_size {
                                            self.config.max_icon_size = self.config.icon_size;
                                        }
                                        self.dirty = true;
                                    }
                                    if ui
                                        .checkbox(
                                            &mut self.config.magnification_enabled,
                                            t.enable_magnification,
                                        )
                                        .changed()
                                    {
                                        self.dirty = true;
                                    }
                                    ui.add_enabled_ui(self.config.magnification_enabled, |ui| {
                                        if labeled_slider(
                                            ui,
                                            t.icon_max_size,
                                            &mut self.config.max_icon_size,
                                            self.config.icon_size..=128.0,
                                        )
                                        {
                                            self.dirty = true;
                                        }
                                    });
                                });

                                form_card(&mut cols[0], t.sec_geometry_glass, |ui| {
                                    if labeled_slider(
                                        ui,
                                        t.icon_spacing,
                                        &mut self.config.icon_spacing,
                                        2.0..=24.0,
                                    )
                                    {
                                        self.dirty = true;
                                    }
                                    if labeled_slider(
                                        ui,
                                        t.pill_padding_x,
                                        &mut self.config.pill_padding_x,
                                        4.0..=32.0,
                                    )
                                    {
                                        self.dirty = true;
                                    }
                                    if labeled_slider(
                                        ui,
                                        t.pill_padding_y,
                                        &mut self.config.pill_padding_y,
                                        4.0..=28.0,
                                    )
                                    {
                                        self.dirty = true;
                                    }
                                    if labeled_slider(
                                        ui,
                                        t.corner_radius,
                                        &mut self.config.corner_radius,
                                        0.0..=36.0,
                                    )
                                    {
                                        self.dirty = true;
                                    }
                                    if labeled_slider(
                                        ui,
                                        t.bottom_margin,
                                        &mut self.config.bottom_margin,
                                        0..=40,
                                    )
                                    {
                                        self.dirty = true;
                                    }
                                });

                                // Columna derecha: Tema cristal y desenfoque
                                form_card(&mut cols[1], t.glass_theme, |ui| {
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
                                                    if ui
                                                        .selectable_value(
                                                            &mut self.config.glass_theme,
                                                            val,
                                                            text,
                                                        )
                                                        .changed()
                                                    {
                                                        self.dirty = true;
                                                    }
                                                }
                                            });
                                    });
                                    if self.config.glass_theme == GlassTheme::Custom {
                                        ui.horizontal(|ui| {
                                            ui.label(t.tint_color);
                                            if ui
                                                .color_edit_button_srgb(&mut self.config.custom_tint)
                                                .changed()
                                            {
                                                self.dirty = true;
                                            }
                                        });
                                    }
                                    if labeled_slider(
                                        ui,
                                        t.glass_opacity,
                                        &mut self.config.glass_opacity,
                                        0.0..=1.0,
                                    )
                                    {
                                        self.dirty = true;
                                    }
                                    if labeled_slider(
                                        ui,
                                        t.border_intensity,
                                        &mut self.config.border_intensity,
                                        0.0..=2.0,
                                    )
                                    {
                                        self.dirty = true;
                                    }
                                    ui.horizontal(|ui| {
                                        ui.label(t.blur_mode);
                                        egui::ComboBox::from_id_salt("blur_mode")
                                            .selected_text(format!("{:?}", self.config.blur_mode))
                                            .show_ui(ui, |ui| {
                                                for mode in [
                                                    BlurMode::Auto,
                                                    BlurMode::Kwin,
                                                    BlurMode::External,
                                                    BlurMode::None,
                                                ] {
                                                    if ui
                                                        .selectable_value(
                                                            &mut self.config.blur_mode,
                                                            mode,
                                                            format!("{mode:?}"),
                                                        )
                                                        .changed()
                                                    {
                                                        self.dirty = true;
                                                    }
                                                }
                                            });
                                    });
                                });
                            });
                        }

                        Tab::Behavior => {
                            ui.columns(2, |cols| {
                                // Columna izquierda: Idioma + Ocultado + Posición/Monitor
                                form_card(&mut cols[0], t.sec_language, |ui| {
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
                                                        let new_t = settings_strings(
                                                            self.config.language,
                                                        );
                                                        ctx.send_viewport_cmd(
                                                            egui::ViewportCommand::Title(
                                                                new_t.window_title.to_owned(),
                                                            ),
                                                        );
                                                    }
                                                }
                                            });
                                    });
                                });

                                form_card(&mut cols[0], t.sec_hiding, |ui| {
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
                                                for (val, text) in [
                                                    (HideMode::None, t.hide_none),
                                                    (HideMode::AutoHide, t.hide_auto),
                                                    (HideMode::DodgeWindows, t.hide_dodge_windows),
                                                    (HideMode::DodgeActive, t.hide_dodge_active),
                                                ] {
                                                    if ui
                                                        .selectable_value(
                                                            &mut self.config.hide_mode,
                                                            val,
                                                            text,
                                                        )
                                                        .changed()
                                                    {
                                                        self.dirty = true;
                                                    }
                                                }
                                            });
                                    });
                                    if labeled_slider(
                                        ui,
                                        t.auto_hide_delay,
                                        &mut self.config.auto_hide_delay_ms,
                                        0..=2000,
                                    )
                                    {
                                        self.dirty = true;
                                    }
                                });

                                form_card(&mut cols[0], t.screen_position, |ui| {
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
                                                    if ui
                                                        .selectable_value(
                                                            &mut self.config.position,
                                                            val,
                                                            text,
                                                        )
                                                        .changed()
                                                    {
                                                        self.dirty = true;
                                                    }
                                                }
                                            });
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label(t.monitor_optional);
                                        let mut mon =
                                            self.config.monitor.clone().unwrap_or_default();
                                        if ui.text_edit_singleline(&mut mon).changed() {
                                            let trimmed = mon.trim().to_string();
                                            self.config.monitor = if trimmed.is_empty() {
                                                None
                                            } else {
                                                Some(trimmed)
                                            };
                                            self.dirty = true;
                                        }
                                    });
                                });

                                // Columna derecha: Indicadores + Secciones del dock
                                form_card(&mut cols[1], t.sec_indicators, |ui| {
                                    for (val, text) in [
                                        (&mut self.config.show_indicators, t.show_indicators),
                                        (&mut self.config.show_badges, t.show_badges),
                                        (&mut self.config.animate_opening, t.animate_opening),
                                        (
                                            &mut self.config.bounce_on_attention,
                                            t.bounce_on_attention,
                                        ),
                                        (&mut self.config.dim_minimized, t.dim_minimized),
                                        (&mut self.config.show_tooltips, t.show_tooltips),
                                    ] {
                                        if ui.checkbox(val, text).changed() {
                                            self.dirty = true;
                                        }
                                    }
                                });

                                form_card(&mut cols[1], t.sec_dock_sections, |ui| {
                                    if ui.checkbox(&mut self.autostart, t.autostart).changed() {
                                        self.dirty = true;
                                    }
                                    if ui
                                        .checkbox(&mut self.config.show_launchpad, t.show_launchpad)
                                        .changed()
                                    {
                                        self.dirty = true;
                                    }
                                    if ui
                                        .checkbox(&mut self.config.show_trash, t.show_trash)
                                        .changed()
                                    {
                                        self.dirty = true;
                                    }
                                    if ui
                                        .checkbox(
                                            &mut self.config.show_mounted_drives,
                                            t.show_mounted_drives,
                                        )
                                        .changed()
                                    {
                                        self.dirty = true;
                                    }
                                    if ui
                                        .checkbox(
                                            &mut self.config.active_apps_only,
                                            t.active_apps_only,
                                        )
                                        .changed()
                                    {
                                        self.dirty = true;
                                    }
                                    ui.add_enabled_ui(!self.config.active_apps_only, |ui| {
                                        if ui
                                            .checkbox(
                                                &mut self.config.show_recent_apps,
                                                t.show_recent_apps,
                                            )
                                            .changed()
                                        {
                                            self.dirty = true;
                                        }
                                    });
                                    ui.add_enabled_ui(
                                        self.config.show_recent_apps
                                            && !self.config.active_apps_only,
                                        |ui| {
                                            if labeled_slider(
                                                ui,
                                                t.max_recent_apps,
                                                &mut self.config.max_recent_apps,
                                                0..=10,
                                            )
                                            {
                                                self.dirty = true;
                                            }
                                        },
                                    );
                                });
                            });
                        }

                        Tab::Pinned => {
                            form_card(ui, t.sec_pinned, |ui| {
                                ui.label(egui::RichText::new(t.pinned_instant_hint).weak());
                                ui.add_space(6.0);

                                let mut swap_op: Option<(usize, usize)> = None;
                                let mut remove_idx: Option<usize> = None;
                                let len = self.config.pinned_apps.len();

                                for (idx, app_id) in self.config.pinned_apps.iter().enumerate() {
                                    ui.horizontal(|ui| {
                                        ui.label(format!("{}. {app_id}", idx + 1));
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui.button(t.remove).clicked() {
                                                    remove_idx = Some(idx);
                                                }
                                                if ui
                                                    .add_enabled(
                                                        idx + 1 < len,
                                                        egui::Button::new("⬇"),
                                                    )
                                                    .clicked()
                                                {
                                                    swap_op = Some((idx, idx + 1));
                                                }
                                                if ui
                                                    .add_enabled(idx > 0, egui::Button::new("⬆"))
                                                    .clicked()
                                                {
                                                    swap_op = Some((idx, idx - 1));
                                                }
                                            },
                                        );
                                    });
                                    if idx + 1 < len {
                                        ui.separator();
                                    }
                                }

                                if let Some((a, b)) = swap_op {
                                    self.config.pinned_apps.swap(a, b);
                                    self.dirty = true;
                                }
                                if let Some(idx) = remove_idx {
                                    self.config.pinned_apps.remove(idx);
                                    self.dirty = true;
                                }
                            });

                            form_card(ui, t.add_desktop_id, |ui| {
                                ui.horizontal(|ui| {
                                    let resp = ui.text_edit_singleline(&mut self.new_app_id);
                                    let enter = resp.lost_focus()
                                        && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                    if (ui.button(t.add).clicked() || enter)
                                        && !self.new_app_id.trim().is_empty()
                                    {
                                        self.config
                                            .pinned_apps
                                            .push(self.new_app_id.trim().to_string());
                                        self.new_app_id.clear();
                                        self.dirty = true;
                                    }
                                });
                            });
                        }

                        Tab::About => {
                            form_card(ui, "NeonPear Dock", |ui| {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{}{}{}",
                                        t.about_subtitle_prefix,
                                        env!("CARGO_PKG_VERSION"),
                                        t.about_subtitle_suffix
                                    ))
                                    .weak(),
                                );
                                ui.add_space(8.0);

                                egui::Grid::new("about_grid")
                                    .num_columns(2)
                                    .min_col_width(120.0)
                                    .spacing([24.0, 8.0])
                                    .show(ui, |ui| {
                                        ui.style_mut().wrap_mode =
                                            Some(egui::TextWrapMode::Extend);
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
                                                ctx.copy_text(
                                                    "fernando.vidriales.89@gmail.com".to_owned(),
                                                );
                                                self.copied_email = true;
                                            }
                                            if self.copied_email {
                                                ui.label(
                                                    egui::RichText::new(t.copied)
                                                        .small()
                                                        .color(egui::Color32::from_rgb(
                                                            166, 227, 161,
                                                        )),
                                                );
                                            }
                                        });
                                        ui.end_row();
                                    });

                                ui.add_space(12.0);
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

                                ui.add_space(8.0);
                                ui.label(egui::RichText::new(t.about_footer).small().weak());
                            });
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
            .with_inner_size([960.0, 620.0])
            .with_min_inner_size([760.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "neonpear-dock-settings",
        options,
        Box::new(|cc| {
            configure_system_font(&cc.egui_ctx);
            apply_kde_theme(&cc.egui_ctx);
            Ok(Box::new(SettingsApp::new()))
        }),
    )
}
