use crate::desktop::{DesktopApp, DockEntry, DockEntryKind, DockSection};

pub const MENU_MIN_WIDTH: f32 = 178.0;
pub const MENU_MAX_WIDTH: f32 = 250.0;
pub const MENU_PAD_X: f32 = 6.0;
pub const MENU_PAD_Y: f32 = 6.0;
pub const MENU_ITEM_HEIGHT: f32 = 30.0;
pub const MENU_SEPARATOR_HEIGHT: f32 = 9.0;
pub const MENU_ARROW_HEIGHT: f32 = 6.0;
pub const MENU_GAP: f32 = 8.0;

#[derive(Debug, Clone)]
pub struct OpenWindowItem {
    pub id: u64,
    pub title: String,
    pub active: bool,
}

fn truncate_title(title: &str, max_chars: usize) -> String {
    let trimmed = title.trim();
    if trimmed.chars().count() <= max_chars {
        trimmed.to_string()
    } else {
        let mut s: String = trimmed.chars().take(max_chars.saturating_sub(1)).collect();
        s.push('…');
        s
    }
}

#[derive(Debug, Clone)]
pub enum ContextMenuAction {
    LaunchApp(DesktopApp),
    ActivateWindow(u64),
    OpenSettings,
    RunDesktopAction {
        app_id: String,
        label: String,
        exec: String,
    },
    PinApp(DesktopApp),
    UnpinApp(String),
    RemoveRecent(String),
    CloseApp(DesktopApp),
    ForceQuitApp(DesktopApp),
    OpenTrash,
    EmptyTrash,
    ToggleAutoHide,
    ToggleMagnification,
    ToggleRecentApps,
    ToggleActiveAppsOnly,
    ToggleLaunchpad,
    OpenLaunchpad,
    AddSpacer,
    RemoveSpacer(String),
}

#[derive(Debug, Clone)]
pub struct ContextMenuItem {
    pub label: String,
    pub enabled: bool,
    pub destructive: bool,
    pub separator_before: bool,
    pub action: ContextMenuAction,
}

#[derive(Debug, Clone)]
pub struct MenuLayout {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    /// Alto total incluyendo la flecha inferior.
    pub height: f32,
    /// Alto únicamente de la caja redondeada (sin la flecha).
    pub box_height: f32,
    /// Posición X en coordenadas lógicas donde apunta la flecha inferior.
    pub arrow_x: f32,
    pub point_up: bool,
    pub items: Vec<ContextMenuItem>,
    /// Rectángulo `[x, y, w, h]` de cada ítem en coordenadas lógicas del lienzo.
    pub item_rects: Vec<[f32; 4]>,
    pub hovered_item: Option<usize>,
    pub opacity: f32,
    /// Coordenada Y superior del icono origen para mantener la región de entrada continua.
    pub anchor_y: f32,
}

/// Construye las opciones del menú contextual según el tipo de entrada y si tiene ventanas abiertas.
pub fn build_menu_items(
    entry: &DockEntry,
    is_running: bool,
    open_windows: &[OpenWindowItem],
) -> Vec<ContextMenuItem> {
    let mut items = Vec::new();

    match &entry.kind {
        DockEntryKind::App(app) => {
            if !open_windows.is_empty() {
                for win in open_windows {
                    let base = if win.title.trim().is_empty() {
                        app.name.as_str()
                    } else {
                        win.title.as_str()
                    };
                    let title = truncate_title(base, 32);
                    let label = if win.active {
                        format!("• {title}")
                    } else {
                        title
                    };
                    items.push(ContextMenuItem {
                        label,
                        enabled: true,
                        destructive: false,
                        separator_before: false,
                        action: ContextMenuAction::ActivateWindow(win.id),
                    });
                }
            }

            let can_launch = !app.exec.trim().is_empty();

            if can_launch {
                items.push(ContextMenuItem {
                    label: if is_running {
                        "Nueva ventana".to_string()
                    } else {
                        "Abrir".to_string()
                    },
                    enabled: true,
                    destructive: false,
                    separator_before: !items.is_empty(),
                    action: ContextMenuAction::LaunchApp(app.clone()),
                });
            }

            if !app.actions.is_empty() {
                for (i, act) in app.actions.iter().enumerate() {
                    items.push(ContextMenuItem {
                        label: act.name.clone(),
                        enabled: true,
                        destructive: false,
                        separator_before: i == 0 && !items.is_empty(),
                        action: ContextMenuAction::RunDesktopAction {
                            app_id: app.id.clone(),
                            label: format!("{} - {}", app.name, act.name),
                            exec: act.exec.clone(),
                        },
                    });
                }
            }

            let sep_before_pin = !app.actions.is_empty();
            match entry.section {
                DockSection::Pinned => {
                    items.push(ContextMenuItem {
                        label: "Quitar del dock".to_string(),
                        enabled: true,
                        destructive: false,
                        separator_before: sep_before_pin,
                        action: ContextMenuAction::UnpinApp(app.id.clone()),
                    });
                }
                DockSection::Dynamic => {
                    if can_launch {
                        items.push(ContextMenuItem {
                            label: "Fijar en el dock".to_string(),
                            enabled: true,
                            destructive: false,
                            separator_before: sep_before_pin,
                            action: ContextMenuAction::PinApp(app.clone()),
                        });
                    }
                    if !is_running {
                        items.push(ContextMenuItem {
                            label: "Quitar de recientes".to_string(),
                            enabled: true,
                            destructive: false,
                            separator_before: false,
                            action: ContextMenuAction::RemoveRecent(app.id.clone()),
                        });
                    }
                }
                DockSection::Utility => {}
            }

            if is_running {
                items.push(ContextMenuItem {
                    label: "Cerrar".to_string(),
                    enabled: true,
                    destructive: true,
                    separator_before: !items.is_empty(),
                    action: ContextMenuAction::CloseApp(app.clone()),
                });
                items.push(ContextMenuItem {
                    label: "Forzar salida".to_string(),
                    enabled: true,
                    destructive: true,
                    separator_before: false,
                    action: ContextMenuAction::ForceQuitApp(app.clone()),
                });
            }
        }
        DockEntryKind::Spacer { id } => {
            items.push(ContextMenuItem {
                label: "Quitar espaciador".to_string(),
                enabled: true,
                destructive: true,
                separator_before: false,
                action: ContextMenuAction::RemoveSpacer(id.clone()),
            });
        }
        DockEntryKind::Launchpad => {
            items.push(ContextMenuItem {
                label: "Abrir lanzador de aplicaciones".to_string(),
                enabled: true,
                destructive: false,
                separator_before: false,
                action: ContextMenuAction::OpenLaunchpad,
            });
            items.push(ContextMenuItem {
                label: "Ocultar botón Launchpad".to_string(),
                enabled: true,
                destructive: false,
                separator_before: true,
                action: ContextMenuAction::ToggleLaunchpad,
            });
        }
        DockEntryKind::Trash { full } => {
            items.push(ContextMenuItem {
                label: "Abrir Papelera".to_string(),
                enabled: true,
                destructive: false,
                separator_before: false,
                action: ContextMenuAction::OpenTrash,
            });
            items.push(ContextMenuItem {
                label: "Vaciar Papelera".to_string(),
                enabled: *full,
                destructive: true,
                separator_before: true,
                action: ContextMenuAction::EmptyTrash,
            });
        }
    }

    items.push(ContextMenuItem {
        label: "Configuración de NeonPear Dock…".to_string(),
        enabled: true,
        destructive: false,
        separator_before: !items.is_empty(),
        action: ContextMenuAction::OpenSettings,
    });

    items
}

/// Calcula dimensiones `(ancho, alto_caja, alto_total)` del menú en píxeles lógicos.
pub fn measure_menu(items: &[ContextMenuItem]) -> (f32, f32, f32) {
    let max_chars = items
        .iter()
        .map(|it| it.label.chars().count())
        .max()
        .unwrap_or(10) as f32;
    let width = (max_chars * 7.8 + 38.0).clamp(MENU_MIN_WIDTH, MENU_MAX_WIDTH);

    let mut content_h = MENU_PAD_Y * 2.0;
    for (i, item) in items.iter().enumerate() {
        if i > 0 && item.separator_before {
            content_h += MENU_SEPARATOR_HEIGHT;
        }
        content_h += MENU_ITEM_HEIGHT;
    }

    let box_height = content_h.ceil();
    let total_height = box_height + MENU_ARROW_HEIGHT;
    (width.ceil(), box_height, total_height)
}

/// Calcula los rectángulos lógicos de cada ítem dentro de un menú situado en `(x, y)`.
pub fn layout_item_rects(x: f32, y: f32, width: f32, items: &[ContextMenuItem]) -> Vec<[f32; 4]> {
    let mut rects = Vec::with_capacity(items.len());
    let mut cy = y + MENU_PAD_Y;
    let item_w = (width - MENU_PAD_X * 2.0).max(40.0);

    for (i, item) in items.iter().enumerate() {
        if i > 0 && item.separator_before {
            cy += MENU_SEPARATOR_HEIGHT;
        }
        rects.push([x + MENU_PAD_X, cy, item_w, MENU_ITEM_HEIGHT]);
        cy += MENU_ITEM_HEIGHT;
    }

    rects
}


/// Construye el menú rápido del dock al hacer clic derecho sobre un separador o espacio vacío.
pub fn build_dock_quick_menu(config: &crate::DockConfig) -> Vec<ContextMenuItem> {
    let auto_hide_on = config.hide_mode != crate::HideMode::None;
    vec![
        ContextMenuItem {
            label: if auto_hide_on {
                "Desactivar ocultamiento automático".to_string()
            } else {
                "Activar ocultamiento automático".to_string()
            },
            enabled: true,
            destructive: false,
            separator_before: false,
            action: ContextMenuAction::ToggleAutoHide,
        },
        ContextMenuItem {
            label: if config.magnification_enabled {
                "Desactivar ampliación".to_string()
            } else {
                "Activar ampliación".to_string()
            },
            enabled: true,
            destructive: false,
            separator_before: false,
            action: ContextMenuAction::ToggleMagnification,
        },
        ContextMenuItem {
            label: if config.show_recent_apps {
                "Ocultar aplicaciones recientes".to_string()
            } else {
                "Mostrar aplicaciones recientes".to_string()
            },
            enabled: true,
            destructive: false,
            separator_before: false,
            action: ContextMenuAction::ToggleRecentApps,
        },
        ContextMenuItem {
            label: if config.active_apps_only {
                "Mostrar todas las aplicaciones".to_string()
            } else {
                "Solo aplicaciones activas".to_string()
            },
            enabled: true,
            destructive: false,
            separator_before: false,
            action: ContextMenuAction::ToggleActiveAppsOnly,
        },
        ContextMenuItem {
            label: if config.show_launchpad {
                "Ocultar botón Launchpad".to_string()
            } else {
                "Mostrar botón Launchpad".to_string()
            },
            enabled: true,
            destructive: false,
            separator_before: false,
            action: ContextMenuAction::ToggleLaunchpad,
        },
        ContextMenuItem {
            label: "Añadir espaciador".to_string(),
            enabled: true,
            destructive: false,
            separator_before: true,
            action: ContextMenuAction::AddSpacer,
        },
        ContextMenuItem {
            label: "Ajustes de NeonPear Dock...".to_string(),
            enabled: true,
            destructive: false,
            separator_before: true,
            action: ContextMenuAction::OpenSettings,
        },
    ]
}
