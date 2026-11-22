use std::collections::HashMap;
use std::thread;

use calloop::channel::Sender;
use dock_core::desktop::normalize_id;
use dock_wayland::SystemEvent;
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type as MessageType;
use zbus::zvariant::OwnedValue;
use zbus::MatchRule;

/// Inicia un hilo en segundo plano que escucha señales `com.canonical.Unity.LauncherEntry.Update`.
pub fn spawn_badge_monitor(tx: Sender<SystemEvent>) {
    thread::spawn(move || {
        if let Err(err) = run_badge_listener(tx) {
            tracing::warn!("Monitor de badges D-Bus detenido: {err}");
        }
    });
}

fn run_badge_listener(tx: Sender<SystemEvent>) -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::session()?;
    let rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .interface("com.canonical.Unity.LauncherEntry")?
        .member("Update")?
        .build();

    let iter = MessageIterator::for_match_rule(rule, &conn, None)?;
    tracing::info!("Monitor de badges D-Bus (Unity.LauncherEntry) activo");

    for msg_res in iter {
        let Ok(msg) = msg_res else {
            continue;
        };
        let Ok((app_uri, props)) = msg
            .body()
            .deserialize::<(String, HashMap<String, OwnedValue>)>()
        else {
            continue;
        };

        let app_id = parse_launcher_uri(&app_uri);
        if app_id.is_empty() {
            continue;
        }

        let visible = props
            .get("count-visible")
            .and_then(|v| bool::try_from(v).ok())
            .unwrap_or(true);

        let raw_count = props.get("count").and_then(extract_count).unwrap_or(0);
        let count = if visible { raw_count } else { 0 };

        tracing::debug!(%app_id, count, "Badge D-Bus recibido");
        if tx.send(SystemEvent::BadgeUpdate { app_id, count }).is_err() {
            break;
        }
    }

    Ok(())
}

fn parse_launcher_uri(uri: &str) -> String {
    let trimmed = uri
        .trim()
        .strip_prefix("application://")
        .unwrap_or(uri.trim());
    let without_ext = trimmed.strip_suffix(".desktop").unwrap_or(trimmed);
    normalize_id(without_ext)
}

fn extract_count(val: &OwnedValue) -> Option<u64> {
    if let Ok(n) = i64::try_from(val) {
        return Some(n.max(0) as u64);
    }
    if let Ok(n) = u64::try_from(val) {
        return Some(n);
    }
    if let Ok(n) = i32::try_from(val) {
        return Some(n.max(0) as u64);
    }
    if let Ok(n) = u32::try_from(val) {
        return Some(n as u64);
    }
    None
}
