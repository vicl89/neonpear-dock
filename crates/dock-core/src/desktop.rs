use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockSection {
    Pinned,
    Dynamic,
    Utility,
}

pub const SPACER_PREFIX: &str = "__spacer__:";

pub fn is_spacer_id(id: &str) -> bool {
    id.starts_with(SPACER_PREFIX)
}

pub fn make_spacer_app(id: String) -> DesktopApp {
    DesktopApp {
        id,
        name: String::new(),
        exec: String::new(),
        exec_binary: None,
        startup_wm_class: None,
        icon_name: String::new(),
        icon_path: None,
        actions: Vec::new(),
    }
}

#[derive(Debug, Clone)]
pub enum DockEntryKind {
    App(DesktopApp),
    Trash { full: bool },
    Spacer { id: String },
    Launchpad,
}

#[derive(Debug, Clone)]
pub struct DockEntry {
    pub key: String,
    pub label: String,
    pub section: DockSection,
    pub icon_path: Option<PathBuf>,
    pub kind: DockEntryKind,
}

impl DockEntry {
    pub fn spacer(id: String) -> Self {
        Self {
            key: format!("spacer:{id}"),
            label: String::new(),
            section: DockSection::Pinned,
            icon_path: None,
            kind: DockEntryKind::Spacer { id },
        }
    }

    pub fn from_app(app: DesktopApp, section: DockSection) -> Self {
        let prefix = match section {
            DockSection::Pinned => "pinned",
            DockSection::Dynamic => "dyn",
            DockSection::Utility => "util",
        };
        Self {
            key: format!("{prefix}:{}", normalize_id(&app.id)),
            label: app.name.clone(),
            section,
            icon_path: app.icon_path.clone(),
            kind: DockEntryKind::App(app),
        }
    }

    pub fn icon_cache_key(&self) -> String {
        match &self.icon_path {
            Some(p) => format!("path:{}", p.to_string_lossy()),
            None => format!("fallback:{}", self.key),
        }
    }

    pub fn matches_window_app_id(&self, window_app_id: &str) -> bool {
        match &self.kind {
            DockEntryKind::App(app) => app.matches_app_id(window_app_id),
            DockEntryKind::Trash { .. } | DockEntryKind::Spacer { .. } | DockEntryKind::Launchpad => false,
        }
    }

    pub fn as_app(&self) -> Option<&DesktopApp> {
        match &self.kind {
            DockEntryKind::App(app) => Some(app),
            DockEntryKind::Trash { .. } | DockEntryKind::Spacer { .. } | DockEntryKind::Launchpad => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DesktopAction {
    pub id: String,
    pub name: String,
    pub exec: String,
}

#[derive(Debug, Clone)]
pub struct DesktopApp {
    pub id: String,
    pub name: String,
    pub exec: String,
    pub exec_binary: Option<String>,
    pub startup_wm_class: Option<String>,
    pub icon_name: String,
    pub icon_path: Option<PathBuf>,
    pub actions: Vec<DesktopAction>,
}

impl DesktopApp {
    /// Comprueba si el `app_id` reportado por KWin corresponde a esta aplicación.
    pub fn matches_app_id(&self, window_app_id: &str) -> bool {
        let win = normalize_id(window_app_id);
        if win.is_empty() {
            return false;
        }

        let id_norm = normalize_id(&self.id);
        if win == id_norm || matches_rdns_suffix(&win, &id_norm) {
            return true;
        }

        if let Some(wm_class) = &self.startup_wm_class {
            let wm_norm = normalize_id(wm_class);
            if !wm_norm.is_empty() && (win == wm_norm || matches_rdns_suffix(&win, &wm_norm)) {
                return true;
            }
        }

        if let Some(bin) = &self.exec_binary {
            let bin_norm = normalize_id(bin);
            if !bin_norm.is_empty() && (win == bin_norm || matches_rdns_suffix(&win, &bin_norm)) {
                return true;
            }
        }

        false
    }

    /// Lanza la aplicación desacoplada del proceso del dock.
    pub fn launch(&self) {
        Self::launch_exec(&self.name, &self.exec);
    }

    /// Lanza la aplicación pasando una lista de rutas de archivo como argumentos.
    pub fn launch_with_files(&self, files: &[String]) {
        if files.is_empty() {
            self.launch();
            return;
        }
        let (envs, cmd_parts) = parse_exec_command(&self.exec);
        let Some((program, args)) = cmd_parts.split_first() else {
            tracing::warn!(app = %self.name, "Comando Exec vacío");
            return;
        };

        let mut command = Command::new(program);
        command
            .args(args)
            .args(files)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for (k, v) in envs {
            command.env(k, v);
        }
        if let Err(err) = command.spawn() {
            tracing::error!(app = %self.name, ?files, "No se pudo lanzar con archivos: {err}");
        }
    }

    /// Lanza cualquier comando `Exec` (principal o de una `[Desktop Action ...]`) desacoplado.
    pub fn launch_exec(label: &str, exec: &str) {
        let (envs, cmd_parts) = parse_exec_command(exec);
        let Some((program, args)) = cmd_parts.split_first() else {
            tracing::warn!(app = %label, "Comando Exec vacío");
            return;
        };

        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        for (k, v) in envs {
            command.env(k, v);
        }

        match command.spawn() {
            Ok(_) => tracing::info!(app = %label, %program, "Aplicación/acción lanzada"),
            Err(err) => tracing::error!(app = %label, %program, "Error al lanzar: {err}"),
        }
    }
}

/// Catálogo en memoria de todos los `.desktop` instalados y el tema de iconos activo.
#[derive(Debug, Clone)]
pub struct AppCatalog {
    icon_theme: Option<String>,
    by_id: HashMap<String, DesktopApp>,
}

impl AppCatalog {
    pub fn scan() -> Self {
        let icon_theme = detect_kde_icon_theme();
        tracing::info!(?icon_theme, "Tema de iconos detectado en Plasma");
        let by_id = scan_desktop_files(icon_theme.as_deref());
        tracing::info!(total = by_id.len(), "Catálogo de aplicaciones .desktop cargado");
        Self { icon_theme, by_id }
    }

    pub fn resolve_pinned(&self, pinned_ids: &[String]) -> Vec<DesktopApp> {
        let mut resolved = Vec::new();

        for wanted in pinned_ids {
            if is_spacer_id(wanted) {
                resolved.push(make_spacer_app(wanted.clone()));
                continue;
            }
            let exact = wanted.trim().to_ascii_lowercase();
            let normalized = normalize_id(wanted);
            if let Some(app) = self
                .by_id
                .get(&exact)
                .or_else(|| self.by_id.get(&normalized))
                .or_else(|| self.by_id.values().find(|a| a.matches_app_id(wanted)))
            {
                resolved.push(app.clone());
            } else {
                tracing::warn!(desktop_id = %wanted, "Archivo .desktop no encontrado; se omite");
            }
        }

        if resolved.is_empty() {
            let mut fallback: Vec<DesktopApp> = self.by_id.values().cloned().collect();
            fallback.sort_by(|a, b| a.name.cmp(&b.name));
            fallback.truncate(6);
            return fallback;
        }

        resolved
    }

    /// Resuelve el `DesktopApp` correspondiente a una ventana abierta reportada por KWin.
    pub fn resolve_window_app(&self, window_app_id: &str, window_title: &str) -> DesktopApp {
        let key = normalize_id(window_app_id);

        if let Some(exact) = self.by_id.get(&key) {
            return exact.clone();
        }

        if let Some(wm_match) = self.by_id.values().find(|app| {
            app.startup_wm_class
                .as_deref()
                .map(|wm| normalize_id(wm) == key)
                .unwrap_or(false)
        }) {
            return wm_match.clone();
        }

        if let Some(fuzzy) = self.by_id.values().find(|app| app.matches_app_id(window_app_id)) {
            return fuzzy.clone();
        }

        // Respaldo para apps sin archivo .desktop (p. ej. binarios locales o AppImage)
        let short_name = window_app_id
            .rsplit('.')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or(window_app_id);
        let icon_path = resolve_icon_path(window_app_id, self.icon_theme.as_deref())
            .or_else(|| resolve_icon_path(short_name, self.icon_theme.as_deref()))
            .or_else(|| resolve_icon_path(&key, self.icon_theme.as_deref()));

        let display_name = if !window_title.trim().is_empty() {
            window_title.trim().to_string()
        } else {
            short_name.to_string()
        };

        DesktopApp {
            id: window_app_id.to_string(),
            name: display_name,
            exec: String::new(),
            exec_binary: Some(short_name.to_ascii_lowercase()),
            startup_wm_class: Some(window_app_id.to_string()),
            icon_name: short_name.to_string(),
            icon_path,
            actions: Vec::new(),
        }
    }

    /// Crea la entrada de la Papelera con el icono vacío o lleno del tema actual.
    pub fn trash_entry(&self, full: bool) -> DockEntry {
        let icon_path = if full {
            resolve_icon_path("user-trash-full", self.icon_theme.as_deref())
                .or_else(|| resolve_icon_path("user-trash", self.icon_theme.as_deref()))
        } else {
            resolve_icon_path("user-trash", self.icon_theme.as_deref())
        };

        DockEntry {
            key: "util:trash".to_string(),
            label: "Papelera".to_string(),
            section: DockSection::Utility,
            icon_path,
            kind: DockEntryKind::Trash { full },
        }
    }

    /// Crea la entrada del botón Launchpad usando el icono del tema actual de Plasma.
    pub fn launchpad_entry(&self) -> DockEntry {
        let icon_path = resolve_icon_path("view-app-grid", self.icon_theme.as_deref())
            .or_else(|| resolve_icon_path("view-app-grid-symbolic", self.icon_theme.as_deref()))
            .or_else(|| resolve_icon_path("start-here-kde", self.icon_theme.as_deref()))
            .or_else(|| resolve_icon_path("applications-other", self.icon_theme.as_deref()));

        DockEntry {
            key: "util:launchpad".to_string(),
            label: "Aplicaciones".to_string(),
            section: DockSection::Pinned,
            icon_path,
            kind: DockEntryKind::Launchpad,
        }
    }
}

/// Abre el menú lanzador de aplicaciones de KDE Plasma vía D-Bus.
pub fn open_launchpad() {
    let spawned = std::process::Command::new("qdbus6")
        .args([
            "org.kde.plasmashell",
            "/PlasmaShell",
            "org.kde.PlasmaShell.activateLauncherMenu",
        ])
        .spawn();

    if spawned.is_err() {
        let _ = std::process::Command::new("qdbus")
            .args([
                "org.kde.plasmashell",
                "/PlasmaShell",
                "org.kde.PlasmaShell.activateLauncherMenu",
            ])
            .spawn();
    }
}

/// Ruta estándar de archivos en la Papelera (`~/.local/share/Trash/files`).
pub fn trash_files_dir() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("Trash").join("files"))
}

/// Devuelve `true` si la Papelera contiene al menos un archivo o carpeta.
pub fn is_trash_non_empty() -> bool {
    let Some(dir) = trash_files_dir() else {
        return false;
    };
    fs::read_dir(dir)
        .ok()
        .and_then(|mut it| it.next())
        .is_some()
}

/// Abre la Papelera en el gestor de archivos de KDE Plasma (`trash:/`).
pub fn open_trash() {
    let candidates: &[(&str, &[&str])] = &[
        ("kioclient", &["exec", "trash:/"]),
        ("xdg-open", &["trash:/"]),
        ("dolphin", &["trash:/"]),
    ];

    for (prog, args) in candidates {
        let res = Command::new(prog)
            .args(*args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if res.is_ok() {
            tracing::info!(%prog, "Papelera abierta");
            return;
        }
    }

    tracing::warn!("No se pudo abrir trash:/ con kioclient, xdg-open ni dolphin");
}

/// Vacía la Papelera usando `ktrash6 --empty` (o limpieza directa como respaldo).
pub fn empty_trash() {
    for prog in ["ktrash6", "ktrash5"] {
        let res = Command::new(prog)
            .arg("--empty")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if res.is_ok() {
            tracing::info!(%prog, "Vaciando Papelera");
            return;
        }
    }

    if let Some(data_dir) = dirs::data_dir() {
        for sub in ["Trash/files", "Trash/info"] {
            let dir = data_dir.join(sub);
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    let _ = if p.is_dir() {
                        fs::remove_dir_all(&p)
                    } else {
                        fs::remove_file(&p)
                    };
                }
            }
        }
        tracing::info!("Papelera vaciada directamente desde disco");
    }
}


/// Envoltorio compatible con la carga directa de apps fijadas.
pub fn load_pinned_apps(pinned_ids: &[String]) -> Vec<DesktopApp> {
    AppCatalog::scan().resolve_pinned(pinned_ids)
}

pub fn normalize_id(raw: &str) -> String {
    let trimmed = raw.trim();
    let without_ext = trimmed
        .strip_suffix(".desktop")
        .or_else(|| trimmed.strip_suffix(".DESKTOP"))
        .unwrap_or(trimmed);
    without_ext.to_ascii_lowercase()
}

/// Permite emparejar IDs cortos con nombres DNS inversos (ej. `systemsettings` <-> `org.kde.systemsettings`).
fn matches_rdns_suffix(a: &str, b: &str) -> bool {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    short.len() >= 4 && !short.contains('.') && long.rsplit('.').next() == Some(short)
}

fn extract_exec_binary(exec: &str) -> Option<String> {
    let (_, cmd_parts) = parse_exec_command(exec);
    let program = cmd_parts.first()?;
    let basename = Path::new(program)
        .file_name()
        .and_then(|n| n.to_str())?
        .to_ascii_lowercase();

    if matches!(
        basename.as_str(),
        "env" | "flatpak" | "sh" | "bash" | "zsh" | "python" | "python3" | "java"
    ) {
        return None;
    }

    Some(basename)
}

fn parse_exec_command(exec: &str) -> (Vec<(String, String)>, Vec<String>) {
    let tokens = split_exec_tokens(exec);
    let mut envs = Vec::new();
    let mut cmd_parts = Vec::new();
    let mut parsing_envs = true;

    for token in tokens {
        if token.starts_with('%') && token.len() == 2 {
            continue;
        }
        if parsing_envs {
            if let Some((k, v)) = token.split_once('=') {
                if !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    envs.push((k.to_string(), v.to_string()));
                    continue;
                }
            }
            parsing_envs = false;
        }
        cmd_parts.push(token);
    }

    (envs, cmd_parts)
}

/// Lee el tema de iconos activo en KDE Plasma desde `~/.config/kdeglobals`.
pub fn detect_kde_icon_theme() -> Option<String> {
    let path = dirs::config_dir()?.join("kdeglobals");
    let content = fs::read_to_string(path).ok()?;
    let mut in_icons_section = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_icons_section = trimmed == "[Icons]";
            continue;
        }
        if in_icons_section {
            if let Some(theme) = trimmed.strip_prefix("Theme=") {
                let name = theme.trim();
                if !name.is_empty() {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

fn desktop_search_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(data_dir) = dirs::data_dir() {
        out.push(data_dir.join("applications"));
        out.push(data_dir.join("flatpak/exports/share/applications"));
    }
    out.push(PathBuf::from("/usr/local/share/applications"));
    out.push(PathBuf::from("/usr/share/applications"));
    out.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
    out
}

fn scan_desktop_files(icon_theme: Option<&str>) -> HashMap<String, DesktopApp> {
    let mut map = HashMap::new();

    for dir in desktop_search_dirs() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let key = stem.to_lowercase();
            if map.contains_key(&key) {
                continue;
            }
            if let Some(app) = parse_desktop_file(&path, stem, icon_theme) {
                map.insert(key, app);
            }
        }
    }

    map
}

fn parse_desktop_file(
    path: &Path,
    stem: &str,
    icon_theme: Option<&str>,
) -> Option<DesktopApp> {
    let content = fs::read_to_string(path).ok()?;
    let mut in_main = false;
    let mut current_action: Option<String> = None;
    let mut name = None;
    let mut exec = None;
    let mut icon_name = None;
    let mut startup_wm_class = None;
    let mut entry_type = None;
    let mut no_display = false;
    let mut hidden = false;
    let mut declared_actions: Vec<String> = Vec::new();
    let mut action_map: std::collections::HashMap<String, (Option<String>, Option<String>, Option<String>)> =
        std::collections::HashMap::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_main = trimmed == "[Desktop Entry]";
            current_action = trimmed
                .strip_prefix("[Desktop Action ")
                .and_then(|s| s.strip_suffix(']'))
                .map(|s| s.trim().to_string());
            continue;
        }
        let Some((k, v)) = trimmed.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let val = v.trim();

        if in_main {
            match key {
                "Type" => entry_type = Some(val.to_string()),
                "Name" => name = Some(val.to_string()),
                "Exec" => exec = Some(val.to_string()),
                "Icon" => icon_name = Some(val.to_string()),
                "StartupWMClass" => {
                    if !val.is_empty() {
                        startup_wm_class = Some(val.to_string());
                    }
                }
                "Actions" => {
                    declared_actions = val
                        .split(';')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .collect();
                }
                "NoDisplay" => no_display = val.eq_ignore_ascii_case("true"),
                "Hidden" => hidden = val.eq_ignore_ascii_case("true"),
                _ => {}
            }
        } else if let Some(act_id) = &current_action {
            let entry = action_map.entry(act_id.clone()).or_insert((None, None, None));
            match key {
                "Name" => entry.0 = Some(val.to_string()),
                "Name[es]" | "Name[es_MX]" => entry.1 = Some(val.to_string()),
                "Exec" => entry.2 = Some(val.to_string()),
                _ => {}
            }
        }
    }

    if entry_type.as_deref() != Some("Application") || no_display || hidden {
        return None;
    }

    let exec = exec?;
    let exec_binary = extract_exec_binary(&exec);
    let name = name.unwrap_or_else(|| stem.to_string());
    let icon_name = icon_name.unwrap_or_default();
    let icon_path = resolve_icon_path(&icon_name, icon_theme);

    let mut actions = Vec::new();
    for act_id in declared_actions {
        if let Some((default_name, es_name, Some(act_exec))) = action_map.remove(&act_id) {
            let act_name = es_name.or(default_name).unwrap_or_else(|| act_id.clone());
            if !act_exec.is_empty() && !act_name.is_empty() {
                actions.push(DesktopAction {
                    id: act_id,
                    name: act_name,
                    exec: act_exec,
                });
            }
        }
        if actions.len() >= 6 {
            break;
        }
    }

    Some(DesktopApp {
        id: stem.to_string(),
        name,
        exec,
        exec_binary,
        startup_wm_class,
        icon_name,
        icon_path,
        actions,
    })
}

fn resolve_icon_path(icon_name: &str, icon_theme: Option<&str>) -> Option<PathBuf> {
    if icon_name.is_empty() {
        return None;
    }

    let direct = Path::new(icon_name);
    if direct.is_absolute() && direct.exists() {
        return Some(direct.to_path_buf());
    }

    if let Some(theme) = icon_theme {
        if let Some(found) = freedesktop_icons::lookup(icon_name)
            .with_size(128)
            .with_theme(theme)
            .with_cache()
            .find()
        {
            return Some(found);
        }
    }

    freedesktop_icons::lookup(icon_name)
        .with_size(128)
        .with_cache()
        .find()
}

fn split_exec_tokens(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' if !in_single => escaped = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            c if c.is_whitespace() && !in_single && !in_double => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}
