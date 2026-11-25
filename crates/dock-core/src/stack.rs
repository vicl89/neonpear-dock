use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Elemento individual dentro de un Stack desplegado en abanico.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackEntryItem {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_image: bool,
    pub icon_path: Option<String>,
}

/// Datos completos de un Stack listo para calcular su geometría de abanico.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FanStackData {
    pub folder_path: String,
    pub folder_name: String,
    pub items: Vec<StackEntryItem>,
    pub total_count: usize,
    pub remaining_count: usize,
}

/// Geometría calculada de un ítem del abanico en coordenadas de superficie (lógicas).
#[derive(Debug, Clone, Copy)]
pub struct FanSlotGeometry {
    pub icon_x: f32,
    pub icon_y: f32,
    pub icon_size: f32,
    pub angle_rad: f32,
    pub label_x: f32,
    pub label_y: f32,
    pub label_w: f32,
    pub label_h: f32,
    pub label_on_left: bool,
}

/// Geometría completa del abanico desplegado.
#[derive(Debug, Clone)]
pub struct FanLayout {
    pub slots: Vec<FanSlotGeometry>,
    pub more_button_center: Option<[f32; 2]>,
    pub more_button_radius: f32,
    pub more_label_rect: Option<[f32; 4]>, // [x, y, w, h]
    pub more_label_text: Option<String>,
    pub more_angle_rad: f32,
}

const MAX_VISIBLE_ITEMS: usize = 8;

const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "bmp", "gif", "qoi", "tga", "tiff", "tif",
];

fn is_image_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| IMAGE_EXTS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn candidate_icon_names(path: &Path, is_dir: bool) -> &'static [&'static str] {
    if is_dir {
        return &["folder", "inode-directory", "folder-blue"];
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "xlsx" | "xls" | "ods" | "csv" => &[
            "application-vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            "application-vnd.ms-excel",
            "x-office-spreadsheet",
            "libreoffice-calc",
        ],
        "docx" | "doc" | "odt" | "rtf" => &[
            "application-vnd.openxmlformats-officedocument.wordprocessingml.document",
            "application-msword",
            "x-office-document",
            "libreoffice-writer",
        ],
        "pptx" | "ppt" | "odp" => &[
            "application-vnd.openxmlformats-officedocument.presentationml.presentation",
            "application-vnd.ms-powerpoint",
            "x-office-presentation",
            "libreoffice-impress",
        ],
        "pdf" => &["application-pdf", "x-office-document"],
        "zip" | "tar" | "gz" | "xz" | "7z" | "rar" | "rpm" => &[
            "application-x-archive",
            "application-zip",
            "package-x-generic",
        ],
        "mp3" | "flac" | "wav" | "ogg" | "m4a" => &["audio-x-generic"],
        "mp4" | "mkv" | "webm" | "avi" | "mov" => &["video-x-generic"],
        "txt" | "md" | "rs" | "py" | "json" | "toml" | "sh" | "c" | "cpp" | "js" | "ts"
        | "html" | "css" => &["text-x-generic", "text-plain"],
        _ => &["text-x-generic", "application-octet-stream", "unknown"],
    }
}

fn icon_theme_roots() -> Vec<PathBuf> {
    let mut themes = Vec::new();
    if let Some(active) = crate::desktop::detect_kde_icon_theme() {
        themes.push(active);
    }
    for fallback in ["breeze-dark", "breeze", "Adwaita", "hicolor"] {
        if !themes.iter().any(|t| t == fallback) {
            themes.push(fallback.to_string());
        }
    }

    let mut base_dirs = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        let h = PathBuf::from(home);
        base_dirs.push(h.join(".local/share/icons"));
        base_dirs.push(h.join(".icons"));
    }
    base_dirs.push(PathBuf::from("/usr/share/icons"));

    let mut roots = Vec::new();
    for theme in &themes {
        for base in &base_dirs {
            let dir = base.join(theme);
            if dir.is_dir() {
                roots.push(dir);
            }
        }
    }
    roots
}

fn resolve_entry_icon(roots: &[PathBuf], path: &Path, is_dir: bool) -> Option<String> {
    let names = candidate_icon_names(path, is_dir);
    let subdirs: &[&str] = if is_dir {
        &[
            "places/scalable",
            "places/64",
            "places/48",
            "places/32",
            "64x64/places",
            "48x48/places",
            "scalable/places",
        ]
    } else {
        &[
            "mimetypes/scalable",
            "mimetypes/64",
            "mimetypes/48",
            "mimetypes/32",
            "64x64/mimetypes",
            "48x48/mimetypes",
            "scalable/mimetypes",
            "apps/scalable",
            "48x48/apps",
        ]
    };

    for root in roots {
        for name in names {
            for sub in subdirs {
                for ext in ["svg", "png"] {
                    let candidate = root.join(sub).join(format!("{name}.{ext}"));
                    if candidate.is_file() {
                        return Some(candidate.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
    None
}

/// Lee el contenido de una carpeta y prepara los datos del Stack en abanico.
pub fn build_fan_stack_data(folder_path: &str, more_fmt: &str) -> FanStackData {
    let p = Path::new(folder_path);
    let folder_name = p
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(folder_path)
        .to_string();

    let roots = icon_theme_roots();
    let mut raw: Vec<(SystemTime, StackEntryItem)> = Vec::new();
    if let Ok(rd) = fs::read_dir(p) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let full_path = entry.path();
            let meta = entry.metadata().ok();
            let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let modified = meta
                .and_then(|m| m.modified().ok())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            let is_image = !is_dir && is_image_file(&full_path);

            raw.push((
                modified,
                StackEntryItem {
                    name,
                    path: full_path.to_string_lossy().to_string(),
                    is_dir,
                    is_image,
                    icon_path: None,
                },
            ));
        }
    }

    raw.sort_by(|a, b| b.0.cmp(&a.0));
    let total_count = raw.len();
    let items: Vec<StackEntryItem> = raw
        .into_iter()
        .take(MAX_VISIBLE_ITEMS)
        .map(|(_, mut item)| {
            if item.is_image {
                item.icon_path = Some(item.path.clone());
            } else {
                item.icon_path = resolve_entry_icon(&roots, Path::new(&item.path), item.is_dir);
            }
            item
        })
        .collect();
    let remaining_count = total_count.saturating_sub(items.len());
    let _ = more_fmt;

    FanStackData {
        folder_path: folder_path.to_string(),
        folder_name,
        items,
        total_count,
        remaining_count,
    }
}

fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

fn truncate_label(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        text.to_string()
    } else {
        let mut s: String = text.chars().take(max_chars.saturating_sub(1)).collect();
        s.push('…');
        s
    }
}

/// Calcula un arco circular suave (estilo macOS Fan Stack) adaptado a las 4 posiciones del dock
/// y al tamaño dinámico `icon_size`, interpolado con `progress` (0.0 = cerrado, 1.0 = abierto).
#[allow(clippy::too_many_arguments)]
pub fn compute_fan_layout(
    data: &FanStackData,
    anchor_x: f32,
    anchor_y: f32,
    canvas_w: f32,
    canvas_h: f32,
    icon_size: f32,
    position: crate::DockPosition,
    progress: f32,
) -> FanLayout {
    let n = data.items.len();
    let mut slots = Vec::with_capacity(n);

    let p = ease_out_cubic(progress);
    let icon_size = icon_size.clamp(28.0, 96.0);
    let label_h = (icon_size * 0.48).clamp(24.0, 30.0);
    let char_w = (label_h * 0.30).clamp(6.8, 8.6);

    // En Left/Right el abanico también se despliega verticalmente (hacia arriba, o hacia abajo si está muy arriba)
    // a un costado del dock para que las etiquetas horizontales nunca se empalmen.
    let fan_up = match position {
        crate::DockPosition::Bottom => true,
        crate::DockPosition::Top => false,
        crate::DockPosition::Left | crate::DockPosition::Right => anchor_y >= canvas_h * 0.42,
    };

    let available_span = if fan_up {
        (anchor_y - 28.0).max(160.0)
    } else {
        (canvas_h - anchor_y - 28.0).max(160.0)
    };
    let desired_step = icon_size + 10.0;
    let max_fit_step = if n > 0 {
        available_span / (n as f32 + 1.15)
    } else {
        desired_step
    };
    let step_px = desired_step.min(max_fit_step).max(icon_size * 0.72);

    let radius = (step_px * 36.0).max(1750.0);
    let step_rad = step_px / radius;

    let label_on_left = match position {
        crate::DockPosition::Bottom | crate::DockPosition::Top => anchor_x > canvas_w * 0.35,
        crate::DockPosition::Left => false,
        crate::DockPosition::Right => true,
    };
    let curve_right = match position {
        crate::DockPosition::Bottom | crate::DockPosition::Top => {
            anchor_x < canvas_w - icon_size - 48.0
        }
        crate::DockPosition::Left => true,
        crate::DockPosition::Right => false,
    };

    let base_x = match position {
        crate::DockPosition::Right => (anchor_x - icon_size).max(12.0),
        _ => anchor_x,
    };

    for (idx, item) in data.items.iter().enumerate() {
        let theta = (idx as f32 + 0.35) * step_rad * p;
        let along = radius * theta.sin();
        let perp = radius * (1.0 - theta.cos());

        let target_ix = if curve_right { base_x + perp } else { base_x - perp };
        let target_iy = if fan_up {
            anchor_y - icon_size - along
        } else {
            anchor_y + along
        };
        let angle_rad = match (fan_up, curve_right) {
            (true, true) => theta * 0.72,
            (true, false) => -(theta * 0.72),
            (false, true) => -(theta * 0.72),
            (false, false) => theta * 0.72,
        };

        let icon_x = target_ix.clamp(12.0, (canvas_w - icon_size - 12.0).max(12.0));
        let icon_y = target_iy.clamp(12.0, (canvas_h - icon_size - 12.0).max(12.0));

        let display_name = truncate_label(&item.name, 34);
        let text_len = display_name.chars().count() as f32;
        let label_w = (text_len * char_w + 24.0).clamp(76.0, 290.0);

        let label_y = icon_y + (icon_size - label_h) * 0.5;
        let label_x = if label_on_left {
            (icon_x - label_w - 14.0).max(10.0)
        } else {
            (icon_x + icon_size + 14.0).min((canvas_w - label_w - 10.0).max(10.0))
        };

        slots.push(FanSlotGeometry {
            icon_x,
            icon_y,
            icon_size,
            angle_rad,
            label_x,
            label_y,
            label_w,
            label_h,
            label_on_left,
        });
    }

    let (more_button_center, more_button_radius, more_label_rect, more_label_text, more_angle_rad) =
        if n > 0 {
            let top_theta = (n as f32 + 0.35) * step_rad * p;
            let along = radius * top_theta.sin();
            let perp = radius * (1.0 - top_theta.cos());
            let btn_r = (icon_size * 0.38).clamp(15.0, 24.0);

            let cx = (if curve_right { base_x + perp } else { base_x - perp } + icon_size * 0.5)
                .clamp(btn_r + 8.0, (canvas_w - btn_r - 8.0).max(btn_r + 8.0));
            let cy = (if fan_up {
                anchor_y - icon_size * 0.5 - along
            } else {
                anchor_y + icon_size * 0.5 + along
            })
            .clamp(btn_r + 8.0, (canvas_h - btn_r - 8.0).max(btn_r + 8.0));

            let mang = match (fan_up, curve_right) {
                (true, true) => top_theta * 0.72,
                (true, false) => -(top_theta * 0.72),
                (false, true) => -(top_theta * 0.72),
                (false, false) => top_theta * 0.72,
            };

            let raw_label = format!("{} items in {}", data.total_count, data.folder_name);
            let label = truncate_label(&raw_label, 34);
            let lw = (label.chars().count() as f32 * char_w + 24.0).clamp(96.0, 290.0);
            let lh = label_h;
            let ly = cy - lh * 0.5;
            let lx = if label_on_left {
                (cx - btn_r - lw - 14.0).max(10.0)
            } else {
                (cx + btn_r + 14.0).min((canvas_w - lw - 10.0).max(10.0))
            };

            (Some([cx, cy]), btn_r, Some([lx, ly, lw, lh]), Some(label), mang)
        } else {
            (None, 16.0, None, None, 0.0)
        };

    FanLayout {
        slots,
        more_button_center,
        more_button_radius,
        more_label_rect,
        more_label_text,
        more_angle_rad,
    }
}
