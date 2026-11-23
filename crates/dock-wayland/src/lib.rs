use smithay_client_toolkit::data_device_manager::{
    data_device::{DataDevice, DataDeviceHandler},
    data_offer::{DataOfferHandler, DragOffer},
    data_source::DataSourceHandler,
    DataDeviceManagerState, WritePipe,
};
use wayland_client::protocol::{
    wl_data_device, wl_data_device_manager::DndAction, wl_data_source,
};
mod blur;
mod windows;

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::time::{Duration, Instant};

use blur::BlurController;
use calloop::channel::{Channel, Event as ChannelEvent};
use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use dock_core::desktop::{
    empty_trash, normalize_id, open_trash, AppCatalog, DesktopApp, DockEntry, DockEntryKind,
    DockSection,
};
use dock_core::menu::{build_menu_items, ContextMenuAction};
use dock_core::physics::{DockPhysics, FrameLayout};
use dock_core::{DockConfig, HideMode, ThemeTransition};
use dock_render::DockRenderer;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_compositor, delegate_data_device, delegate_layer, delegate_output, delegate_keyboard, delegate_pointer, delegate_registry,
    delegate_seat,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
};
use wayland_client::{
    delegate_noop,
    globals::registry_queue_init,
    protocol::{wl_output, wl_keyboard, wl_pointer, wl_seat, wl_surface},
    Connection, Dispatch, Proxy, QueueHandle,
};
use wayland_protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1,
    ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
};
use wayland_protocols_plasma::blur::client::{
    org_kde_kwin_blur::OrgKdeKwinBlur,
    org_kde_kwin_blur_manager::OrgKdeKwinBlurManager,
};
use wayland_protocols_plasma::plasma_window_management::client::{
    org_kde_plasma_window::{self, OrgKdePlasmaWindow},
    org_kde_plasma_window_management::{self, OrgKdePlasmaWindowManagement},
};
use windows::WindowTracker;

const MENU_LEAVE_GRACE_MS: u64 = 90;
const DODGE_HIDE_DELAY_MS: u64 = 140;
const SCROLL_CYCLE_COOLDOWN_MS: u64 = 180;

#[derive(Debug, Clone)]
pub enum SystemEvent {
    SetDark(bool),
    TrashChanged(bool),
    BadgeUpdate { app_id: String, count: u64 },
    ConfigReloaded(DockConfig),
    PinDesktopFiles(Vec<String>),
}

struct DragState {
    pinned_idx: usize,
    start_pos: (f32, f32),
    active: bool,
    outside: bool,
    reordered: bool,
}

struct DockApp {
    // `renderer` debe declararse primero para liberarse antes que los recursos de Wayland.
    renderer: Option<DockRenderer>,
    blur: BlurController,
    window_tracker: WindowTracker,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    modifiers: Modifiers,
    drag_state: Option<DragState>,
    data_device_manager: Option<DataDeviceManagerState>,
    data_device: Option<DataDevice>,
    event_tx: calloop::channel::Sender<SystemEvent>,
    active_output: Option<wl_output::WlOutput>,
    layer_surface: LayerSurface,
    conn: Connection,
    qh: QueueHandle<Self>,

    registry_state: RegistryState,
    output_state: OutputState,
    compositor_state: CompositorState,
    seat_state: SeatState,

    config: DockConfig,
    catalog: AppCatalog,
    pinned_apps: Vec<DesktopApp>,
    recent_apps: Vec<DesktopApp>,
    entries: Vec<DockEntry>,
    trash_full: bool,
    badges: HashMap<String, u64>,

    physics: DockPhysics,
    logical_width: u32,
    logical_height: u32,
    requested_width: u32,
    requested_height: u32,
    scale_factor: u32,
    theme: ThemeTransition,
    hide_deadline: Option<Instant>,
    menu_leave_deadline: Option<Instant>,
    menu_pressed_item: Option<usize>,
    last_scroll: Option<Instant>,
    last_input_rects: Vec<(i32, i32, i32, i32)>,
    last_tick: Instant,
    frame_pending: bool,
    dirty: bool,
    animating: bool,
    exit: bool,
}

impl DockApp {
    /// Construye la lista ordenada de las 3 zonas: Fijadas | Abiertas/Recientes | Papelera.
    fn build_entries(&mut self) -> Vec<DockEntry> {
        let mut out = Vec::new();
        let own_namespace = normalize_id(&self.config.namespace);

        // Botón opcional tipo Launchpad al inicio del dock
        if self.config.show_launchpad {
            out.push(self.catalog.launchpad_entry());
        }

        // Zona 1: Aplicaciones fijadas (y espaciadores)
        if !self.config.active_apps_only {
            for app in &self.pinned_apps {
                if dock_core::desktop::is_spacer_id(&app.id) {
                    out.push(DockEntry::spacer(app.id.clone()));
                } else {
                    out.push(DockEntry::from_app(app.clone(), DockSection::Pinned));
                }
            }
        }

        // Zona 2: Aplicaciones abiertas no fijadas (en orden de apertura)
        let mut open_dynamic: Vec<DesktopApp> = Vec::new();
        let mut seen_dynamic: HashSet<String> = HashSet::new();

        for (app_id, title) in self.window_tracker.open_window_descriptors() {
            let norm_win = normalize_id(&app_id);
            if norm_win.is_empty() || norm_win == own_namespace {
                continue;
            }
            if !self.config.active_apps_only && self.pinned_apps.iter().any(|p| p.matches_app_id(&app_id)) {
                continue;
            }

            let resolved = self.catalog.resolve_window_app(&app_id, &title);
            let resolved_key = normalize_id(&resolved.id);
            if seen_dynamic.insert(resolved_key) {
                open_dynamic.push(resolved);
            }
        }

        // Actualiza el historial de recientes con las apps dinámicas que tienen comando Exec
        if self.config.show_recent_apps && !self.config.active_apps_only && self.config.max_recent_apps > 0 {
            for app in open_dynamic.iter().rev() {
                if app.exec.is_empty() {
                    continue;
                }
                let k = normalize_id(&app.id);
                self.recent_apps.retain(|r| normalize_id(&r.id) != k);
                self.recent_apps.insert(0, app.clone());
            }
            self.recent_apps.truncate(self.config.max_recent_apps + 8);
        }

        for app in &open_dynamic {
            out.push(DockEntry::from_app(app.clone(), DockSection::Dynamic));
        }

        // Completa los espacios recientes con apps cerradas recientemente
        if self.config.show_recent_apps && self.config.max_recent_apps > 0 {
            let mut shown_dynamic = open_dynamic.len();
            for recent in &self.recent_apps {
                if shown_dynamic >= self.config.max_recent_apps {
                    break;
                }
                let rk = normalize_id(&recent.id);
                if seen_dynamic.contains(&rk) {
                    continue;
                }
                if self.pinned_apps.iter().any(|p| normalize_id(&p.id) == rk) {
                    continue;
                }
                seen_dynamic.insert(rk);
                out.push(DockEntry::from_app(recent.clone(), DockSection::Dynamic));
                shown_dynamic += 1;
            }
        }

        // Zona 3: Utilidades (Papelera)
        if self.config.show_trash {
            out.push(self.catalog.trash_entry(self.trash_full));
        }

        out
    }

    /// Calcula el rectángulo global `(x, y, ancho, alto)` de la píldora en reposo sobre el monitor activo.
    fn dock_global_rect(&self) -> Option<(i32, i32, i32, i32)> {
        let output = self
            .active_output
            .clone()
            .or_else(|| self.output_state.outputs().next())?;
        let info = self.output_state.info(&output)?;

        let (ox, oy) = info.logical_position.unwrap_or(info.location);
        let (ow, oh) = info.logical_size.or_else(|| {
            info.modes
                .iter()
                .find(|m| m.current)
                .map(|m| m.dimensions)
        })?;

        let (pill_w, pill_h) = self.physics.resting_pill_size(&self.config);
        let dw = pill_w.ceil() as i32;
        let dh = pill_h.ceil() as i32 + self.config.bottom_margin.max(0);
        let dx = ox + (ow - dw) / 2;
        let dy = match self.config.position {
            dock_core::DockPosition::Top => oy,
            dock_core::DockPosition::Bottom => oy + oh - dh,
        };
        Some((dx, dy, dw, dh))
    }

    fn should_hide_now(&self) -> bool {
        match self.config.effective_hide_mode() {
            HideMode::None => false,
            HideMode::AutoHide => true,
            HideMode::DodgeWindows => self.window_tracker.is_dock_obstructed(
                self.dock_global_rect(),
                false,
                &self.config.namespace,
            ),
            HideMode::DodgeActive => self.window_tracker.is_dock_obstructed(
                self.dock_global_rect(),
                true,
                &self.config.namespace,
            ),
        }
    }

    /// Evalúa si el dock debe ocultarse o mostrarse según el modo activo y las ventanas abiertas.
    /// Devuelve `true` si cambió inmediatamente el estado de visibilidad.
    fn evaluate_hide_state(&mut self, delay_ms: u64) -> bool {
        if !self.config.hiding_enabled() {
            self.hide_deadline = None;
            return self.physics.set_hidden(false);
        }

        if self.physics.is_cursor_inside() || self.physics.has_menu() {
            self.hide_deadline = None;
            return self.physics.set_hidden(false);
        }

        if self.should_hide_now() {
            if !self.physics.is_hidden() && self.hide_deadline.is_none() {
                self.hide_deadline = Some(Instant::now() + Duration::from_millis(delay_ms));
            }
            false
        } else {
            self.hide_deadline = None;
            self.physics.set_hidden(false)
        }
    }

    fn sync_canvas_size(&mut self) -> bool {
        let (target_w, target_h) = self.physics.canvas_size(&self.config);
        if target_w != self.requested_width || target_h != self.requested_height {
            self.requested_width = target_w;
            self.requested_height = target_h;
            self.layer_surface.set_size(target_w, target_h);
            self.layer_surface.commit();
            return true;
        }
        false
    }

    fn sync_dock_state(&mut self) {
        let new_entries = self.build_entries();
        let entries_changed = self.entries.len() != new_entries.len()
            || self
                .entries
                .iter()
                .zip(new_entries.iter())
                .any(|(a, b)| a.key != b.key || a.section != b.section || a.icon_path != b.icon_path);

        self.entries = new_entries;
        let phys_changed = self.physics.sync_entries(&self.entries, &self.config);
        let flags = self.window_tracker.running_flags_for(&self.entries);
        let running_changed = self.physics.set_running_states(&flags);
        let min_flags = self.window_tracker.minimized_flags_for(&self.entries);
        let min_changed = self.physics.set_minimized_states(&min_flags);
        let attn_flags = self.window_tracker.attention_flags_for(&self.entries);
        let attn_changed = self.physics.set_attention_states(&attn_flags, &self.config);
        let badge_counts: Vec<u64> = self
            .entries
            .iter()
            .map(|entry| match &entry.kind {
                DockEntryKind::App(app) => self
                    .badges
                    .iter()
                    .find(|(k, _)| app.matches_app_id(k))
                    .map(|(_, &c)| c)
                    .unwrap_or(0),
                DockEntryKind::Trash { .. } | DockEntryKind::Spacer { .. } | DockEntryKind::Launchpad => 0,
            })
            .collect();
        let badges_changed = self.physics.set_badge_counts(&badge_counts);
        let hide_changed = self.evaluate_hide_state(DODGE_HIDE_DELAY_MS);

        let icon_px = self.max_icon_phys_px();
        if let Some(renderer) = &mut self.renderer {
            renderer.sync_icons(&self.entries, icon_px);
        }

        if self.sync_canvas_size() {
            return;
        }

        if entries_changed || phys_changed || running_changed || min_changed || attn_changed || badges_changed || hide_changed {
            self.request_redraw();
        }
    }

    fn execute_menu_action(&mut self, action: ContextMenuAction) {
        match action {
            ContextMenuAction::LaunchApp(app) => {
                if !app.exec.is_empty() {
                    let target = normalize_id(&app.id);
                    if let Some(idx) = self.entries.iter().position(|e| {
                        matches!(&e.kind, DockEntryKind::App(a) if normalize_id(&a.id) == target)
                    }) {
                        self.physics.trigger_bounce(idx, &self.config);
                    }
                    app.launch();
                }
            }
            ContextMenuAction::RunDesktopAction {
                app_id,
                label,
                exec,
            } => {
                if !exec.is_empty() {
                    let target = normalize_id(&app_id);
                    if let Some(idx) = self.entries.iter().position(|e| {
                        matches!(&e.kind, DockEntryKind::App(a) if normalize_id(&a.id) == target)
                    }) {
                        self.physics.trigger_bounce(idx, &self.config);
                    }
                    DesktopApp::launch_exec(&label, &exec);
                }
            }
            ContextMenuAction::PinApp(app) => {
                let target = normalize_id(&app.id);
                if !self.pinned_apps.iter().any(|p| normalize_id(&p.id) == target) {
                    self.pinned_apps.push(app.clone());
                }
                if !self
                    .config
                    .pinned_apps
                    .iter()
                    .any(|id| normalize_id(id) == target)
                {
                    self.config.pinned_apps.push(app.id.clone());
                    self.config.save();
                }
                self.recent_apps.retain(|r| normalize_id(&r.id) != target);
                self.sync_dock_state();
            }
            ContextMenuAction::UnpinApp(app_id) => {
                let target = normalize_id(&app_id);
                let removed_app = self
                    .pinned_apps
                    .iter()
                    .find(|p| normalize_id(&p.id) == target)
                    .cloned();
                self.pinned_apps.retain(|p| normalize_id(&p.id) != target);
                self.config.pinned_apps.retain(|cfg_id| {
                    if normalize_id(cfg_id) == target {
                        return false;
                    }
                    if let Some(app) = &removed_app {
                        if app.matches_app_id(cfg_id) {
                            return false;
                        }
                    }
                    true
                });
                self.config.save();
                self.sync_dock_state();
            }
            ContextMenuAction::RemoveRecent(app_id) => {
                let target = normalize_id(&app_id);
                self.recent_apps.retain(|r| normalize_id(&r.id) != target);
                self.sync_dock_state();
            }
            ContextMenuAction::CloseApp(app) => {
                self.window_tracker.close_app(&app);
            }
            ContextMenuAction::ForceQuitApp(app) => {
                self.window_tracker.force_quit_app(&app);
            }
            ContextMenuAction::ActivateWindow(order) => {
                self.window_tracker.activate_window_by_order(order);
            }
            ContextMenuAction::ToggleAutoHide => {
                if self.config.hide_mode == dock_core::HideMode::None {
                    self.config.hide_mode = dock_core::HideMode::DodgeWindows;
                    self.config.auto_hide = true;
                } else {
                    self.config.hide_mode = dock_core::HideMode::None;
                    self.config.auto_hide = false;
                }
                self.config.save();
                self.sync_dock_state();
                self.sync_canvas_size();
                self.request_redraw();
            }
            ContextMenuAction::ToggleMagnification => {
                self.config.magnification_enabled = !self.config.magnification_enabled;
                self.config.save();
                self.sync_canvas_size();
                self.request_redraw();
            }
            ContextMenuAction::OpenLaunchpad => {
                dock_core::desktop::open_launchpad();
            }
            ContextMenuAction::ToggleLaunchpad => {
                self.config.show_launchpad = !self.config.show_launchpad;
                self.config.save();
                self.sync_dock_state();
                self.sync_canvas_size();
                self.request_redraw();
            }
            ContextMenuAction::ToggleActiveAppsOnly => {
                self.config.active_apps_only = !self.config.active_apps_only;
                self.config.save();
                self.sync_dock_state();
                self.sync_canvas_size();
                self.request_redraw();
            }
            ContextMenuAction::ToggleRecentApps => {
                self.config.show_recent_apps = !self.config.show_recent_apps;
                self.config.save();
                self.sync_dock_state();
                self.sync_canvas_size();
                self.request_redraw();
            }
            ContextMenuAction::AddSpacer => {
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                let id = format!("{}{}", dock_core::desktop::SPACER_PREFIX, ts);
                self.pinned_apps.push(dock_core::desktop::make_spacer_app(id.clone()));
                self.config.pinned_apps.push(id);
                self.config.save();
                self.sync_dock_state();
                self.sync_canvas_size();
                self.request_redraw();
            }
            ContextMenuAction::RemoveSpacer(id) => {
                self.pinned_apps.retain(|a| a.id != id);
                self.config.pinned_apps.retain(|p| p != &id);
                self.config.save();
                self.sync_dock_state();
                self.sync_canvas_size();
                self.request_redraw();
            }
            ContextMenuAction::OpenSettings => {
                self.physics.close_menu();
                self.physics.set_cursor(None);
                self.sync_canvas_size();
                self.request_redraw();
                let _ = self.conn.flush();

                std::thread::spawn(|| {
                    std::thread::sleep(Duration::from_millis(60));
                    let sibling = std::env::current_exe()
                        .ok()
                        .and_then(|p| p.parent().map(|d| d.join("neonpear-dock-settings")))
                        .filter(|p| p.exists());
                    let cmd = sibling
                        .as_deref()
                        .map(|p| p.as_os_str())
                        .unwrap_or_else(|| std::ffi::OsStr::new("neonpear-dock-settings"));
                    if let Err(err) = std::process::Command::new(cmd)
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()
                    {
                        tracing::error!(?cmd, "No se pudo lanzar neonpear-dock-settings: {err}");
                    }
                });
            }
            ContextMenuAction::OpenTrash => {
                open_trash();
            }
            ContextMenuAction::EmptyTrash => {
                empty_trash();
            }
        }
    }

    fn next_timeout(&self) -> Option<Duration> {
        let now = Instant::now();
        let mut min_dur: Option<Duration> = None;
        for deadline in [self.hide_deadline, self.menu_leave_deadline]
            .into_iter()
            .flatten()
        {
            let dur = deadline.saturating_duration_since(now);
            min_dur = Some(match min_dur {
                Some(cur) => cur.min(dur),
                None => dur,
            });
        }
        min_dur
    }

    fn check_deadlines(&mut self) {
        let now = Instant::now();

        if let Some(deadline) = self.menu_leave_deadline {
            if now >= deadline {
                self.menu_leave_deadline = None;
                if !self.physics.is_cursor_inside() && self.physics.close_menu() {
                    if self.config.hiding_enabled() && self.should_hide_now() {
                        self.hide_deadline = Some(
                            now + Duration::from_millis(self.config.auto_hide_delay_ms),
                        );
                    }
                    self.request_redraw();
                }
            }
        }

        if let Some(deadline) = self.hide_deadline {
            if now >= deadline {
                self.hide_deadline = None;
                if self.config.hiding_enabled()
                    && !self.physics.is_cursor_inside()
                    && !self.physics.has_menu()
                    && self.should_hide_now()
                    && self.physics.set_hidden(true)
                {
                    self.request_redraw();
                }
            }
        }
    }

    fn update_input_region(&mut self, layout: &FrameLayout) {
        let ch = layout.canvas_height as i32;
        let cw = layout.canvas_width as i32;
        let mut rects = Vec::new();
        let hiding = self.config.hiding_enabled();

        let base_rect =
            if hiding && self.physics.is_hidden() && layout.hide_progress > 0.95 {
                let strip_h = 4;
                let x0 = (layout.pill.x - 16.0).floor().max(0.0) as i32;
                let x1 = (layout.pill.x + layout.pill.width + 16.0)
                    .ceil()
                    .min(cw as f32) as i32;
                let strip_y = if self.config.position == dock_core::DockPosition::Top {
                    0
                } else {
                    (ch - strip_h).max(0)
                };
                (x0, strip_y, (x1 - x0).max(48), strip_h)
            } else {
                let mut min_x = layout.pill.x;
                let mut min_y = layout.pill.y;
                let mut max_x = layout.pill.x + layout.pill.width;
                let mut max_y = layout.pill.y + layout.pill.height;

                for icon in &layout.icons {
                    min_x = min_x.min(icon.x);
                    min_y = min_y.min(icon.y);
                    max_x = max_x.max(icon.x + icon.size);
                    max_y = max_y.max(icon.y + icon.size);
                }

                if hiding {
                    if self.config.position == dock_core::DockPosition::Top {
                        min_y = 0.0;
                        if !self.physics.is_hidden() {
                            let resting_bottom =
                                layout.pill.height + self.config.bottom_margin.max(0) as f32;
                            max_y = max_y.max(resting_bottom);
                        } else {
                            max_y = max_y.max(4.0);
                        }
                    } else {
                        max_y = ch as f32;
                        if !self.physics.is_hidden() {
                            let resting_top = ch as f32
                                - layout.pill.height
                                - self.config.bottom_margin.max(0) as f32;
                            min_y = min_y.min(resting_top);
                        } else {
                            min_y = min_y.min((ch - 4) as f32);
                        }
                    }
                }

                let x0 = min_x.floor().clamp(0.0, cw as f32) as i32;
                let y0 = min_y.floor().clamp(0.0, ch as f32) as i32;
                let x1 = max_x.ceil().clamp(0.0, cw as f32) as i32;
                let y1 = max_y.ceil().clamp(0.0, ch as f32) as i32;
                (x0, y0, x1 - x0, y1 - y0)
            };

        if base_rect.2 > 0 && base_rect.3 > 0 {
            rects.push(base_rect);
        }

        // Cuando hay menú abierto, añadimos su caja y el puente continuo hasta la píldora
        if self.physics.is_menu_open() {
            if let Some(menu) = &layout.menu {
                let mx0 = (menu.x - 4.0).floor().clamp(0.0, cw as f32) as i32;
                let mx1 = (menu.x + menu.width + 4.0).ceil().clamp(0.0, cw as f32) as i32;
                let (my0, my1) = if menu.point_up {
                    let bridge_top = (layout.pill.y + layout.pill.height - 4.0).min(menu.y - 4.0);
                    (
                        bridge_top.floor().clamp(0.0, ch as f32) as i32,
                        (menu.y + menu.height + 4.0).ceil().clamp(0.0, ch as f32) as i32,
                    )
                } else {
                    let bridge_bottom = (layout.pill.y + 4.0).max(menu.anchor_y + 4.0);
                    (
                        (menu.y - 4.0).floor().clamp(0.0, ch as f32) as i32,
                        bridge_bottom.ceil().clamp(0.0, ch as f32) as i32,
                    )
                };
                if mx1 > mx0 && my1 > my0 {
                    rects.push((mx0, my0, mx1 - mx0, my1 - my0));
                }
            }
        }

        if rects.is_empty() || self.last_input_rects == rects {
            return;
        }
        self.last_input_rects = rects.clone();

        if let Ok(region) = Region::new(&self.compositor_state) {
            for (x, y, w, h) in rects {
                region.add(x, y, w, h);
            }
            self.layer_surface
                .wl_surface()
                .set_input_region(Some(region.wl_region()));
        }
    }

    fn max_icon_phys_px(&self) -> u32 {
        let base = if self.config.magnification_enabled {
            self.config.max_icon_size.max(self.config.icon_size)
        } else {
            self.config.icon_size
        };
        (base * self.scale_factor as f32).ceil() as u32
    }

    fn draw(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f32().min(0.05);
        self.last_tick = now;

        let theme_anim = self.theme.step(dt);
        let phys_anim = self.physics.step(dt, &self.config);

        if self.sync_canvas_size() {
            return;
        }
        if self.logical_width != self.requested_width
            || self.logical_height != self.requested_height
        {
            return;
        }

        let layout = self.physics.compute_frame(&self.config);

        if layout.hide_progress < 0.05 && !layout.icons.is_empty() {
            let icon_rects: Vec<(u32, u32, u32, u32)> = layout
                .icons
                .iter()
                .map(|slot| {
                    let s = slot.size.round().max(1.0) as u32;
                    let x = (slot.x - slot.size * 0.5).round().max(0.0) as u32;
                    let y = (slot.y - slot.size * 0.5).round().max(0.0) as u32;
                    (x, y, s, s)
                })
                .collect();
            self.window_tracker.sync_minimized_geometries(
                self.layer_surface.wl_surface(),
                &self.entries,
                &icon_rects,
            );
        }

        self.blur.update_regions(
            &self.compositor_state,
            &layout.pill,
            self.config.corner_radius,
            layout.menu.as_ref(),
            layout.canvas_height,
        );
        self.update_input_region(&layout);

        self.animating = theme_anim || phys_anim;
        if self.renderer.is_some() && !self.frame_pending {
            let surf = self.layer_surface.wl_surface();
            surf.frame(&self.qh, surf.clone());
            self.frame_pending = true;
        }

        if let Some(renderer) = &mut self.renderer {
            let effective_dark = match self.config.glass_theme {
                dock_core::GlassTheme::Auto => self.theme.current,
                dock_core::GlassTheme::Dark => 1.0,
                dock_core::GlassTheme::Light => 0.0,
                dock_core::GlassTheme::Custom => self.theme.current,
            };
            let custom_tint = match self.config.glass_theme {
                dock_core::GlassTheme::Custom => [
                    self.config.custom_tint[0] as f32 / 255.0,
                    self.config.custom_tint[1] as f32 / 255.0,
                    self.config.custom_tint[2] as f32 / 255.0,
                    1.0,
                ],
                _ => [0.0, 0.0, 0.0, 0.0],
            };
            let glass_style = dock_render::GlassStyle {
                is_dark: effective_dark,
                opacity: self.config.glass_opacity,
                border_strength: self.config.border_intensity,
                custom_tint,
            };
            renderer.render(
                &layout,
                self.config.corner_radius,
                glass_style,
                self.blur.is_glass_active(),
            );
        }
    }

    fn request_redraw(&mut self) {
        if self.renderer.is_some() {
            if self.frame_pending {
                self.dirty = true;
            } else {
                self.dirty = false;
                self.last_tick = Instant::now();
                self.draw();
            }
        }
    }
}

impl CompositorHandler for DockApp {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        let factor = new_factor.max(1) as u32;
        if factor == self.scale_factor {
            return;
        }
        self.scale_factor = factor;
        surface.set_buffer_scale(factor as i32);

        let icon_px = self.max_icon_phys_px();
        if let Some(renderer) = &mut self.renderer {
            renderer.resize(
                self.logical_width * self.scale_factor,
                self.logical_height * self.scale_factor,
                self.scale_factor as f32,
            );
            renderer.sync_icons(&self.entries, icon_px);
            self.draw();
        }
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        self.frame_pending = false;
        if self.dirty || self.animating {
            self.dirty = false;
            self.draw();
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        self.active_output = Some(output.clone());
        if self.evaluate_hide_state(DODGE_HIDE_DELAY_MS) {
            self.request_redraw();
        }
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for DockApp {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        let matches_preferred = self
            .config
            .monitor
            .as_deref()
            .filter(|m| !m.trim().is_empty())
            .and_then(|preferred| {
                self.output_state
                    .info(&output)
                    .and_then(|info| info.name)
                    .map(|name| name.eq_ignore_ascii_case(preferred.trim()))
            })
            .unwrap_or(false);

        if matches_preferred || self.active_output.is_none() {
            self.active_output = Some(output);
        }
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        if self.evaluate_hide_state(DODGE_HIDE_DELAY_MS) {
            self.request_redraw();
        }
    }
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        if self.active_output.as_ref() == Some(&output) {
            self.active_output = None;
        }
    }
}

impl SeatHandler for DockApp {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        if self.data_device.is_none() {
            if let Some(ddm) = &self.data_device_manager {
                self.data_device = Some(ddm.get_data_device(qh, &seat));
            }
        }
    }

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if self.data_device.is_none() {
            if let Some(ddm) = &self.data_device_manager {
                self.data_device = Some(ddm.get_data_device(qh, &seat));
                tracing::info!("DataDevice Wayland inicializado para Drag & Drop");
            }
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            if let Ok(pointer) = self.seat_state.get_pointer(qh, &seat) {
                self.pointer = Some(pointer);
            }
        }
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            if let Ok(keyboard) = self.seat_state.get_keyboard(qh, &seat, None) {
                self.keyboard = Some(keyboard);
            }
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer {
            if let Some(pointer) = self.pointer.take() {
                pointer.release();
            }
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for DockApp {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        let mut needs_redraw = false;

        for event in events {
            if &event.surface != self.layer_surface.wl_surface() {
                continue;
            }
            match event.kind {
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                    self.hide_deadline = None;
                    self.menu_leave_deadline = None;
                    if self.config.hiding_enabled() {
                        self.physics.set_hidden(false);
                    }
                    let pos = (event.position.0 as f32, event.position.1 as f32);
                    self.physics.set_cursor(Some(pos));

                    let mut reorder_pair: Option<(usize, usize)> = None;
                    let mut cancel_press = false;
                    if let Some(drag) = self.drag_state.as_mut() {
                        drag.outside = false;
                        if !drag.active {
                            let dist = (pos.0 - drag.start_pos.0).hypot(pos.1 - drag.start_pos.1);
                            if dist >= 10.0 {
                                drag.active = true;
                                cancel_press = true;
                            }
                        }
                        if drag.active {
                            if let Some(target_idx) = self.physics.icon_at_cursor(&self.config) {
                                if target_idx < self.pinned_apps.len()
                                    && target_idx != drag.pinned_idx
                                {
                                    reorder_pair = Some((drag.pinned_idx, target_idx));
                                    drag.pinned_idx = target_idx;
                                    drag.reordered = true;
                                }
                            }
                        }
                    }
                    if cancel_press {
                        self.physics.set_pressed(false, &self.config);
                    }
                    if let Some((from, to)) = reorder_pair {
                        let app = self.pinned_apps.remove(from);
                        self.pinned_apps.insert(to, app);
                        self.config.pinned_apps = self
                            .pinned_apps
                            .iter()
                            .map(|a| a.id.clone())
                            .collect();
                        self.sync_dock_state();
                    }
                    needs_redraw = true;
                }
                PointerEventKind::Leave { .. } => {
                    if let Some(drag) = self.drag_state.as_mut() {
                        if drag.active {
                            drag.outside = true;
                        }
                    }
                    self.physics.set_cursor(None);
                    self.menu_pressed_item = None;
                    if self.physics.is_menu_open() {
                        self.menu_leave_deadline =
                            Some(Instant::now() + Duration::from_millis(MENU_LEAVE_GRACE_MS));
                    } else if self.config.hiding_enabled() && self.should_hide_now() {
                        self.hide_deadline = Some(
                            Instant::now() + Duration::from_millis(self.config.auto_hide_delay_ms),
                        );
                    }
                    needs_redraw = true;
                }
                PointerEventKind::Press { button, .. } if button == BTN_RIGHT => {
                    self.menu_pressed_item = None;
                    self.menu_leave_deadline = None;
                    if let Some(idx) = self.physics.icon_at_cursor(&self.config) {
                        if let Some(entry) = self.entries.get(idx) {
                            let is_running = self.physics.icon_is_running(idx);
                            let open_windows = match &entry.kind {
                                DockEntryKind::App(app) => {
                                    self.window_tracker.open_windows_for_app(app)
                                }
                                DockEntryKind::Trash { .. } | DockEntryKind::Spacer { .. } | DockEntryKind::Launchpad => Vec::new(),
                            };
                            let items = build_menu_items(entry, is_running, &open_windows);
                            if self.physics.open_menu(idx, items) {
                                self.hide_deadline = None;
                                if !self.sync_canvas_size() {
                                    needs_redraw = true;
                                }
                            }
                        }
                    } else {
                        let items = dock_core::menu::build_dock_quick_menu(&self.config);
                        if self.physics.open_dock_menu(event.position.0 as f32, items) {
                            self.hide_deadline = None;
                            if !self.sync_canvas_size() {
                                needs_redraw = true;
                            }
                        }
                    }
                }
                PointerEventKind::Press { button, .. } if button == BTN_LEFT => {
                    if self.physics.is_menu_open() {
                        if let Some(item_idx) = self.physics.menu_hit_test(&self.config) {
                            self.menu_pressed_item = Some(item_idx);
                        } else if !self.physics.is_cursor_in_menu(&self.config) {
                            self.menu_pressed_item = None;
                            self.physics.close_menu();
                            needs_redraw = true;
                        }
                    } else {
                        if let Some(idx) = self.physics.icon_at_cursor(&self.config) {
                            if idx < self.pinned_apps.len() {
                                self.drag_state = Some(DragState {
                                    pinned_idx: idx,
                                    start_pos: (event.position.0 as f32, event.position.1 as f32),
                                    active: false,
                                    outside: false,
                                    reordered: false,
                                });
                            } else {
                                self.drag_state = None;
                            }
                        } else {
                            self.drag_state = None;
                        }
                        self.physics.set_pressed(true, &self.config);
                        needs_redraw = true;
                    }
                }
                PointerEventKind::Release { button, .. } if button == BTN_LEFT => {
                    if self.physics.is_menu_open() {
                        let pressed = self.menu_pressed_item.take();
                        let released = self.physics.menu_hit_test(&self.config);
                        if let (Some(a), Some(b)) = (pressed, released) {
                            if a == b {
                                if let Some(action) = self.physics.take_menu_action(a) {
                                    self.execute_menu_action(action);
                                    needs_redraw = true;
                                }
                            }
                        }
                    } else if let Some(drag) = self.drag_state.take().filter(|d| d.active) {
                        self.physics.set_pressed(false, &self.config);
                        if drag.outside {
                            if drag.pinned_idx < self.pinned_apps.len() {
                                let removed = self.pinned_apps.remove(drag.pinned_idx);
                                self.config.pinned_apps = self
                                    .pinned_apps
                                    .iter()
                                    .map(|a| a.id.clone())
                                    .collect();
                                self.config.save();
                                tracing::info!(app = %removed.name, "Aplicación desfijada mediante arrastre");
                                self.sync_dock_state();
                            }
                        } else if drag.reordered {
                            self.config.pinned_apps = self
                                .pinned_apps
                                .iter()
                                .map(|a| a.id.clone())
                                .collect();
                            self.config.save();
                            tracing::info!("Nuevo orden de aplicaciones fijadas guardado en config.toml");
                        }
                        needs_redraw = true;
                    } else {
                        self.drag_state = None;
                        if let Some(idx) = self.physics.set_pressed(false, &self.config) {
                            if let Some(entry) = self.entries.get(idx) {
                                match &entry.kind {
                                    DockEntryKind::App(app) => {
                                        if self.modifiers.shift {
                                            if !app.exec.is_empty() {
                                                self.physics.trigger_bounce(idx, &self.config);
                                                app.launch();
                                            }
                                        } else if !self.window_tracker.toggle_or_activate(app)
                                            && !app.exec.is_empty()
                                        {
                                            self.physics.trigger_bounce(idx, &self.config);
                                            app.launch();
                                        }
                                    }
                                    DockEntryKind::Spacer { .. } => {}
                                    DockEntryKind::Launchpad => dock_core::desktop::open_launchpad(),
                                DockEntryKind::Trash { .. } => {
                                        open_trash();
                                    }
                                }
                            }
                        }
                        needs_redraw = true;
                    }
                }
                PointerEventKind::Press { button, .. } if button == BTN_MIDDLE => {
                    if self.physics.is_menu_open() {
                        self.physics.close_menu();
                    }
                    if let Some(idx) = self.physics.icon_at_cursor(&self.config) {
                        if let Some(entry) = self.entries.get(idx) {
                            match &entry.kind {
                                DockEntryKind::App(app) => {
                                    if !app.exec.is_empty() {
                                        self.physics.trigger_bounce(idx, &self.config);
                                        app.launch();
                                    }
                                }
                                DockEntryKind::Spacer { .. } => {}
                                DockEntryKind::Launchpad => dock_core::desktop::open_launchpad(),
                                DockEntryKind::Trash { .. } => {
                                    open_trash();
                                }
                            }
                        }
                    }
                    needs_redraw = true;
                }
                PointerEventKind::Axis { vertical, .. } => {
                    let delta = if vertical.discrete != 0 {
                        vertical.discrete as f64
                    } else {
                        vertical.absolute
                    };
                    if delta.abs() > 0.0 && !self.physics.is_menu_open() {
                        let ready = self.last_scroll.map_or(true, |t| {
                            t.elapsed() >= Duration::from_millis(SCROLL_CYCLE_COOLDOWN_MS)
                        });
                        if ready {
                            if let Some(idx) = self.physics.icon_at_cursor(&self.config) {
                                if let Some(DockEntryKind::App(app)) =
                                    self.entries.get(idx).map(|e| &e.kind)
                                {
                                    if self.window_tracker.cycle_windows(app, delta > 0.0) {
                                        self.last_scroll = Some(Instant::now());
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if needs_redraw {
            self.request_redraw();
        }
    }
}

impl LayerShellHandler for DockApp {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let old_w = self.logical_width;
        let old_h = self.logical_height;

        if configure.new_size.0 > 0 {
            self.logical_width = configure.new_size.0;
        }
        if configure.new_size.1 > 0 {
            self.logical_height = configure.new_size.1;
        }

        if old_w != self.logical_width || old_h != self.logical_height {
            self.physics
                .adjust_cursor_for_resize(old_w, old_h, self.logical_width, self.logical_height);
        }

        let phys_w = self.logical_width * self.scale_factor;
        let phys_h = self.logical_height * self.scale_factor;
        layer.wl_surface().set_buffer_scale(self.scale_factor as i32);

        let icon_px = self.max_icon_phys_px();
        if let Some(renderer) = &mut self.renderer {
            renderer.resize(phys_w, phys_h, self.scale_factor as f32);
            renderer.sync_icons(&self.entries, icon_px);
        } else {
            let display_ptr = self.conn.backend().display_ptr() as *mut c_void;
            let surface_ptr = layer.wl_surface().id().as_ptr() as *mut c_void;
            match unsafe {
                DockRenderer::new(
                    display_ptr,
                    surface_ptr,
                    phys_w,
                    phys_h,
                    self.scale_factor as f32,
                )
            } {
                Ok(mut r) => {
                    r.sync_icons(&self.entries, icon_px);
                    self.renderer = Some(r);
                }
                Err(e) => {
                    tracing::error!("No se pudo iniciar DockRenderer: {e}");
                    self.exit = true;
                    return;
                }
            }
        }

        if !self.frame_pending {
            self.last_tick = Instant::now();
        }
        self.draw();
    }
}

impl Dispatch<OrgKdePlasmaWindowManagement, ()> for DockApp {
    fn event(
        state: &mut Self,
        proxy: &OrgKdePlasmaWindowManagement,
        event: org_kde_plasma_window_management::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        state.window_tracker.handle_manager_event(proxy, event, qh);
    }
}

impl Dispatch<OrgKdePlasmaWindow, ()> for DockApp {
    fn event(
        state: &mut Self,
        proxy: &OrgKdePlasmaWindow,
        event: org_kde_plasma_window::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if state.window_tracker.handle_window_event(proxy, event) {
            state.sync_dock_state();
        }
    }
}

impl ProvidesRegistryState for DockApp {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(DockApp);
delegate_output!(DockApp);
delegate_seat!(DockApp);
delegate_pointer!(DockApp);

impl KeyboardHandler for DockApp {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        self.modifiers = Modifiers::default();
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: u32,
    ) {
        self.modifiers = modifiers;
    }
}

delegate_keyboard!(DockApp);

fn parse_uri_list(raw: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("file://") {
            // Quitar posible hostname (file://localhost/...)
            let path_part = if let Some(stripped) = rest.strip_prefix("localhost") {
                stripped
            } else {
                rest
            };
            let bytes = path_part.as_bytes();
            let mut decoded = Vec::with_capacity(bytes.len());
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'%' && i + 2 < bytes.len() {
                    if let Ok(hex_str) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                        if let Ok(val) = u8::from_str_radix(hex_str, 16) {
                            decoded.push(val);
                            i += 3;
                            continue;
                        }
                    }
                }
                decoded.push(bytes[i]);
                i += 1;
            }
            if let Ok(s) = String::from_utf8(decoded) {
                paths.push(s);
            }
        } else {
            paths.push(trimmed.to_string());
        }
    }
    paths
}

fn move_files_to_trash(paths: &[String]) {
    if paths.is_empty() {
        return;
    }
    let mut args: Vec<&str> = Vec::with_capacity(paths.len() + 2);
    args.push("move");
    for p in paths {
        args.push(p.as_str());
    }
    args.push("trash:/");

    let ok = std::process::Command::new("kioclient")
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok();

    if !ok {
        let _ = std::process::Command::new("gio")
            .arg("trash")
            .args(paths)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

impl DataDeviceHandler for DockApp {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_data_device::WlDataDevice,
        x: f64,
        y: f64,
        wl_surface: &wl_surface::WlSurface,
    ) {
        if wl_surface != self.layer_surface.wl_surface() {
            return;
        }
        self.hide_deadline = None;
        if self.config.hiding_enabled() {
            self.physics.set_hidden(false);
        }
        self.physics.set_cursor(Some((x as f32, y as f32)));

        if let Some(offer) = self
            .data_device
            .as_ref()
            .and_then(|d| d.data().drag_offer())
        {
            let has_uris = offer.with_mime_types(|mimes| {
                mimes.iter().any(|m| m == "text/uri-list")
            });
            tracing::info!(has_uris, "DnD enter recibido en el dock");
            if has_uris {
                offer.accept_mime_type(offer.serial, Some("text/uri-list".to_string()));
                offer.set_actions(DndAction::Copy | DndAction::Move, DndAction::Copy);
            }
        }
        self.request_redraw();
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_data_device::WlDataDevice,
    ) {
        self.physics.set_cursor(None);
        if self.config.hiding_enabled() && self.should_hide_now() {
            self.hide_deadline = Some(
                Instant::now() + Duration::from_millis(self.config.auto_hide_delay_ms),
            );
        }
        self.request_redraw();
    }

    fn motion(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_data_device::WlDataDevice,
        x: f64,
        y: f64,
    ) {
        self.physics.set_cursor(Some((x as f32, y as f32)));
        self.request_redraw();
    }

    fn selection(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_data_device::WlDataDevice,
    ) {
    }

    fn drop_performed(
        &mut self,
        conn: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_data_device::WlDataDevice,
    ) {
        let Some(offer) = self
            .data_device
            .as_ref()
            .and_then(|d| d.data().drag_offer())
        else {
            return;
        };

        let target = self
            .physics
            .icon_at_cursor(&self.config)
            .and_then(|idx| self.entries.get(idx).map(|e| (idx, e.kind.clone())));

        if let Ok(mut pipe) = offer.receive("text/uri-list".to_string()) {
            let _ = conn.flush();
            if let Some((idx, _)) = &target {
                self.physics.trigger_bounce(*idx, &self.config);
            }
            let tx = self.event_tx.clone();
            std::thread::spawn(move || {
                use std::io::Read;
                let mut raw = String::new();
                if pipe.read_to_string(&mut raw).is_ok() {
                    let paths = parse_uri_list(&raw);
                    tracing::info!(?paths, "DnD drop recibido en el dock");
                    if paths.is_empty() {
                        return;
                    }

                    let desktop_items: Vec<String> = paths
                        .iter()
                        .filter(|p| {
                            p.ends_with(".desktop") || p.starts_with("applications:")
                        })
                        .cloned()
                        .collect();

                    if !desktop_items.is_empty() {
                        let _ = tx.send(SystemEvent::PinDesktopFiles(desktop_items));
                    } else if let Some((_, kind)) = target {
                        match kind {
                            DockEntryKind::App(app) => app.launch_with_files(&paths),
                            DockEntryKind::Trash { .. } => move_files_to_trash(&paths),
                            DockEntryKind::Spacer { .. } | DockEntryKind::Launchpad => {},
                        }
                    }
                }
            });
        }

        offer.finish();
        self.physics.set_cursor(None);
        self.request_redraw();
    }
}

impl DataOfferHandler for DockApp {
    fn source_actions(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }

    fn selected_action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }
}

impl DataSourceHandler for DockApp {
    fn accept_mime(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_data_source::WlDataSource,
        _: Option<String>,
    ) {
    }

    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_data_source::WlDataSource,
        _: String,
        _: WritePipe,
    ) {
    }

    fn cancelled(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_data_source::WlDataSource,
    ) {
    }

    fn dnd_dropped(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_data_source::WlDataSource,
    ) {
    }

    fn dnd_finished(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_data_source::WlDataSource,
    ) {
    }

    fn action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_data_source::WlDataSource,
        _: DndAction,
    ) {
    }
}

delegate_data_device!(DockApp);


delegate_layer!(DockApp);
delegate_registry!(DockApp);
delegate_noop!(DockApp: ignore ExtBackgroundEffectManagerV1);
delegate_noop!(DockApp: ignore ExtBackgroundEffectSurfaceV1);
delegate_noop!(DockApp: ignore OrgKdeKwinBlurManager);
delegate_noop!(DockApp: ignore OrgKdeKwinBlur);

pub fn run_wayland_loop(
    config: DockConfig,
    catalog: AppCatalog,
    pinned_apps: Vec<DesktopApp>,
    trash_full: bool,
    event_tx: calloop::channel::Sender<SystemEvent>,
    event_rx: Channel<SystemEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::connect_to_env()?;
    let (globals, event_queue) = registry_queue_init::<DockApp>(&conn)?;
    let qh = event_queue.handle();

    let compositor_state = CompositorState::bind(&globals, &qh)?;
    let layer_shell = LayerShell::bind(&globals, &qh)?;
    let seat_state = SeatState::new(&globals, &qh);

    let mut initial_entries: Vec<DockEntry> = pinned_apps
        .iter()
        .cloned()
        .map(|app| DockEntry::from_app(app, DockSection::Pinned))
        .collect();
    if config.show_trash {
        initial_entries.push(catalog.trash_entry(trash_full));
    }

    let physics = DockPhysics::new(&initial_entries, &config);
    let (canvas_w, canvas_h) = physics.canvas_size(&config);
    let resting_pill_h = (config.icon_size + config.pill_padding_y * 2.0).round() as i32;

    let surface = compositor_state.create_surface(&qh);
    let layer_surface = layer_shell.create_layer_surface(
        &qh,
        surface,
        Layer::Overlay,
        Some(config.namespace.clone()),
        None,
    );

    let anchor = match config.position {
        dock_core::DockPosition::Bottom => Anchor::BOTTOM,
        dock_core::DockPosition::Top => Anchor::TOP,
    };
    layer_surface.set_anchor(anchor);
    layer_surface.set_size(canvas_w, canvas_h);
    layer_surface.set_margin(0, 0, 0, 0);
    if config.hiding_enabled() {
        layer_surface.set_exclusive_zone(-1);
    } else {
        layer_surface.set_exclusive_zone(resting_pill_h + config.bottom_margin);
    }
    layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);

    let blur = BlurController::new(
        config.blur_mode,
        &config.namespace,
        &globals,
        layer_surface.wl_surface(),
        &qh,
    );
    let window_tracker = WindowTracker::bind(&globals, &qh);

    layer_surface.commit();

    let initial_hide_deadline = if config.effective_hide_mode() == HideMode::AutoHide {
        Some(Instant::now() + Duration::from_millis(config.auto_hide_delay_ms.max(900)))
    } else {
        None
    };

    tracing::info!(hide_mode = ?config.effective_hide_mode(), "Modo de ocultamiento activo");

    let mut app = DockApp {
        renderer: None,
        blur,
        window_tracker,
        pointer: None,
        keyboard: None,
        modifiers: Modifiers::default(),
        drag_state: None,
        data_device_manager: DataDeviceManagerState::bind(&globals, &qh).ok(),
        data_device: None,
        event_tx,
        active_output: None,
        layer_surface,
        conn: conn.clone(),
        qh: qh.clone(),
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        compositor_state,
        seat_state,
        config,
        catalog,
        pinned_apps,
        recent_apps: Vec::new(),
        entries: initial_entries,
        trash_full,
        badges: HashMap::new(),
        physics,
        logical_width: canvas_w,
        logical_height: canvas_h,
        requested_width: canvas_w,
        requested_height: canvas_h,
        scale_factor: 1,
        theme: ThemeTransition::default(),
        hide_deadline: initial_hide_deadline,
        menu_leave_deadline: None,
        menu_pressed_item: None,
        last_scroll: None,
        last_input_rects: Vec::new(),
        last_tick: Instant::now(),
        frame_pending: false,
        dirty: false,
        animating: false,
        exit: false,
    };

    let mut event_loop: EventLoop<DockApp> = EventLoop::try_new()?;
    let loop_handle = event_loop.handle();

    WaylandSource::new(conn, event_queue).insert(loop_handle.clone())?;

    loop_handle
        .insert_source(event_rx, |event, _, state| {
            if let ChannelEvent::Msg(msg) = event {
                match msg {
                    SystemEvent::SetDark(is_dark) => {
                        tracing::info!(is_dark, "Cambio de esquema de color detectado");
                        state.theme.set_dark(is_dark);
                        state.request_redraw();
                    }
                    SystemEvent::TrashChanged(full) => {
                        if state.trash_full != full {
                            tracing::info!(full, "Estado de la Papelera actualizado");
                            state.trash_full = full;
                            state.sync_dock_state();
                        }
                    }
                    SystemEvent::BadgeUpdate { app_id, count } => {
                        let key = normalize_id(&app_id);
                        let changed = if count == 0 {
                            state.badges.remove(&key).is_some()
                        } else {
                            state.badges.insert(key, count) != Some(count)
                        };
                        if changed {
                            state.sync_dock_state();
                        }
                    }
                    SystemEvent::ConfigReloaded(new_config) => {
                        tracing::info!("config.toml recargado en caliente");
                        state.pinned_apps = state.catalog.resolve_pinned(&new_config.pinned_apps);
                        state.config = new_config;
                        let anchor = match state.config.position {
                            dock_core::DockPosition::Bottom => Anchor::BOTTOM,
                            dock_core::DockPosition::Top => Anchor::TOP,
                        };
                        state.layer_surface.set_anchor(anchor);
                        state.sync_dock_state();
                        state.sync_canvas_size();
                        state.layer_surface.commit();
                        state.request_redraw();
                        let _ = state.conn.flush();
                    }
                    SystemEvent::PinDesktopFiles(uris) => {
                        let mut changed = false;
                        for uri in uris {
                            let cleaned = uri
                                .trim()
                                .trim_start_matches("applications://")
                                .trim_start_matches("applications:");
                            let desktop_id = std::path::Path::new(cleaned)
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or(cleaned)
                                .trim_end_matches(".desktop")
                                .to_string();
                            if desktop_id.is_empty() {
                                continue;
                            }
                            let resolved = state.catalog.resolve_pinned(&[desktop_id.clone()]);
                            if let Some(app) = resolved.into_iter().next() {
                                let target = normalize_id(&app.id);
                                if !state
                                    .pinned_apps
                                    .iter()
                                    .any(|p| normalize_id(&p.id) == target)
                                {
                                    tracing::info!(app = %app.name, id = %app.id, "Aplicación fijada mediante Drag & Drop");
                                    state.config.pinned_apps.push(app.id.clone());
                                    state.pinned_apps.push(app);
                                    state.recent_apps.retain(|r| normalize_id(&r.id) != target);
                                    changed = true;
                                }
                            } else {
                                tracing::warn!(uri = %uri, id = %desktop_id, "No se encontró el .desktop en AppCatalog");
                            }
                        }
                        if changed {
                            state.config.save();
                            state.sync_dock_state();
                            state.sync_canvas_size();
                            state.request_redraw();
                        }
                    }
                }
            }
        })
        .map_err(|e| format!("Error registrando canal de eventos del sistema: {e:?}"))?;

    while !app.exit {
        let timeout = app.next_timeout();
        event_loop.dispatch(timeout, &mut app)?;
        app.check_deadlines();
    }

    Ok(())
}
