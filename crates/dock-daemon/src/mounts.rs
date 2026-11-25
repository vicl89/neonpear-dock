use std::collections::HashMap;
use std::thread;

use calloop::channel::Sender;
use dock_core::MountedDrive;
use dock_wayland::SystemEvent;
use zbus::blocking::{Connection, MessageIterator, Proxy};
use zbus::message::Type as MessageType;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::MatchRule;

type ManagedObjects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

/// Inicia un hilo en segundo plano que monitorea unidades extraíbles montadas vía UDisks2.
pub fn spawn_mounts_watcher(tx: Sender<SystemEvent>) {
    thread::spawn(move || {
        if let Err(err) = run_mounts_listener(tx) {
            tracing::warn!("Monitor de unidades USB (UDisks2) detenido: {err}");
        }
    });
}

fn run_mounts_listener(tx: Sender<SystemEvent>) -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::system()?;

    let mut last_drives = query_mounted_drives(&conn).unwrap_or_default();
    let _ = tx.send(SystemEvent::MountedDrivesChanged(last_drives.clone()));

    let rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .sender("org.freedesktop.UDisks2")?
        .build();

    let iter = MessageIterator::for_match_rule(rule, &conn, None)?;
    tracing::info!("Monitor de unidades USB (UDisks2) activo");

    for msg_res in iter {
        let Ok(_msg) = msg_res else {
            continue;
        };
        if let Ok(drives) = query_mounted_drives(&conn) {
            if drives != last_drives {
                last_drives = drives.clone();
                if tx.send(SystemEvent::MountedDrivesChanged(drives)).is_err() {
                    break;
                }
            }
        }
    }

    Ok(())
}

/// Consulta a UDisks2 todas las particiones extraíbles actualmente montadas.
pub fn query_mounted_drives(conn: &Connection) -> Result<Vec<MountedDrive>, zbus::Error> {
    let proxy = Proxy::new(
        conn,
        "org.freedesktop.UDisks2",
        "/org/freedesktop/UDisks2",
        "org.freedesktop.DBus.ObjectManager",
    )?;
    let objects: ManagedObjects = proxy.call("GetManagedObjects", &())?;

    let mut drives = Vec::new();

    for (block_path, ifaces) in &objects {
        let Some(fs) = ifaces.get("org.freedesktop.UDisks2.Filesystem") else {
            continue;
        };
        let Some(block) = ifaces.get("org.freedesktop.UDisks2.Block") else {
            continue;
        };

        if get_bool(block, "HintIgnore") || get_bool(block, "HintSystem") {
            continue;
        }

        let Some(mount_point) = get_first_mount_point(fs) else {
            continue;
        };

        let drive_path = get_object_path(block, "Drive").unwrap_or_else(|| "/".to_string());
        if drive_path == "/" {
            continue;
        }
        let Some(drive_obj) = objects
            .iter()
            .find(|(p, _)| p.as_str() == drive_path)
            .and_then(|(_, i)| i.get("org.freedesktop.UDisks2.Drive"))
        else {
            continue;
        };

        let removable = get_bool(drive_obj, "Removable") || get_bool(drive_obj, "MediaRemovable");
        let bus = get_string(drive_obj, "ConnectionBus").unwrap_or_default();
        if !removable && bus != "usb" && bus != "sdio" {
            continue;
        }

        let id_label = get_string(block, "IdLabel").unwrap_or_default();
        let model = get_string(drive_obj, "Model").unwrap_or_default();
        let label = if !id_label.trim().is_empty() {
            id_label
        } else if !model.trim().is_empty() {
            model
        } else {
            mount_point
                .rsplit('/')
                .next()
                .unwrap_or("USB")
                .to_string()
        };

        let icon_name = if bus == "sdio" {
            "media-flash-sd-mmc".to_string()
        } else {
            "drive-removable-media-usb".to_string()
        };

        drives.push(MountedDrive {
            block_path: block_path.as_str().to_string(),
            drive_path,
            label,
            mount_point,
            icon_name,
        });
    }

    drives.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
    Ok(drives)
}

fn get_bool(map: &HashMap<String, OwnedValue>, key: &str) -> bool {
    map.get(key)
        .and_then(|v| v.downcast_ref::<bool>().ok())
        .unwrap_or(false)
}

fn get_string(map: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| String::try_from(v.clone()).ok())
}

fn get_object_path(map: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(|v| OwnedObjectPath::try_from(v.clone()).ok())
        .map(|p| p.as_str().to_string())
}

fn get_first_mount_point(fs: &HashMap<String, OwnedValue>) -> Option<String> {
    let val = fs.get("MountPoints")?;
    let points = Vec::<Vec<u8>>::try_from(val.clone()).ok()?;
    let first = points.into_iter().next()?;
    let trimmed: Vec<u8> = first.into_iter().take_while(|&b| b != 0).collect();
    String::from_utf8(trimmed).ok().filter(|s| !s.is_empty())
}
