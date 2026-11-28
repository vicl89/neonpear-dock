use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use calloop::channel::Sender;
use dock_core::DockConfig;
use dock_wayland::SystemEvent;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

fn read_kde_font_line(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    content
        .lines()
        .find(|line| line.starts_with("font="))
        .map(|line| line.trim().to_string())
}

pub fn spawn_config_watcher(tx: Sender<SystemEvent>) -> Option<RecommendedWatcher> {
    let base_config_dir: PathBuf = dirs::config_dir()?;
    let config_path: PathBuf = base_config_dir.join("neonpear-dock").join("config.toml");
    let watch_dir = config_path.parent()?.to_path_buf();
    let kdeglobals_path: PathBuf = base_config_dir.join("kdeglobals");

    if let Err(err) = fs::create_dir_all(&watch_dir) {
        tracing::warn!(?watch_dir, "No se pudo asegurar directorio de config: {err}");
        return None;
    }

    let target_name = config_path.file_name()?.to_os_string();
    let config_path_cb = config_path.clone();
    let kdeglobals_path_cb = kdeglobals_path.clone();
    let mut last_reload = Instant::now() - Duration::from_secs(1);
    let mut last_font_reload = Instant::now() - Duration::from_secs(1);
    let mut last_kde_font = read_kde_font_line(&kdeglobals_path);

    let mut watcher = match notify::recommended_watcher(move |res: notify::Result<Event>| {
        let Ok(event) = res else { return };
        if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
            return;
        }

        let touches_config = event
            .paths
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n == target_name));

        if touches_config && last_reload.elapsed() >= Duration::from_millis(120) {
            last_reload = Instant::now();
            if let Ok(raw) = fs::read_to_string(&config_path_cb) {
                if !raw.trim().is_empty() {
                    match toml::from_str::<DockConfig>(&raw) {
                        Ok(cfg) => {
                            let _ = tx.send(SystemEvent::ConfigReloaded(cfg));
                        }
                        Err(err) => {
                            tracing::warn!(
                                ?config_path_cb,
                                "config.toml modificado con error de sintaxis: {err}"
                            );
                        }
                    }
                }
            }
        }

        let touches_kdeglobals = event
            .paths
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n == "kdeglobals"));

        if touches_kdeglobals && last_font_reload.elapsed() >= Duration::from_millis(200) {
            let current_font = read_kde_font_line(&kdeglobals_path_cb);
            if current_font.is_some() && current_font != last_kde_font {
                last_kde_font = current_font;
                last_font_reload = Instant::now();
                let _ = tx.send(SystemEvent::SystemFontChanged);
            }
        }
    }) {
        Ok(w) => w,
        Err(err) => {
            tracing::warn!("No se pudo inicializar watcher de config.toml: {err}");
            return None;
        }
    };

    if let Err(err) = watcher.watch(&watch_dir, RecursiveMode::NonRecursive) {
        tracing::warn!(?watch_dir, "No se pudo vigilar directorio de config.toml: {err}");
        return None;
    }

    if let Err(err) = watcher.watch(&base_config_dir, RecursiveMode::NonRecursive) {
        tracing::warn!(?base_config_dir, "No se pudo vigilar kdeglobals para recarga de fuentes: {err}");
    }

    tracing::info!(?config_path, ?kdeglobals_path, "Hot-reload de config.toml y tipografía KDE activo");
    Some(watcher)
}
