use dock_core::menu::MenuLayout;
use dock_core::physics::PillRect;
use dock_core::BlurMode;
use smithay_client_toolkit::compositor::{CompositorState, Region};
use wayland_client::{
    globals::GlobalList,
    protocol::wl_surface::WlSurface,
    Dispatch, QueueHandle,
};
use wayland_protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1,
    ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
};
use wayland_protocols_plasma::blur::client::{
    org_kde_kwin_blur::OrgKdeKwinBlur,
    org_kde_kwin_blur_manager::OrgKdeKwinBlurManager,
};

enum BlurBackend {
    ExtEffect {
        _manager: ExtBackgroundEffectManagerV1,
        surface: ExtBackgroundEffectSurfaceV1,
    },
    KwinBlur {
        _manager: OrgKdeKwinBlurManager,
        blur: OrgKdeKwinBlur,
    },
    External,
    Disabled,
}

type BlurKey = ((i32, i32, i32, i32, i32, i32), Option<(i32, i32, i32, i32)>, Option<(i32, i32, i32, i32, i32)>);

pub struct BlurController {
    backend: BlurBackend,
    last_key: Option<BlurKey>,
}

impl BlurController {
    pub fn new<D>(
        mode: BlurMode,
        namespace: &str,
        globals: &GlobalList,
        surface: &WlSurface,
        qh: &QueueHandle<D>,
    ) -> Self
    where
        D: Dispatch<ExtBackgroundEffectManagerV1, ()>
            + Dispatch<ExtBackgroundEffectSurfaceV1, ()>
            + Dispatch<OrgKdeKwinBlurManager, ()>
            + Dispatch<OrgKdeKwinBlur, ()>
            + 'static,
    {
        let backend = match mode {
            BlurMode::None => {
                tracing::info!("Modo blur: desactivado por configuración");
                BlurBackend::Disabled
            }
            BlurMode::External => {
                tracing::info!(%namespace, "Modo blur: delegado al compositor externo");
                BlurBackend::External
            }
            BlurMode::Auto | BlurMode::Kwin => {
                if let Ok(mgr) = globals.bind::<ExtBackgroundEffectManagerV1, _, _>(qh, 1..=1, ()) {
                    let effect = mgr.get_background_effect(surface, qh, ());
                    tracing::info!("Modo blur: ext_background_effect_manager_v1 activo");
                    BlurBackend::ExtEffect {
                        _manager: mgr,
                        surface: effect,
                    }
                } else if let Ok(mgr) = globals.bind::<OrgKdeKwinBlurManager, _, _>(qh, 1..=1, ()) {
                    let blur = mgr.create(surface, qh, ());
                    tracing::info!("Modo blur: org_kde_kwin_blur_manager activo");
                    BlurBackend::KwinBlur {
                        _manager: mgr,
                        blur,
                    }
                } else {
                    tracing::warn!("Sin protocolo de blur disponible; usando fondo sólido translúcido");
                    BlurBackend::Disabled
                }
            }
        };

        Self {
            backend,
            last_key: None,
        }
    }

    pub fn is_glass_active(&self) -> bool {
        !matches!(self.backend, BlurBackend::Disabled)
    }

    /// Actualiza la región de desenfoque redondeada de la píldora y, si hay un menú visible,
    /// también añade la tarjeta del menú contextual.
    pub fn update_regions(
        &mut self,
        compositor: &CompositorState,
        pill: &PillRect,
        corner_radius: f32,
        menu: Option<&MenuLayout>,
        stack_rect: Option<(f32, f32, f32, f32, f32)>,
        canvas_height: u32,
    ) {
        if matches!(self.backend, BlurBackend::External | BlurBackend::Disabled) {
            return;
        }

        let px = pill.x.round() as i32;
        let py = pill.y.round() as i32;
        let pw = pill.width.round() as i32;
        let ph = pill.height.round() as i32;
        let pr = corner_radius.round().max(0.0) as i32;
        let ch = canvas_height as i32;

        let menu_rect = menu.filter(|m| m.opacity > 0.35).map(|m| {
            let lift = (1.0 - m.opacity) * 5.0;
            (
                m.x.round() as i32,
                (m.y + lift).round() as i32,
                m.width.round() as i32,
                m.box_height.round() as i32,
            )
        });

        let stack_rect_i = stack_rect.map(|(sx, sy, sw, sh, sr)| {
            (
                sx.round() as i32,
                sy.round() as i32,
                sw.round() as i32,
                sh.round() as i32,
                sr.round().max(0.0) as i32,
            )
        });
        let key: BlurKey = ((px, py, pw, ph, pr, ch), menu_rect, stack_rect_i);
        if self.last_key == Some(key) {
            return;
        }
        self.last_key = Some(key);

        let Ok(region) = Region::new(compositor) else {
            return;
        };

        let mut added = add_rounded_rect(&region, px, py, pw, ph, pr, ch);
        if let Some((mx, my, mw, mh)) = menu_rect {
            added |= add_rounded_rect(&region, mx, my, mw, mh, 12, ch);
        }
        if let Some((sx, sy, sw, sh, sr)) = stack_rect_i {
            added |= add_rounded_rect(&region, sx, sy, sw, sh, sr, ch);
        }

        if !added {
            self.clear_blur(compositor);
            return;
        }

        match &self.backend {
            BlurBackend::ExtEffect { surface, .. } => {
                surface.set_blur_region(Some(region.wl_region()));
            }
            BlurBackend::KwinBlur { blur, .. } => {
                blur.set_region(Some(region.wl_region()));
                blur.commit();
            }
            BlurBackend::External | BlurBackend::Disabled => {}
        }
    }

    fn clear_blur(&self, compositor: &CompositorState) {
        match &self.backend {
            BlurBackend::ExtEffect { surface, .. } => {
                surface.set_blur_region(None);
            }
            BlurBackend::KwinBlur { blur, .. } => {
                if let Ok(empty) = Region::new(compositor) {
                    blur.set_region(Some(empty.wl_region()));
                    blur.commit();
                }
            }
            BlurBackend::External | BlurBackend::Disabled => {}
        }
    }
}

fn add_rounded_rect(
    region: &Region,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    radius: i32,
    canvas_height: i32,
) -> bool {
    if w <= 0 || h <= 0 || y >= canvas_height || y + h <= 0 {
        return false;
    }

    let r = radius.max(0).min(w / 2).min(h / 2);
    let rf = r as f32;
    let mut added = false;

    let mut add_clipped = |rx: i32, ry: i32, rw: i32, rh: i32| {
        let top = ry.max(0);
        let bottom = (ry + rh).min(canvas_height);
        if bottom > top && rw > 0 {
            region.add(rx, top, rw, bottom - top);
            added = true;
        }
    };

    for dy in 0..r {
        let cy = rf - (dy as f32 + 0.5);
        let dx = (rf - (rf * rf - cy * cy).max(0.0).sqrt()).ceil() as i32;
        add_clipped(x + dx, y + dy, w - 2 * dx, 1);
    }

    add_clipped(x, y + r, w, h - 2 * r);

    for dy in 0..r {
        let cy = dy as f32 + 0.5;
        let dx = (rf - (rf * rf - cy * cy).max(0.0).sqrt()).ceil() as i32;
        add_clipped(x + dx, y + h - r + dy, w - 2 * dx, 1);
    }

    added
}
