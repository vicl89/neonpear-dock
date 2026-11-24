mod badges;
mod config_watch;
mod trash;

use calloop::channel::{self, Sender};
use dock_core::DockConfig;
use dock_wayland::SystemEvent;
use futures_util::StreamExt;
use tracing_subscriber::EnvFilter;
use zbus::zvariant::{OwnedValue, Value};

#[zbus::proxy(
interface = "org.freedesktop.portal.Settings",
default_service = "org.freedesktop.portal.Desktop",
default_path = "/org/freedesktop/portal/desktop"
)]
trait PortalSettings {
    fn read(&self, namespace: &str, key: &str) -> zbus::Result<OwnedValue>;

    #[zbus(signal)]
    fn setting_changed(
        &self,
        namespace: &str,
        key: &str,
        value: Value<'_>,
    ) -> zbus::Result<()>;
}

fn extract_u32(val: &Value<'_>) -> Option<u32> {
    match val {
        Value::U32(n) => Some(*n),
        Value::Value(inner) => extract_u32(inner),
        _ => None,
    }
}

async fn watch_plasma_color_scheme(tx: Sender<SystemEvent>) -> zbus::Result<()> {
    let conn = zbus::Connection::session().await?;
    let proxy = PortalSettingsProxy::new(&conn).await?;

    if let Ok(initial) = proxy.read("org.freedesktop.appearance", "color-scheme").await {
        if let Some(scheme) = extract_u32(&initial) {
            let _ = tx.send(SystemEvent::SetDark(scheme == 1));
        }
    }

    let mut stream = proxy.receive_setting_changed().await?;
    while let Some(signal) = stream.next().await {
        if let Ok(args) = signal.args() {
            if args.namespace == "org.freedesktop.appearance" && args.key == "color-scheme" {
                if let Some(scheme) = extract_u32(&args.value) {
                    if tx.send(SystemEvent::SetDark(scheme == 1)).is_err() {
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _instance_lock = match acquire_single_instance_lock() {
        Some(lock) => lock,
        None => {
            eprintln!("NeonPear Dock ya está en ejecución.");
            std::process::exit(0);
        }
    };

    tracing_subscriber::fmt()
    .with_env_filter(
        EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,wgpu_hal=error,wgpu_core=warn,naga=warn")),
    )
    .init();

    let config = DockConfig::load_or_create();
    if let Some(path) = DockConfig::config_path() {
        tracing::info!(?path, "Configuración cargada");
    }

    let catalog = dock_core::desktop::AppCatalog::scan();
    let apps = catalog.resolve_pinned(&config.pinned_apps);
    let app_names: Vec<&str> = apps.iter().map(|a| a.name.as_str()).collect();
    tracing::info!(count = apps.len(), ?app_names, "Aplicaciones fijadas resueltas");

    let (theme_tx, theme_rx) = channel::channel::<SystemEvent>();
    badges::spawn_badge_monitor(theme_tx.clone());
    let trash_tx = theme_tx.clone();
    let config_tx = theme_tx.clone();
    let dnd_tx = theme_tx.clone();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("No se pudo iniciar Tokio para D-Bus");

        if let Err(err) = rt.block_on(watch_plasma_color_scheme(theme_tx)) {
            tracing::warn!("Monitor de tema D-Bus detenido: {err}");
        }
    });

    let trash_full = dock_core::desktop::is_trash_non_empty();
    let _trash_watcher = if config.show_trash {
        trash::spawn_trash_watcher(trash_tx, trash_full)
    } else {
        None
    };
    let _config_watcher = config_watch::spawn_config_watcher(config_tx);
    dock_wayland::run_wayland_loop(config, catalog, apps, trash_full, dnd_tx, theme_rx)?;
    Ok(())
}

fn acquire_single_instance_lock() -> Option<std::os::unix::net::UnixListener> {
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::{SocketAddr, UnixListener};

    let scope = std::env::var("XDG_RUNTIME_DIR")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "default".to_string());
    let name = format!("neonpear-dock-instance-{scope}");
    let addr = SocketAddr::from_abstract_name(&name).ok()?;
    UnixListener::bind_addr(&addr).ok()
}
