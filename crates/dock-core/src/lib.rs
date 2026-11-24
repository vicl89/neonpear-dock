pub mod desktop;
pub mod menu;
pub mod physics;

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DockPosition {
    Bottom,
    Top,
    Left,
    Right,
}

impl DockPosition {
    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GlassTheme {
    Auto,
    Dark,
    Light,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlurMode {
    Auto,
    Kwin,
    External,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HideMode {
    /// Siempre visible; reserva zona exclusiva en el borde inferior.
    None,
    /// Se oculta siempre que el cursor sale del dock.
    AutoHide,
    /// Ocultamiento inteligente: se oculta solo cuando una ventana maximizada o flotante toca el dock.
    DodgeWindows,
    /// Ocultamiento inteligente solo frente a la ventana activa.
    DodgeActive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DockConfig {
    pub namespace: String,
    pub icon_size: f32,
    pub max_icon_size: f32,
    pub magnification_enabled: bool,
    pub show_indicators: bool,
    pub show_badges: bool,
    pub animate_opening: bool,
    pub bounce_on_attention: bool,
    pub dim_minimized: bool,
    pub show_tooltips: bool,
    pub hide_mode: HideMode,
    /// Opción heredada para compatibilidad con configuraciones previas.
    pub auto_hide: bool,
    pub auto_hide_delay_ms: u64,
    pub active_apps_only: bool,
    pub show_recent_apps: bool,
    pub max_recent_apps: usize,
    pub show_launchpad: bool,
    pub show_trash: bool,
    pub icon_spacing: f32,
    pub pill_padding_x: f32,
    pub pill_padding_y: f32,
    pub corner_radius: f32,
    pub bottom_margin: i32,
    pub position: DockPosition,
    pub monitor: Option<String>,
    pub blur_mode: BlurMode,
    pub glass_theme: GlassTheme,
    pub glass_opacity: f32,
    pub border_intensity: f32,
    pub custom_tint: [u8; 3],
    pub pinned_apps: Vec<String>,
}

impl Default for DockConfig {
    fn default() -> Self {
        Self {
            namespace: "neonpear-dock".to_string(),
            icon_size: 48.0,
            max_icon_size: 76.0,
            magnification_enabled: true,
            show_indicators: true,
            show_badges: true,
            animate_opening: true,
            bounce_on_attention: true,
            dim_minimized: true,
            show_tooltips: true,
            hide_mode: HideMode::DodgeWindows,
            auto_hide: false,
            auto_hide_delay_ms: 450,
            active_apps_only: false,
            show_recent_apps: true,
            max_recent_apps: 3,
            show_launchpad: false,
            show_trash: true,
            icon_spacing: 8.0,
            pill_padding_x: 12.0,
            pill_padding_y: 10.0,
            corner_radius: 20.0,
            bottom_margin: 10,
            position: DockPosition::Bottom,
            monitor: None,
            blur_mode: BlurMode::Auto,
            glass_theme: GlassTheme::Auto,
            glass_opacity: 0.50,
            border_intensity: 1.0,
            custom_tint: [30, 30, 46],
            pinned_apps: vec![
                "org.kde.dolphin".into(),
                "kitty".into(),
                "systemsettings".into(),
                "org.kde.discover".into(),
            ],
        }
    }
}

impl DockConfig {
    pub fn config_path() -> Option<PathBuf> {
        Some(dirs::config_dir()?.join("neonpear-dock").join("config.toml"))
    }

    pub fn effective_hide_mode(&self) -> HideMode {
        if self.auto_hide && self.hide_mode == HideMode::DodgeWindows {
            HideMode::AutoHide
        } else {
            self.hide_mode
        }
    }

    pub fn hiding_enabled(&self) -> bool {
        self.effective_hide_mode() != HideMode::None
    }

    pub fn load_or_create() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };

        if let Ok(raw) = fs::read_to_string(&path) {
            match toml::from_str::<Self>(&raw) {
                Ok(cfg) => return cfg,
                Err(err) => {
                    tracing::warn!(?path, "config.toml inválido ({err}); usando valores por defecto")
                }
            }
        }

        let cfg = Self::default();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(serialized) = toml::to_string_pretty(&cfg) {
            let _ = fs::write(&path, serialized);
        }
        cfg
    }

    pub fn save(&self) {
        let Some(path) = Self::config_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match toml::to_string_pretty(self) {
            Ok(serialized) => {
                let tmp_path = path.with_extension("toml.tmp");
                if let Err(err) = fs::write(&tmp_path, serialized)
                    .and_then(|_| fs::rename(&tmp_path, &path))
                {
                    let _ = fs::remove_file(&tmp_path);
                    tracing::error!(?path, "No se pudo guardar config.toml: {err}");
                } else {
                    tracing::info!(?path, "Configuración guardada");
                }
            }
            Err(err) => tracing::error!("No se pudo serializar config.toml: {err}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ThemeTransition {
    pub current: f32,
    pub target: f32,
    pub speed: f32,
}

impl Default for ThemeTransition {
    fn default() -> Self {
        Self {
            current: 0.0,
            target: 0.0,
            speed: 8.0,
        }
    }
}

impl ThemeTransition {
    pub fn set_dark(&mut self, dark: bool) {
        self.target = if dark { 1.0 } else { 0.0 };
    }

    pub fn is_animating(&self) -> bool {
        (self.current - self.target).abs() > 0.002
    }

    pub fn step(&mut self, dt_secs: f32) -> bool {
        let diff = self.target - self.current;
        if diff.abs() <= 0.002 {
            self.current = self.target;
            return false;
        }
        let factor = (1.0 - (-self.speed * dt_secs).exp()).clamp(0.0, 1.0);
        self.current += diff * factor;
        self.is_animating()
    }
}
