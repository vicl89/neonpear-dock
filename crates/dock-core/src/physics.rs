use std::collections::HashMap;
use std::f32::consts::PI;

use crate::desktop::{DockEntry, DockSection};
use crate::menu::{
    layout_item_rects, measure_menu, ContextMenuAction, ContextMenuItem, MenuLayout, MENU_GAP,
};
use crate::DockConfig;

const BOUNCE_AMPLITUDE: f32 = 22.0;
const BOUNCE_CYCLE_SECS: f32 = 0.56;
const BOUNCE_MAX_SECS: f32 = 6.0;
const DOT_RADIUS: f32 = 2.0;
const DIVIDER_WIDTH: f32 = 1.5;
const TOOLTIP_HEADROOM: f32 = 44.0;
const TOOLTIP_GAP: f32 = 8.0;

#[derive(Debug, Clone, Copy)]
pub struct PillRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl PillRect {
    pub fn center(&self) -> [f32; 2] {
        [self.x + self.width * 0.5, self.y + self.height * 0.5]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct IconSlot {
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub pressed: bool,
    pub opacity: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct DotSlot {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub opacity: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct BadgeSlot {
    /// Centro X del globo en coordenadas lógicas del lienzo.
    pub x: f32,
    /// Centro Y del globo en coordenadas lógicas del lienzo.
    pub y: f32,
    pub scale: f32,
    pub count: u64,
    pub opacity: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct DividerSlot {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone)]
pub struct TooltipSlot {
    pub label: String,
    /// Centro X del icono objetivo en coordenadas lógicas del lienzo.
    pub anchor_x: f32,
    /// Coordenada Y donde descansa la punta inferior de la flecha del tooltip.
    pub anchor_y: f32,
    pub opacity: f32,
    pub point_up: bool,
}

#[derive(Debug, Clone)]
pub struct FrameLayout {
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub pill: PillRect,
    pub icons: Vec<IconSlot>,
    pub dots: Vec<DotSlot>,
    pub badges: Vec<BadgeSlot>,
    pub dividers: Vec<DividerSlot>,
    pub tooltip: Option<TooltipSlot>,
    pub menu: Option<MenuLayout>,
    pub hide_progress: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct BounceState {
    active: bool,
    /// Fase normalizada [0.0, 1.0) de un salto individual.
    phase: f32,
    elapsed: f32,
}

impl BounceState {
    fn offset_y(&self) -> f32 {
        if self.phase <= 0.0001 {
            return 0.0;
        }
        (self.phase * PI).sin().powf(0.85) * BOUNCE_AMPLITUDE
    }

    fn is_animating(&self) -> bool {
        self.active || self.phase > 0.0001
    }
}

#[derive(Debug, Clone)]
struct ItemPhysics {
    key: String,
    label: String,
    section: DockSection,
    current_size: f32,
    running: bool,
    minimized: bool,
    demands_attention: bool,
    icon_opacity: f32,
    dot_opacity: f32,
    badge_count: u64,
    badge_opacity: f32,
    bounce: BounceState,
}

#[derive(Debug, Clone)]
enum MenuAnchor {
    Entry(String),
    PointX(f32),
}

#[derive(Debug, Clone)]
struct MenuState {
    anchor: MenuAnchor,
    items: Vec<ContextMenuItem>,
    hovered_item: Option<usize>,
    opacity: f32,
    closing: bool,
}

pub struct DockPhysics {
    items: Vec<ItemPhysics>,
    current_pill_width: f32,
    cursor: Option<(f32, f32)>,
    hovered_index: Option<usize>,
    pressed_index: Option<usize>,
    hidden: bool,
    hide_progress: f32,
    tooltip_label: Option<String>,
    tooltip_anchor_x: f32,
    tooltip_anchor_y: f32,
    tooltip_opacity: f32,
    tooltip_has_pos: bool,
    menu: Option<MenuState>,
}

impl DockPhysics {
    pub fn new(entries: &[DockEntry], config: &DockConfig) -> Self {
        let items: Vec<ItemPhysics> = entries
            .iter()
            .map(|e| ItemPhysics {
                key: e.key.clone(),
                label: e.label.clone(),
                section: e.section,
                current_size: config.icon_size,
                running: false,
                minimized: false,
                demands_attention: false,
                icon_opacity: 1.0,
                dot_opacity: 0.0,
                badge_count: 0,
                badge_opacity: 0.0,
                bounce: BounceState::default(),
            })
            .collect();

        let resting_width = resting_pill_width(&items, config);
        Self {
            items,
            current_pill_width: resting_width,
            cursor: None,
            hovered_index: None,
            pressed_index: None,
            hidden: false,
            hide_progress: 0.0,
            tooltip_label: None,
            tooltip_anchor_x: 0.0,
            tooltip_anchor_y: 0.0,
            tooltip_opacity: 0.0,
            tooltip_has_pos: false,
            menu: None,
        }
    }

    /// Sincroniza la lista visible de iconos preservando animaciones por clave.
    /// Devuelve `true` si cambió la composición, orden o etiquetas.
    pub fn sync_entries(&mut self, entries: &[DockEntry], config: &DockConfig) -> bool {
        let unchanged = self.items.len() == entries.len()
            && self.items.iter().zip(entries.iter()).all(|(old, new)| {
                old.key == new.key && old.section == new.section && old.label == new.label
            });
        if unchanged {
            return false;
        }

        let pressed_key = self
            .pressed_index
            .and_then(|i| self.items.get(i))
            .map(|it| it.key.clone());

        let mut existing: HashMap<String, ItemPhysics> = self
            .items
            .drain(..)
            .map(|it| (it.key.clone(), it))
            .collect();

        let mut rebuilt = Vec::with_capacity(entries.len());
        for entry in entries {
            if let Some(mut old) = existing.remove(&entry.key) {
                old.section = entry.section;
                old.label = entry.label.clone();
                rebuilt.push(old);
            } else {
                rebuilt.push(ItemPhysics {
                    key: entry.key.clone(),
                    label: entry.label.clone(),
                    section: entry.section,
                    current_size: (config.icon_size * 0.35).max(8.0),
                    running: false,
                    minimized: false,
                    demands_attention: false,
                    icon_opacity: 1.0,
                    dot_opacity: 0.0,
                    badge_count: 0,
                    badge_opacity: 0.0,
                    bounce: BounceState::default(),
                });
            }
        }

        self.items = rebuilt;

        // Si el icono asociado al menú abierto desapareció, cerramos el menú
        if let Some(menu) = &self.menu {
            if let MenuAnchor::Entry(key) = &menu.anchor {
                if !self.items.iter().any(|it| &it.key == key) {
                    self.menu = None;
                }
            }
        }

        self.pressed_index = pressed_key
            .and_then(|k| self.items.iter().position(|it| it.key == k));
        self.hovered_index = self.hit_test(config);
        true
    }

    pub fn icon_count(&self) -> usize {
        self.items.len()
    }

    pub fn icon_is_running(&self, index: usize) -> bool {
        self.items.get(index).map(|it| it.running).unwrap_or(false)
    }

    pub fn set_cursor(&mut self, pos: Option<(f32, f32)>) {
        self.cursor = pos;
        if pos.is_none() {
            self.hovered_index = None;
            self.pressed_index = None;
            if let Some(menu) = &mut self.menu {
                menu.hovered_item = None;
            }
        }
    }

    /// Ajusta la posición almacenada del cursor cuando la superficie Wayland cambia de tamaño
    /// manteniendo anclaje inferior centrado.
    pub fn adjust_cursor_for_resize(&mut self, old_w: u32, old_h: u32, new_w: u32, new_h: u32) {
        if let Some((cx, cy)) = &mut self.cursor {
            *cx += (new_w as f32 - old_w as f32) * 0.5;
            *cy += new_h as f32 - old_h as f32;
        }
    }

    pub fn is_cursor_inside(&self) -> bool {
        self.cursor.is_some()
    }

    /// Abre un menú contextual anclado al icono `index`.
    pub fn open_menu(&mut self, index: usize, items: Vec<ContextMenuItem>) -> bool {
        let Some(entry_key) = self.items.get(index).map(|it| it.key.clone()) else {
            return false;
        };
        if items.is_empty() {
            return self.close_menu();
        }

        self.pressed_index = None;
        self.hovered_index = None;
        self.menu = Some(MenuState {
            anchor: MenuAnchor::Entry(entry_key),
            items,
            hovered_item: None,
            opacity: 0.0,
            closing: false,
        });
        true
    }

    /// Abre el menú rápido del dock anclado a la coordenada X del clic.
    pub fn open_dock_menu(&mut self, anchor_x: f32, items: Vec<ContextMenuItem>) -> bool {
        if items.is_empty() {
            return self.close_menu();
        }
        self.pressed_index = None;
        self.hovered_index = None;
        self.menu = Some(MenuState {
            anchor: MenuAnchor::PointX(anchor_x),
            items,
            hovered_item: None,
            opacity: 0.0,
            closing: false,
        });
        true
    }

    /// Inicia el cierre con fundido del menú contextual.
    pub fn close_menu(&mut self) -> bool {
        let Some(menu) = &mut self.menu else {
            return false;
        };
        if menu.closing {
            return false;
        }
        menu.closing = true;
        menu.hovered_item = None;
        true
    }

    /// Devuelve `true` si el menú está abierto y activo (no en proceso de cierre).
    pub fn is_menu_open(&self) -> bool {
        self.menu.as_ref().map(|m| !m.closing).unwrap_or(false)
    }

    /// Devuelve `true` mientras el menú exista en pantalla (abierto o desvaneciéndose).
    pub fn has_menu(&self) -> bool {
        self.menu.is_some()
    }

    /// Devuelve el índice del ítem habilitado bajo el cursor dentro del menú abierto.
    pub fn menu_hit_test(&self, config: &DockConfig) -> Option<usize> {
        let menu = self.menu.as_ref().filter(|m| !m.closing)?;
        let (cx, cy) = self.cursor?;
        let layout = self.compute_menu_layout(menu, config)?;
        layout
            .item_rects
            .iter()
            .enumerate()
            .find_map(|(i, rect)| {
                let enabled = menu.items.get(i).map(|it| it.enabled).unwrap_or(false);
                let inside = enabled
                    && cx >= rect[0]
                    && cx <= rect[0] + rect[2]
                    && cy >= rect[1]
                    && cy <= rect[1] + rect[3];
                inside.then_some(i)
            })
    }

    /// Indica si el cursor está dentro de la tarjeta del menú abierto.
    pub fn is_cursor_in_menu(&self, config: &DockConfig) -> bool {
        let Some(menu) = self.menu.as_ref().filter(|m| !m.closing) else {
            return false;
        };
        let Some((cx, cy)) = self.cursor else {
            return false;
        };
        let Some(layout) = self.compute_menu_layout(menu, config) else {
            return false;
        };
        cx >= layout.x
            && cx <= layout.x + layout.width
            && cy >= layout.y
            && cy <= layout.y + layout.box_height
    }

    /// Extrae la acción del ítem seleccionado y cierra el menú.
    pub fn take_menu_action(&mut self, index: usize) -> Option<ContextMenuAction> {
        let action = self
            .menu
            .as_ref()
            .filter(|m| !m.closing)
            .and_then(|m| m.items.get(index))
            .filter(|it| it.enabled)
            .map(|it| it.action.clone())?;
        self.close_menu();
        Some(action)
    }

    /// Devuelve el icono bajo el cursor ignorando si hay un menú abierto (útil para clic derecho).
    pub fn icon_at_cursor(&self, config: &DockConfig) -> Option<usize> {
        if self.hidden || self.hide_progress > 0.35 {
            return None;
        }
        let (cx, cy) = self.cursor?;
        let layout = self.compute_frame(config);
        layout.icons.iter().enumerate().find_map(|(i, icon)| {
            let hit = cx >= icon.x
                && cx <= icon.x + icon.size
                && cy >= icon.y
                && cy <= icon.y + icon.size;
            hit.then_some(i)
        })
    }

    /// Controla si el dock debe deslizarse fuera de pantalla (cuando `auto_hide` está activo).
    /// Devuelve `true` si cambió el estado objetivo.
    pub fn set_hidden(&mut self, hidden: bool) -> bool {
        if self.hidden == hidden {
            return false;
        }
        self.hidden = hidden;
        if hidden {
            self.hovered_index = None;
            self.pressed_index = None;
            self.close_menu();
        }
        true
    }

    pub fn is_hidden(&self) -> bool {
        self.hidden
    }

    pub fn hide_progress(&self) -> f32 {
        self.hide_progress
    }

    /// Devuelve `(ancho, alto)` de la píldora en reposo (sin ampliación) para calcular colisiones con ventanas.
    pub fn resting_pill_size(&self, config: &DockConfig) -> (f32, f32) {
        let width = resting_pill_width(&self.items, config);
        let height = config.icon_size + config.pill_padding_y * 2.0;
        (width, height)
    }

    pub fn set_pressed(&mut self, pressed: bool, config: &DockConfig) -> Option<usize> {
        if pressed {
            self.pressed_index = self.hit_test(config);
            None
        } else {
            let released_on = self.hit_test(config);
            match (self.pressed_index.take(), released_on) {
                (Some(a), Some(b)) if a == b => Some(a),
                _ => None,
            }
        }
    }

    pub fn hovered_index(&self) -> Option<usize> {
        self.hovered_index
    }

    /// Inicia la animación de rebote al lanzar una app si está habilitada en la configuración.
    pub fn trigger_bounce(&mut self, index: usize, config: &DockConfig) {
        if !config.animate_opening {
            return;
        }
        if let Some(item) = self.items.get_mut(index) {
            item.bounce.active = true;
            item.bounce.elapsed = 0.0;
        }
    }

    /// Actualiza el contador de notificaciones (badge) de cada entrada.
    pub fn set_badge_counts(&mut self, counts: &[u64]) -> bool {
        let mut changed = false;
        for (item, &count) in self.items.iter_mut().zip(counts.iter()) {
            if item.badge_count != count {
                item.badge_count = count;
                changed = true;
            }
        }
        changed
    }

    /// Actualiza qué entradas tienen todas sus ventanas minimizadas/ocultas.
    pub fn set_minimized_states(&mut self, states: &[bool]) -> bool {
        let mut changed = false;
        for (item, &is_min) in self.items.iter_mut().zip(states.iter()) {
            if item.minimized != is_min {
                item.minimized = is_min;
                changed = true;
            }
        }
        changed
    }

    /// Actualiza qué entradas solicitan atención (`STATE_DEMANDS_ATTENTION`) y dispara el rebote.
    pub fn set_attention_states(&mut self, states: &[bool], config: &DockConfig) -> bool {
        let mut changed = false;
        for (item, &wants_attn) in self.items.iter_mut().zip(states.iter()) {
            if item.demands_attention != wants_attn {
                item.demands_attention = wants_attn;
                if wants_attn && config.bounce_on_attention {
                    item.bounce.active = true;
                    item.bounce.elapsed = 0.0;
                } else if !wants_attn && item.running {
                    item.bounce.active = false;
                }
                changed = true;
            }
        }
        changed
    }

    /// Actualiza qué elementos tienen ventanas abiertas.
    pub fn set_running_states(&mut self, states: &[bool]) -> bool {
        let mut changed = false;
        for (item, &is_open) in self.items.iter_mut().zip(states.iter()) {
            if item.running != is_open {
                item.running = is_open;
                changed = true;
            }
            if is_open && item.bounce.active {
                item.bounce.active = false;
                changed = true;
            }
        }
        changed
    }

    /// Calcula el tamaño lógico del lienzo Wayland reservando margen para ampliación,
    /// rebote, tooltips, menú contextual activo y el margen inferior integrado.
    pub fn canvas_size(&self, config: &DockConfig) -> (u32, u32) {
        let n = self.items.len();
        let rest_w = resting_pill_width(&self.items, config);
        let extra_per_icon = if config.magnification_enabled {
            (config.max_icon_size - config.icon_size).max(0.0)
        } else {
            0.0
        };

        let mut side_pad: f32 = if config.show_tooltips { 96.0 } else { 24.0 };
        let max_icon = if config.magnification_enabled {
            config.max_icon_size.max(config.icon_size)
        } else {
            config.icon_size
        };
        let bounce_headroom = if config.animate_opening {
            BOUNCE_AMPLITUDE
        } else {
            0.0
        };
        let tooltip_headroom = if config.show_tooltips {
            TOOLTIP_HEADROOM
        } else {
            0.0
        };
        let bottom_gap = config.bottom_margin.max(0) as f32;

        let mut canvas_h = (max_icon
            + config.pill_padding_y * 2.0
            + bounce_headroom
            + tooltip_headroom
            + bottom_gap
            + 10.0)
            .ceil() as u32;

        if let Some(menu) = &self.menu {
            let (menu_w, _, menu_total_h) = measure_menu(&menu.items);
            side_pad = side_pad.max(menu_w + 24.0);
            let icon_top_from_bottom = max_icon + config.pill_padding_y + 2.0 + bottom_gap;
            let needed_h = (icon_top_from_bottom + MENU_GAP + menu_total_h + 18.0).ceil() as u32;
            canvas_h = canvas_h.max(needed_h);
        }

        let max_pill_w = rest_w + extra_per_icon * (n.min(4) as f32);
        let canvas_w = (max_pill_w + side_pad).ceil() as u32;

        (canvas_w.max(120), canvas_h.max(72))
    }

    /// Avanza ampliación, rebote, indicadores, auto-ocultamiento, tooltip y menú contextual.
    pub fn step(&mut self, dt_secs: f32, config: &DockConfig) -> bool {
        let n = self.items.len();
        if n == 0 {
            return false;
        }

        let mut animating = false;

        // 0. Deslizamiento suave de auto-ocultamiento (pausado si hay menú abierto)
        let target_hide = if config.hiding_enabled() && self.hidden && self.menu.is_none() {
            1.0
        } else {
            0.0
        };
        let hide_diff = target_hide - self.hide_progress;
        if hide_diff.abs() > 0.002 {
            let hide_speed = if target_hide > self.hide_progress {
                11.0
            } else {
                16.0
            };
            let hide_factor = (1.0 - (-hide_speed * dt_secs).exp()).clamp(0.0, 1.0);
            self.hide_progress += hide_diff * hide_factor;
            animating = true;
        } else {
            self.hide_progress = target_hide;
        }

        let (canvas_w, canvas_h) = self.canvas_size(config);
        let bottom_gap = config.bottom_margin.max(0) as f32;
        let pill_h = config.icon_size + config.pill_padding_y * 2.0;
        let pill_top = canvas_h as f32 - bottom_gap - pill_h;
        let rest_w = resting_pill_width(&self.items, config);
        let rest_start_x = (canvas_w as f32 - rest_w) * 0.5 + config.pill_padding_x;

        let active_cursor_x = if self.hidden || self.hide_progress > 0.35 || self.is_menu_open() {
            None
        } else {
            self.cursor.and_then(|(cx, cy)| {
                let inside_x = cx >= rest_start_x - config.pill_padding_x - 18.0
                    && cx <= rest_start_x + rest_w - config.pill_padding_x + 18.0;
                let inside_y = if config.position == crate::DockPosition::Top {
                    cy >= 0.0 && cy <= bottom_gap + pill_h + 14.0
                } else {
                    cy >= pill_top - 14.0 && cy <= canvas_h as f32
                };
                (inside_x && inside_y).then_some(cx)
            })
        };

        let sigma = (config.icon_size * 1.45).max(1.0);
        let two_sigma_sq = 2.0 * sigma * sigma;
        let max_delta = if config.magnification_enabled {
            (config.max_icon_size - config.icon_size).max(0.0)
        } else {
            0.0
        };

        let factor = (1.0 - (-22.0 * dt_secs).exp()).clamp(0.0, 1.0);
        let dot_factor = (1.0 - (-14.0 * dt_secs).exp()).clamp(0.0, 1.0);

        let mut rest_centers = Vec::with_capacity(n);
        let mut rx = rest_start_x;
        for i in 0..n {
            rest_centers.push(rx + config.icon_size * 0.5);
            rx += config.icon_size;
            if i + 1 < n {
                rx += gap_between(self.items[i].section, self.items[i + 1].section, config);
            }
        }

        // 1. Ampliación gaussiana de iconos (congelada mientras el menú contextual está abierto)
        let menu_open = self.is_menu_open();
        for (i, item) in self.items.iter_mut().enumerate() {
            let target = if menu_open {
                item.current_size.max(config.icon_size)
            } else {
                match active_cursor_x {
                    Some(cx) if max_delta > 0.0 => {
                        let dist = cx - rest_centers[i];
                        config.icon_size + max_delta * (-(dist * dist) / two_sigma_sq).exp()
                    }
                    _ => config.icon_size,
                }
            };

            let diff = target - item.current_size;
            if diff.abs() > 0.08 {
                item.current_size += diff * factor;
                animating = true;
            } else {
                item.current_size = target;
            }
        }

        // 2. Ancho elástico de la píldora incluyendo separadores
        let content_w = current_content_width(&self.items, config);
        let target_pill_w = content_w + config.pill_padding_x * 2.0;

        let pw_diff = target_pill_w - self.current_pill_width;
        if pw_diff.abs() > 0.08 {
            self.current_pill_width += pw_diff * factor;
            animating = true;
        } else {
            self.current_pill_width = target_pill_w;
        }

        // 3. Opacidad suave de los puntos indicadores y badges
        for item in &mut self.items {
            let target_alpha = if config.show_indicators && item.running {
                1.0
            } else {
                0.0
            };
            let diff = target_alpha - item.dot_opacity;
            if diff.abs() > 0.005 {
                item.dot_opacity += diff * dot_factor;
                animating = true;
            } else {
                item.dot_opacity = target_alpha;
            }

            let target_badge = if config.show_badges && item.badge_count > 0 {
                1.0
            } else {
                0.0
            };
            let b_diff = target_badge - item.badge_opacity;
            if b_diff.abs() > 0.005 {
                item.badge_opacity += b_diff * dot_factor;
                animating = true;
            } else {
                item.badge_opacity = target_badge;
            }

            let target_icon_alpha = if config.dim_minimized && item.running && item.minimized {
                0.55
            } else {
                1.0
            };
            let i_diff = target_icon_alpha - item.icon_opacity;
            if i_diff.abs() > 0.005 {
                item.icon_opacity += i_diff * dot_factor;
                animating = true;
            } else {
                item.icon_opacity = target_icon_alpha;
            }
        }

        // 4. Física de rebote al abrir
        for item in &mut self.items {
            if !item.bounce.is_animating() {
                continue;
            }
            item.bounce.elapsed += dt_secs;
            if !item.demands_attention && item.bounce.elapsed >= BOUNCE_MAX_SECS {
                item.bounce.active = false;
            }
            item.bounce.phase += dt_secs / BOUNCE_CYCLE_SECS;
            if item.bounce.phase >= 1.0 {
                if item.bounce.active {
                    item.bounce.phase -= 1.0;
                } else {
                    item.bounce.phase = 0.0;
                }
            }
            if item.bounce.is_animating() {
                animating = true;
            }
        }

        self.hovered_index = self.hit_test(config);

        // 5. Tooltip flotante con seguimiento suave y fundido de opacidad
        let active_tooltip_idx = if config.show_tooltips
            && self.menu.is_none()
            && !self.hidden
            && self.hide_progress < 0.15
            && self.pressed_index.is_none()
        {
            self.hovered_index.filter(|&i| {
                self.items
                    .get(i)
                    .map(|it| !it.bounce.is_animating())
                    .unwrap_or(false)
            })
        } else {
            None
        };

        let target_tip_alpha = if let Some(idx) = active_tooltip_idx {
            if let Some((tx, ty)) = self.icon_tooltip_anchor(idx, config) {
                self.tooltip_label = Some(self.items[idx].label.clone());
                if !self.tooltip_has_pos || self.tooltip_opacity < 0.05 {
                    self.tooltip_anchor_x = tx;
                    self.tooltip_anchor_y = ty;
                    self.tooltip_has_pos = true;
                } else {
                    let glide = (1.0 - (-26.0 * dt_secs).exp()).clamp(0.0, 1.0);
                    let dx = tx - self.tooltip_anchor_x;
                    let dy = ty - self.tooltip_anchor_y;
                    if dx.abs() > 0.2 || dy.abs() > 0.2 {
                        self.tooltip_anchor_x += dx * glide;
                        self.tooltip_anchor_y += dy * glide;
                        animating = true;
                    } else {
                        self.tooltip_anchor_x = tx;
                        self.tooltip_anchor_y = ty;
                    }
                }
                1.0
            } else {
                0.0
            }
        } else {
            0.0
        };

        let tip_diff = target_tip_alpha - self.tooltip_opacity;
        if tip_diff.abs() > 0.005 {
            let speed = if target_tip_alpha > self.tooltip_opacity {
                22.0
            } else {
                18.0
            };
            let tip_factor = (1.0 - (-speed * dt_secs).exp()).clamp(0.0, 1.0);
            self.tooltip_opacity += tip_diff * tip_factor;
            animating = true;
        } else {
            self.tooltip_opacity = target_tip_alpha;
            if self.tooltip_opacity <= 0.001 {
                self.tooltip_has_pos = false;
            }
        }

        // 6. Fundido y hover del menú contextual
        let next_menu_hover = self.menu_hit_test(config);
        let mut remove_menu = false;
        if let Some(menu) = &mut self.menu {
            menu.hovered_item = next_menu_hover;
            let target_alpha = if menu.closing { 0.0 } else { 1.0 };
            let diff = target_alpha - menu.opacity;
            if diff.abs() > 0.005 {
                let speed = if menu.closing { 26.0 } else { 28.0 };
                let m_factor = (1.0 - (-speed * dt_secs).exp()).clamp(0.0, 1.0);
                menu.opacity += diff * m_factor;
                animating = true;
            } else {
                menu.opacity = target_alpha;
                if menu.closing {
                    remove_menu = true;
                }
            }
        }
        if remove_menu {
            self.menu = None;
        }

        animating
    }

    pub fn compute_frame(&self, config: &DockConfig) -> FrameLayout {
        let (canvas_width, canvas_height) = self.canvas_size(config);
        let bottom_gap = config.bottom_margin.max(0) as f32;
        let pill_height = config.icon_size + config.pill_padding_y * 2.0;
        let slide_distance = pill_height + bottom_gap + 6.0;
        let hide_offset_y = self.hide_progress * slide_distance;

        let is_top = config.position == crate::DockPosition::Top;
        let pill_x = (canvas_width as f32 - self.current_pill_width) * 0.5;
        let pill_y = if is_top {
            bottom_gap - hide_offset_y
        } else {
            canvas_height as f32 - bottom_gap - pill_height + hide_offset_y
        };

        let n = self.items.len();
        let content_w = current_content_width(&self.items, config);
        let mut cursor_x = (canvas_width as f32 - content_w) * 0.5;

        let baseline_y = if is_top {
            bottom_gap + config.pill_padding_y + 2.0 - hide_offset_y
        } else {
            canvas_height as f32 - bottom_gap - config.pill_padding_y - 2.0 + hide_offset_y
        };
        let dot_y = if is_top {
            bottom_gap + (config.pill_padding_y * 0.45).max(4.0) - hide_offset_y
        } else {
            canvas_height as f32 - bottom_gap - (config.pill_padding_y * 0.45).max(4.0)
                + hide_offset_y
        };
        let div_h = (config.icon_size * 0.72).max(20.0);
        let div_y = pill_y + (pill_height - div_h) * 0.5;

        let mut icons = Vec::with_capacity(n);
        let mut dots = Vec::with_capacity(n);
        let mut badges = Vec::new();
        let mut dividers = Vec::new();

        for (i, item) in self.items.iter().enumerate() {
            let size = item.current_size;
            let bounce_y = item.bounce.offset_y();

            let icon_y = if is_top {
                baseline_y + bounce_y
            } else {
                baseline_y - size - bounce_y
            };
            icons.push(IconSlot {
                x: cursor_x,
                y: icon_y,
                size,
                pressed: self.pressed_index == Some(i),
                opacity: item.icon_opacity,
            });

            if item.dot_opacity > 0.005 {
                dots.push(DotSlot {
                    x: cursor_x + size * 0.5,
                    y: dot_y,
                    radius: DOT_RADIUS,
                    opacity: item.dot_opacity,
                });
            }

            if item.badge_opacity > 0.005 && item.badge_count > 0 {
                let icon_y = if is_top { baseline_y + bounce_y } else { baseline_y - size - bounce_y };
                let badge_scale = (size / config.icon_size.max(1.0)).clamp(0.75, 2.0);
                badges.push(BadgeSlot {
                    x: cursor_x + size * 0.84,
                    y: icon_y + size * 0.14,
                    scale: badge_scale,
                    count: item.badge_count,
                    opacity: item.badge_opacity,
                });
            }

            cursor_x += size;

            if i + 1 < n {
                let next_sec = self.items[i + 1].section;
                let gap = gap_between(item.section, next_sec, config);
                if item.section != next_sec {
                    dividers.push(DividerSlot {
                        x: cursor_x + (gap - DIVIDER_WIDTH) * 0.5,
                        y: div_y,
                        width: DIVIDER_WIDTH,
                        height: div_h,
                    });
                }
                cursor_x += gap;
            }
        }

        let tooltip = if self.tooltip_opacity > 0.005 {
            self.tooltip_label.as_ref().map(|label| TooltipSlot {
                label: label.clone(),
                anchor_x: self.tooltip_anchor_x,
                anchor_y: self.tooltip_anchor_y,
                opacity: self.tooltip_opacity,
                point_up: config.position == crate::DockPosition::Top,
            })
        } else {
            None
        };

        let menu = self
            .menu
            .as_ref()
            .filter(|m| m.opacity > 0.005)
            .and_then(|m| self.compute_menu_layout(m, config));

        FrameLayout {
            canvas_width,
            canvas_height,
            pill: PillRect {
                x: pill_x,
                y: pill_y,
                width: self.current_pill_width,
                height: pill_height,
            },
            icons,
            dots,
            badges,
            dividers,
            tooltip,
            menu,
            hide_progress: self.hide_progress,
        }
    }

    fn compute_menu_layout(&self, menu: &MenuState, config: &DockConfig) -> Option<MenuLayout> {
        let (canvas_width, canvas_height) = self.canvas_size(config);
        let bottom_gap = config.bottom_margin.max(0) as f32;
        let pill_height = config.icon_size + config.pill_padding_y * 2.0;
        let hide_offset_y = self.hide_progress * (pill_height + bottom_gap + 6.0);

        let is_top = config.position == crate::DockPosition::Top;
        let (anchor_x, icon_edge_y) = match &menu.anchor {
            MenuAnchor::Entry(key) => {
                let idx = self.items.iter().position(|it| &it.key == key)?;
                let content_w = current_content_width(&self.items, config);
                let mut cursor_x = (canvas_width as f32 - content_w) * 0.5;
                for i in 0..idx {
                    cursor_x += self.items[i].current_size;
                    cursor_x +=
                        gap_between(self.items[i].section, self.items[i + 1].section, config);
                }
                let item = &self.items[idx];
                let edge_y = if is_top {
                    let baseline_y = bottom_gap + config.pill_padding_y + 2.0 - hide_offset_y;
                    baseline_y + item.current_size + item.bounce.offset_y()
                } else {
                    let baseline_y =
                        canvas_height as f32 - bottom_gap - config.pill_padding_y - 2.0 + hide_offset_y;
                    baseline_y - item.current_size - item.bounce.offset_y()
                };
                (cursor_x + item.current_size * 0.5, edge_y)
            }
            MenuAnchor::PointX(px) => {
                let pill_edge_y = if is_top {
                    bottom_gap + pill_height - hide_offset_y
                } else {
                    canvas_height as f32 - bottom_gap - pill_height + hide_offset_y
                };
                (*px, pill_edge_y)
            }
        };

        let (width, box_height, total_height) = measure_menu(&menu.items);
        let margin = 10.0;
        let max_x = (canvas_width as f32 - width - margin).max(margin);
        let x = (anchor_x - width * 0.5).clamp(margin, max_x);
        let max_y = (canvas_height as f32 - total_height - margin).max(margin);
        let y = if is_top {
            (icon_edge_y + MENU_GAP).clamp(margin, max_y)
        } else {
            (icon_edge_y - MENU_GAP - total_height).max(margin)
        };
        let arrow_x = anchor_x.clamp(x + 16.0, (x + width - 16.0).max(x + 16.0));
        let items_y = if is_top {
            y + crate::menu::MENU_ARROW_HEIGHT
        } else {
            y
        };
        let item_rects = layout_item_rects(x, items_y, width, &menu.items);

        Some(MenuLayout {
            x,
            y,
            width,
            height: total_height,
            box_height,
            arrow_x,
            point_up: is_top,
            items: menu.items.clone(),
            item_rects,
            hovered_item: menu.hovered_item,
            opacity: menu.opacity,
            anchor_y: icon_edge_y,
        })
    }

    fn icon_tooltip_anchor(&self, index: usize, config: &DockConfig) -> Option<(f32, f32)> {
        let item = self.items.get(index)?;
        let (canvas_width, canvas_height) = self.canvas_size(config);
        let bottom_gap = config.bottom_margin.max(0) as f32;
        let pill_height = config.icon_size + config.pill_padding_y * 2.0;
        let hide_offset_y = self.hide_progress * (pill_height + bottom_gap + 6.0);

        let content_w = current_content_width(&self.items, config);
        let mut x = (canvas_width as f32 - content_w) * 0.5;
        for i in 0..index {
            x += self.items[i].current_size;
            x += gap_between(self.items[i].section, self.items[i + 1].section, config);
        }

        let is_top = config.position == crate::DockPosition::Top;
        let baseline_y = if is_top {
            bottom_gap + config.pill_padding_y + 2.0 - hide_offset_y
        } else {
            canvas_height as f32 - bottom_gap - config.pill_padding_y - 2.0 + hide_offset_y
        };
        let anchor_x = x + item.current_size * 0.5;
        let anchor_y = if is_top {
            baseline_y + item.current_size + item.bounce.offset_y() + TOOLTIP_GAP
        } else {
            baseline_y - item.current_size - item.bounce.offset_y() - TOOLTIP_GAP
        };
        Some((anchor_x, anchor_y))
    }

    fn hit_test(&self, config: &DockConfig) -> Option<usize> {
        if self.hidden || self.hide_progress > 0.35 || self.is_menu_open() {
            return None;
        }
        self.icon_at_cursor(config)
    }
}

fn divider_gap(config: &DockConfig) -> f32 {
    (config.icon_spacing * 2.0 + 6.0).max(18.0)
}

fn gap_between(a: DockSection, b: DockSection, config: &DockConfig) -> f32 {
    if a != b {
        divider_gap(config)
    } else {
        config.icon_spacing
    }
}

fn current_content_width(items: &[ItemPhysics], config: &DockConfig) -> f32 {
    if items.is_empty() {
        return config.icon_size;
    }
    let mut total = 0.0;
    for (i, item) in items.iter().enumerate() {
        total += item.current_size;
        if i + 1 < items.len() {
            total += gap_between(item.section, items[i + 1].section, config);
        }
    }
    total
}

fn resting_pill_width(items: &[ItemPhysics], config: &DockConfig) -> f32 {
    if items.is_empty() {
        return config.pill_padding_x * 2.0 + config.icon_size;
    }
    let mut total = items.len() as f32 * config.icon_size;
    for i in 0..items.len().saturating_sub(1) {
        total += gap_between(items[i].section, items[i + 1].section, config);
    }
    total + config.pill_padding_x * 2.0
}
