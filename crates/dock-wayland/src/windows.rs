use std::collections::HashMap;

use dock_core::desktop::{normalize_id, DesktopApp, DockEntry};
use wayland_client::{
    backend::ObjectId,
    globals::GlobalList,
    protocol::wl_surface::WlSurface,
    Dispatch, Proxy, QueueHandle,
};
use wayland_protocols_plasma::plasma_window_management::client::{
    org_kde_plasma_window::{Event as WindowEvent, OrgKdePlasmaWindow},
    org_kde_plasma_window_management::{Event as ManagerEvent, OrgKdePlasmaWindowManagement},
};

const STATE_ACTIVE: u32 = 0x1;
const STATE_MINIMIZED: u32 = 0x2;
const STATE_MAXIMIZED: u32 = 0x4;
const STATE_FULLSCREEN: u32 = 0x8;
const STATE_DEMANDS_ATTENTION: u32 = 0x80;
const STATE_SKIP_TASKBAR: u32 = 0x1000;

struct TrackedWindow {
    pid: Option<u32>,
    handle: OrgKdePlasmaWindow,
    app_id: String,
    title: String,
    state: u32,
    geometry: Option<(i32, i32, i32, i32)>,
    order: u64,
    last_min_geom: Option<(u32, u32, u32, u32)>,
}

impl TrackedWindow {
    fn is_active(&self) -> bool {
        (self.state & STATE_ACTIVE) != 0
    }

    fn is_minimized(&self) -> bool {
        (self.state & STATE_MINIMIZED) != 0
    }

    fn is_maximized(&self) -> bool {
        (self.state & STATE_MAXIMIZED) != 0
    }

    fn is_fullscreen(&self) -> bool {
        (self.state & STATE_FULLSCREEN) != 0
    }

    fn demands_attention(&self) -> bool {
        (self.state & STATE_DEMANDS_ATTENTION) != 0
    }

    fn skip_taskbar(&self) -> bool {
        (self.state & STATE_SKIP_TASKBAR) != 0
    }
}

pub struct WindowTracker {
    _manager: Option<OrgKdePlasmaWindowManagement>,
    windows: HashMap<ObjectId, TrackedWindow>,
    next_order: u64,
}

impl WindowTracker {
    pub fn bind<D>(globals: &GlobalList, qh: &QueueHandle<D>) -> Self
    where
        D: Dispatch<OrgKdePlasmaWindowManagement, ()> + 'static,
    {
        let max_ver = OrgKdePlasmaWindowManagement::interface().version;
        let manager = match globals.bind::<OrgKdePlasmaWindowManagement, _, _>(qh, 1..=max_ver, ()) {
            Ok(mgr) => {
                tracing::info!(
                    version = mgr.version(),
                    "Protocolo org_kde_plasma_window_management activo"
                );
                Some(mgr)
            }
            Err(_) => {
                tracing::warn!(
                    "org_kde_plasma_window_management no autorizado o no disponible; los puntos indicadores y el ocultamiento inteligente requieren lanzar neonpear-dock mediante su archivo .desktop autorizado"
                );
                None
            }
        };

        Self {
            _manager: manager,
            windows: HashMap::new(),
            next_order: 0,
        }
    }

    /// Procesa eventos del gestor de ventanas de KWin y solicita el handle de cada ventana nueva.
    pub fn handle_manager_event<D>(
        &mut self,
        manager: &OrgKdePlasmaWindowManagement,
        event: ManagerEvent,
        qh: &QueueHandle<D>,
    ) where
        D: Dispatch<OrgKdePlasmaWindow, ()> + 'static,
    {
        match event {
            ManagerEvent::WindowWithUuid { uuid, .. } => {
                let win = manager.get_window_by_uuid(uuid, qh, ());
                self.register_window(win);
            }
            ManagerEvent::Window { id } if manager.version() < 13 => {
                let win = manager.get_window(id, qh, ());
                self.register_window(win);
            }
            _ => {}
        }
    }

    fn register_window(&mut self, handle: OrgKdePlasmaWindow) {
        let id = handle.id();
        let order = self.next_order;
        self.next_order += 1;

        self.windows.insert(
            id,
            TrackedWindow {
                pid: None,
                handle,
                app_id: String::new(),
                title: String::new(),
                state: 0,
                geometry: None,
                order,
                last_min_geom: None,
            },
        );
    }

    /// Actualiza metadatos y geometría de una ventana; devuelve `true` si cambió algún estado relevante.
    pub fn handle_window_event(
        &mut self,
        window: &OrgKdePlasmaWindow,
        event: WindowEvent,
    ) -> bool {
        let id = window.id();
        match event {
            WindowEvent::AppIdChanged { app_id } => {
                if let Some(entry) = self.windows.get_mut(&id) {
                    if entry.app_id != app_id {
                        entry.app_id = app_id;
                        return true;
                    }
                }
                false
            }
            WindowEvent::PidChanged { pid } => {
                if let Some(entry) = self.windows.get_mut(&id) {
                    entry.pid = (pid > 0).then_some(pid);
                }
                false
            }
            WindowEvent::TitleChanged { title } => {
                if let Some(entry) = self.windows.get_mut(&id) {
                    entry.title = title;
                }
                false
            }
            WindowEvent::StateChanged { flags } => {
                if let Some(entry) = self.windows.get_mut(&id) {
                    if entry.state != flags {
                        entry.state = flags;
                        return true;
                    }
                }
                false
            }
            WindowEvent::Geometry {
                x,
                y,
                width,
                height,
            } => {
                if let Some(entry) = self.windows.get_mut(&id) {
                    let next = Some((x, y, width as i32, height as i32));
                    if entry.geometry != next {
                        entry.geometry = next;
                        return true;
                    }
                }
                false
            }
            WindowEvent::Unmapped => {
                if let Some(removed) = self.windows.remove(&id) {
                    if removed.handle.version() >= 4 {
                        removed.handle.destroy();
                    }
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// Determina si alguna ventana visible (o solo la activa si `active_only = true`)
    /// invade el rectángulo `dock_rect` `(x, y, w, h)` en coordenadas globales de pantalla.
    pub fn is_dock_obstructed(
        &self,
        dock_rect: Option<(i32, i32, i32, i32)>,
        active_only: bool,
        own_namespace: &str,
    ) -> bool {
        let own_norm = normalize_id(own_namespace);

        for win in self.windows.values() {
            if win.skip_taskbar() || win.is_minimized() {
                continue;
            }
            if !own_norm.is_empty() && normalize_id(&win.app_id) == own_norm {
                continue;
            }
            if active_only && !win.is_active() {
                continue;
            }

            if win.is_fullscreen() {
                return true;
            }

            match (dock_rect, win.geometry) {
                (Some((dx, dy, dw, dh)), Some((wx, wy, ww, wh))) if ww > 0 && wh > 0 => {
                    let intersects =
                        wx < dx + dw && wx + ww > dx && wy < dy + dh && wy + wh > dy;
                    if intersects {
                        return true;
                    }
                }
                _ => {
                    // Respaldo si aún no conocemos la geometría exacta del monitor o de la ventana
                    if win.is_maximized() {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Devuelve `(app_id, title)` de todas las ventanas visibles en el orden en que se abrieron.
    pub fn open_window_descriptors(&self) -> Vec<(String, String)> {
        let mut list: Vec<&TrackedWindow> = self
            .windows
            .values()
            .filter(|w| !w.skip_taskbar() && !w.app_id.trim().is_empty())
            .collect();
        list.sort_by_key(|w| w.order);
        list.into_iter()
            .map(|w| (w.app_id.clone(), w.title.clone()))
            .collect()
    }

    /// Devuelve un vector booleano indicando qué entradas del dock tienen ventanas abiertas.
    pub fn running_flags_for(&self, entries: &[DockEntry]) -> Vec<bool> {
        entries
            .iter()
            .map(|entry| {
                self.windows
                    .values()
                    .any(|w| !w.skip_taskbar() && entry.matches_window_app_id(&w.app_id))
            })
            .collect()
    }


    /// Informa a KWin la posición exacta de cada ícono en el dock para que el efecto Genie
    /// minimice las ventanas hacia su ícono incluso desde el botón de la barra de título.
    pub fn sync_minimized_geometries(
        &mut self,
        surface: &WlSurface,
        entries: &[DockEntry],
        rects: &[(u32, u32, u32, u32)],
    ) {
        for win in self.windows.values_mut() {
            if win.skip_taskbar() {
                continue;
            }
            if let Some((_, &(x, y, w, h))) = entries
                .iter()
                .zip(rects.iter())
                .find(|(entry, _)| entry.matches_window_app_id(&win.app_id))
            {
                let geom = (x, y, w.max(1), h.max(1));
                if win.last_min_geom != Some(geom) {
                    win.handle.set_minimized_geometry(surface, geom.0, geom.1, geom.2, geom.3);
                    win.last_min_geom = Some(geom);
                }
            }
        }
    }

    /// Devuelve `true` para las entradas que tienen ventanas abiertas y todas están minimizadas.
    pub fn minimized_flags_for(&self, entries: &[DockEntry]) -> Vec<bool> {
        entries
            .iter()
            .map(|entry| {
                let mut has_any = false;
                for w in self.windows.values() {
                    if !w.skip_taskbar() && entry.matches_window_app_id(&w.app_id) {
                        has_any = true;
                        if !w.is_minimized() {
                            return false;
                        }
                    }
                }
                has_any
            })
            .collect()
    }

    /// Devuelve `true` para las entradas con al menos una ventana solicitando atención.
    pub fn attention_flags_for(&self, entries: &[DockEntry]) -> Vec<bool> {
        entries
            .iter()
            .map(|entry| {
                self.windows.values().any(|w| {
                    !w.skip_taskbar()
                        && w.demands_attention()
                        && entry.matches_window_app_id(&w.app_id)
                })
            })
            .collect()
    }

    /// Si la app tiene ventanas abiertas, enfoca/restaura una ventana inactiva o minimiza la activa.
    /// Devuelve `true` si manejó una ventana existente, o `false` si debe lanzarse una instancia nueva.
    pub fn toggle_or_activate(&self, app: &DesktopApp) -> bool {
        let matching: Vec<&TrackedWindow> = self
            .windows
            .values()
            .filter(|w| !w.skip_taskbar() && app.matches_app_id(&w.app_id))
            .collect();

        if matching.is_empty() {
            return false;
        }

        // Prioridad 1: enfocar alguna ventana visible que no esté activa
        if let Some(win) = matching.iter().find(|w| !w.is_active() && !w.is_minimized()) {
            win.handle.set_state(STATE_ACTIVE | STATE_MINIMIZED, STATE_ACTIVE);
            tracing::debug!(app = %app.name, title = %win.title, "Ventana enfocada");
            return true;
        }

        // Prioridad 2: restaurar y enfocar una ventana minimizada
        if let Some(win) = matching.iter().find(|w| w.is_minimized()) {
            win.handle.set_state(STATE_ACTIVE | STATE_MINIMIZED, STATE_ACTIVE);
            tracing::debug!(app = %app.name, title = %win.title, "Ventana restaurada");
            return true;
        }

        // Prioridad 3: si ya está activa al frente, minimizarla
        if let Some(win) = matching.first() {
            win.handle.set_state(STATE_MINIMIZED, STATE_MINIMIZED);
            tracing::debug!(app = %app.name, title = %win.title, "Ventana minimizada");
            return true;
        }

        false
    }

    /// Cicla entre todas las ventanas abiertas de `app` (adelante o atrás) y enfoca la siguiente.
    pub fn cycle_windows(&self, app: &DesktopApp, forward: bool) -> bool {
        let mut matching: Vec<&TrackedWindow> = self
            .windows
            .values()
            .filter(|w| !w.skip_taskbar() && app.matches_app_id(&w.app_id))
            .collect();

        if matching.is_empty() {
            return false;
        }
        matching.sort_by_key(|w| w.order);

        let next_idx = if matching.len() == 1 {
            0
        } else if let Some(cur) = matching.iter().position(|w| w.is_active()) {
            if forward {
                (cur + 1) % matching.len()
            } else {
                (cur + matching.len() - 1) % matching.len()
            }
        } else {
            0
        };

        let target = matching[next_idx];
        target
            .handle
            .set_state(STATE_ACTIVE | STATE_MINIMIZED, STATE_ACTIVE);
        tracing::debug!(app = %app.name, title = %target.title, forward, "Ventana ciclada por scroll");
        true
    }

    /// Solicita a KWin el cierre limpio de todas las ventanas pertenecientes a `app`.
    /// Devuelve las ventanas abiertas visibles de `app`, ordenadas por creación.
    pub fn open_windows_for_app(&self, app: &DesktopApp) -> Vec<dock_core::menu::OpenWindowItem> {
        let mut wins: Vec<&TrackedWindow> = self
            .windows
            .values()
            .filter(|w| !w.skip_taskbar() && app.matches_app_id(&w.app_id))
            .collect();
        wins.sort_by_key(|w| w.order);
        wins.into_iter()
            .map(|w| dock_core::menu::OpenWindowItem {
                id: w.order,
                title: w.title.clone(),
                active: w.is_active(),
            })
            .collect()
    }

    /// Activa (y desminimiza si hace falta) una ventana específica por su `order`.
    pub fn activate_window_by_order(&self, order: u64) {
        if let Some(win) = self.windows.values().find(|w| w.order == order) {
            win.handle
                .set_state(STATE_ACTIVE | STATE_MINIMIZED, STATE_ACTIVE);
        }
    }

    /// Envía SIGKILL a todos los procesos asociados a las ventanas abiertas de `app`.
    pub fn force_quit_app(&self, app: &DesktopApp) {
        let mut pids: Vec<u32> = self
            .windows
            .values()
            .filter(|w| !w.skip_taskbar() && app.matches_app_id(&w.app_id))
            .filter_map(|w| w.pid)
            .collect();
        pids.sort_unstable();
        pids.dedup();

        if pids.is_empty() {
            tracing::warn!(app = %app.name, "Sin PID registrado para forzar salida; intentando cierre normal");
            self.close_app(app);
            return;
        }

        for pid in pids {
            match std::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .status()
            {
                Ok(status) if status.success() => {
                    tracing::info!(app = %app.name, pid, "Proceso terminado con SIGKILL");
                }
                Ok(status) => {
                    tracing::warn!(app = %app.name, pid, ?status, "kill -9 devolvió estado no exitoso");
                }
                Err(err) => {
                    tracing::error!(app = %app.name, pid, "Error ejecutando kill -9: {err}");
                }
            }
        }
    }

    pub fn close_app(&self, app: &DesktopApp) {
        let mut closed = 0usize;
        for win in self.windows.values() {
            if !win.skip_taskbar() && app.matches_app_id(&win.app_id) {
                win.handle.close();
                closed += 1;
            }
        }
        tracing::info!(app = %app.name, count = closed, "Solicitud de cierre enviada");
    }
}
