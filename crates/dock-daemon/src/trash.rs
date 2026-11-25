use std::fs;

use calloop::channel::Sender;
use dock_core::desktop::{is_trash_non_empty, trash_files_dir};
use dock_wayland::SystemEvent;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// Inicia un monitor en tiempo real sobre `~/.local/share/Trash/files`.
/// Devuelve el watcher para mantenerlo vivo durante toda la sesión del dock.
pub fn spawn_trash_watcher(
    tx: Sender<SystemEvent>,
    initial_full: bool,
) -> Option<RecommendedWatcher> {
    let dir = trash_files_dir()?;
    if let Err(err) = fs::create_dir_all(&dir) {
        tracing::warn!(?dir, "No se pudo preparar el directorio de Papelera: {err}");
        return None;
    }

    let mut last_full = initial_full;
    let mut watcher = match notify::recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(event) = res {
            if !matches!(event.kind, EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_)) {
                return;
            }
            let now_full = is_trash_non_empty();
            if now_full != last_full {
                last_full = now_full;
                let _ = tx.send(SystemEvent::TrashChanged(now_full));
            }
        }
    }) {
        Ok(w) => w,
        Err(err) => {
            tracing::warn!("No se pudo crear el monitor de Papelera: {err}");
            return None;
        }
    };

    if let Err(err) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
        tracing::warn!(?dir, "No se pudo vigilar la Papelera: {err}");
        return None;
    }

    tracing::info!(?dir, full = initial_full, "Monitor de Papelera activo");
    Some(watcher)
}
