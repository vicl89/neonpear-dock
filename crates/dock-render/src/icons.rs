use std::fs;
use std::path::Path;

use image::imageops::FilterType;

/// Imagen decodificada lista para subirse como textura RGBA8 con alfa premultiplicado.
pub struct DecodedIcon {
    pub width: u32,
    pub height: u32,
    pub rgba_premul: Vec<u8>,
}

/// Carga un icono desde disco (SVG o raster) al tamaño `target_px x target_px`.
/// Si la ruta no existe o falla al decodificarse, genera un icono de respaldo limpio.
pub fn load_icon_rgba(path: Option<&Path>, target_px: u32) -> DecodedIcon {
    let size = target_px.max(32);

    if let Some(p) = path {
        let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

        let decoded = if ext == "svg" || ext == "svgz" {
            decode_svg(p, size)
        } else {
            decode_raster(p, size)
        };

        if let Some(icon) = decoded {
            return icon;
        }
        tracing::warn!(?p, "No se pudo decodificar el icono; usando respaldo");
    }

    generate_fallback_icon(size)
}

fn decode_svg(path: &Path, target_px: u32) -> Option<DecodedIcon> {
    let bytes = fs::read(path).ok()?;
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(&bytes, &opt).ok()?;

    let src = tree.size();
    if src.width() <= 0.0 || src.height() <= 0.0 {
        return None;
    }

    let mut pixmap = resvg::tiny_skia::Pixmap::new(target_px, target_px)?;
    let scale = (target_px as f32 / src.width()).min(target_px as f32 / src.height());
    let tx = (target_px as f32 - src.width() * scale) * 0.5;
    let ty = (target_px as f32 - src.height() * scale) * 0.5;

    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale).post_translate(tx, ty);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // tiny-skia ya genera RGBA con alfa premultiplicado
    Some(DecodedIcon {
        width: target_px,
         height: target_px,
         rgba_premul: pixmap.take(),
    })
}

fn decode_raster(path: &Path, target_px: u32) -> Option<DecodedIcon> {
    let img = image::open(path).ok()?;
    let resized = img.resize(target_px, target_px, FilterType::Lanczos3).to_rgba8();

    let mut canvas = vec![0u8; (target_px * target_px * 4) as usize];
    let offset_x = (target_px.saturating_sub(resized.width())) / 2;
    let offset_y = (target_px.saturating_sub(resized.height())) / 2;

    for (x, y, px) in resized.enumerate_pixels() {
        let dst_x = x + offset_x;
        let dst_y = y + offset_y;
        if dst_x >= target_px || dst_y >= target_px {
            continue;
        }

        let a = px[3] as u32;
        let idx = ((dst_y * target_px + dst_x) * 4) as usize;
        canvas[idx] = ((px[0] as u32 * a + 127) / 255) as u8;
        canvas[idx + 1] = ((px[1] as u32 * a + 127) / 255) as u8;
        canvas[idx + 2] = ((px[2] as u32 * a + 127) / 255) as u8;
        canvas[idx + 3] = px[3];
    }

    Some(DecodedIcon {
        width: target_px,
         height: target_px,
         rgba_premul: canvas,
    })
}

/// Genera un icono cuadrado redondeado genérico en memoria si falta el archivo.
fn generate_fallback_icon(size: u32) -> DecodedIcon {
    let mut data = vec![0u8; (size * size * 4) as usize];
    let half = size as f32 * 0.5;
    let margin = size as f32 * 0.08;
    let box_half = half - margin;
    let radius = size as f32 * 0.22;

    for y in 0..size {
        for x in 0..size {
            let px = (x as f32 + 0.5) - half;
            let py = (y as f32 + 0.5) - half;

            let qx = px.abs() - box_half + radius;
            let qy = py.abs() - box_half + radius;
            let dist = qx.max(qy).min(0.0) + (qx.max(0.0).hypot(qy.max(0.0))) - radius;

            let alpha = (0.5 - dist).clamp(0.0, 1.0);
            if alpha <= 0.0 {
                continue;
            }

            let t = y as f32 / size as f32;
            let r = (120.0 * (1.0 - t) + 75.0 * t) * alpha;
            let g = (145.0 * (1.0 - t) + 95.0 * t) * alpha;
            let b = (195.0 * (1.0 - t) + 145.0 * t) * alpha;

            let idx = ((y * size + x) * 4) as usize;
            data[idx] = r.round() as u8;
            data[idx + 1] = g.round() as u8;
            data[idx + 2] = b.round() as u8;
            data[idx + 3] = (alpha * 255.0).round() as u8;
        }
    }

    DecodedIcon {
        width: size,
        height: size,
        rgba_premul: data,
    }
}
