use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use calloop::channel::Sender;
use dock_core::DockConfig;
use dock_wayland::SystemEvent;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

pub fn spawn_config_watcher(tx: Sender<SystemEvent>) -> Option<RecommendedWatcher> {
    let config_path: PathBuf = dirs::config_dir()?.join("neonpear-dock").join("config.toml");
    let watch_dir = config_path.parent()?.to_path_buf();

    if let Err(err) = fs::create_dir_all(&watch_dir) {
        tracing::warn!(?watch_dir, "No se pudo asegurar directorio de config: {err}");
        return None;
    }

    let target_name = config_path.file_name()?.to_os_string();
    let config_path_cb = config_path.clone();
    let mut last_reload = Instant::now() - Duration::from_secs(1);

    let mut watcher = match notify::recommended_watcher(move |res: notify::Result<Event>| {
        let Ok(event) = res else { return };
        if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
            return;
        }
        let touches_config = event
            .paths
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n == target_name));
        if !touches_config {
            return;
        }

        // Pequeño debounce para evitar lecturas duplicadas al guardar
        if last_reload.elapsed() < Duration::from_millis(120) {
            return;
        }
        last_reload = Instant::now();

        let Ok(raw) = fs::read_to_string(&config_path_cb) else {
            return;
        };
        if raw.trim().is_empty() {
            return;
        }
        match toml::from_str::<DockConfig>(&raw) {
            Ok(cfg) => {
                let _ = tx.send(SystemEvent::ConfigReloaded(cfg));
            }
            Err(err) => {
                tracing::warn!(?config_path_cb, "config.toml modificado con error de sintaxis: {err}");
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

    tracing::info!(?config_path, "Hot-reload de config.toml activo");
    Some(watcher)
}
