use std::path::Path;
use std::time::Instant;

use dock_render::icons::{load_icon_rgba, DecodedIcon};
use smithay_client_toolkit::{
    compositor::{CompositorState, Region},
    shell::{
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerSurface},
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm},
};
use wayland_client::{
    protocol::{wl_output, wl_shm, wl_surface},
    Proxy, QueueHandle,
};

use super::DockApp;

const FADE_DURATION_SECS: f32 = 0.22;

pub struct DragGhost {
    layer_surface: LayerSurface,
    pool: SlotPool,
    icon: DecodedIcon,
    logical_size: u32,
    configured: bool,
    fade_start: Option<Instant>,
}

impl DragGhost {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        compositor_state: &CompositorState,
        layer_shell: &LayerShell,
        shm: &Shm,
        qh: &QueueHandle<DockApp>,
        output: Option<&wl_output::WlOutput>,
        icon_path: Option<&Path>,
        logical_size: u32,
        scale_factor: u32,
        out_x: f32,
        out_y: f32,
    ) -> Option<Self> {
        let logical_size = logical_size.max(32);
        let scale = scale_factor.max(1);
        let phys_size = logical_size * scale;

        let icon = load_icon_rgba(icon_path, phys_size);
        let pool_bytes = (icon.width as usize)
            .saturating_mul(icon.height as usize)
            .saturating_mul(4)
            .saturating_mul(2)
            .max(4096);
        let pool = SlotPool::new(pool_bytes, shm).ok()?;

        let surface = compositor_state.create_surface(qh);
        surface.set_buffer_scale(scale as i32);
        if let Ok(empty_region) = Region::new(compositor_state) {
            surface.set_input_region(Some(empty_region.wl_region()));
        }

        let layer_surface = layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Overlay,
            Some("neonpear-dock-drag-ghost".to_string()),
            output,
        );
        layer_surface.set_anchor(Anchor::TOP | Anchor::LEFT);
        layer_surface.set_size(logical_size, logical_size);
        layer_surface.set_exclusive_zone(-1);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);

        let left = (out_x - logical_size as f32 * 0.5).round() as i32;
        let top = (out_y - logical_size as f32 * 0.5).round() as i32;
        layer_surface.set_margin(top, 0, 0, left);
        layer_surface.commit();

        Some(Self {
            layer_surface,
            pool,
            icon,
            logical_size,
            configured: false,
            fade_start: None,
        })
    }

    pub fn is_surface(&self, surface: &wl_surface::WlSurface) -> bool {
        self.layer_surface.wl_surface().id() == surface.id()
    }

    pub fn is_fading(&self) -> bool {
        self.fade_start.is_some()
    }

    pub fn move_to(&mut self, out_x: f32, out_y: f32) {
        if self.fade_start.is_some() {
            return;
        }
        let left = (out_x - self.logical_size as f32 * 0.5).round() as i32;
        let top = (out_y - self.logical_size as f32 * 0.5).round() as i32;
        self.layer_surface.set_margin(top, 0, 0, left);
        if self.configured {
            self.layer_surface.wl_surface().commit();
        }
    }

    pub fn on_configure(&mut self, qh: &QueueHandle<DockApp>) {
        self.configured = true;
        self.draw_current(qh);
    }

    pub fn start_fade(&mut self, qh: &QueueHandle<DockApp>) {
        self.fade_start = Some(Instant::now());
        if self.configured {
            self.draw_current(qh);
        }
    }

    /// Devuelve `true` mientras la animación siga activa; `false` cuando termina.
    pub fn tick_fade(&mut self, qh: &QueueHandle<DockApp>) -> bool {
        let Some(start) = self.fade_start else {
            return true;
        };
        if start.elapsed().as_secs_f32() >= FADE_DURATION_SECS {
            return false;
        }
        self.draw_current(qh);
        true
    }

    fn draw_current(&mut self, qh: &QueueHandle<DockApp>) {
        let w = self.icon.width;
        let h = self.icon.height;
        if w == 0 || h == 0 {
            return;
        }

        let (opacity, keep_ticking) = match self.fade_start {
            Some(start) => {
                let t = (start.elapsed().as_secs_f32() / FADE_DURATION_SECS).clamp(0.0, 1.0);
                let ease = (1.0 - t) * (1.0 - t);
                (ease, t < 1.0)
            }
            None => (0.94, false),
        };

        let Ok((buffer, canvas)) = self.pool.create_buffer(
            w as i32,
            h as i32,
            (w * 4) as i32,
            wl_shm::Format::Argb8888,
        ) else {
            return;
        };

        for (src, dst) in self
            .icon
            .rgba_premul
            .chunks_exact(4)
            .zip(canvas.chunks_exact_mut(4))
        {
            let r = (src[0] as f32 * opacity).round().clamp(0.0, 255.0) as u8;
            let g = (src[1] as f32 * opacity).round().clamp(0.0, 255.0) as u8;
            let b = (src[2] as f32 * opacity).round().clamp(0.0, 255.0) as u8;
            let a = (src[3] as f32 * opacity).round().clamp(0.0, 255.0) as u8;
            dst[0] = b;
            dst[1] = g;
            dst[2] = r;
            dst[3] = a;
        }

        let wl_surf = self.layer_surface.wl_surface();
        if buffer.attach_to(wl_surf).is_err() {
            return;
        }
        wl_surf.damage_buffer(0, 0, w as i32, h as i32);
        if keep_ticking {
            wl_surf.frame(qh, wl_surf.clone());
        }
        wl_surf.commit();
    }
}
