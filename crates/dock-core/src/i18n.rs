use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    Auto,
    Es,
    En,
    Fr,
    It,
    Pt,
    Zh,
    Ja,
    Ko,
    Ru,
}

impl Language {
    pub const ALL: &'static [Language] = &[
        Language::Auto,
        Language::Es,
        Language::En,
        Language::Fr,
        Language::It,
        Language::Pt,
        Language::Zh,
        Language::Ja,
        Language::Ko,
        Language::Ru,
    ];

    /// Nombre nativo para mostrar en el selector de idioma.
    pub fn native_name(self, ui_lang: Language) -> &'static str {
        match self {
            Language::Auto => match ui_lang.resolve() {
                Language::Es => "Automático (Sistema)",
                Language::En => "Automatic (System)",
                Language::Fr => "Automatique (Système)",
                Language::It => "Automatico (Sistema)",
                Language::Pt => "Automático (Sistema)",
                Language::Zh => "自动（系统）",
                Language::Ja => "自動（システム）",
                Language::Ko => "자동 (시스템)",
                Language::Ru => "Автоматически (Система)",
                Language::Auto => "Automatic (System)",
            },
            Language::Es => "Español",
            Language::En => "English",
            Language::Fr => "Français",
            Language::It => "Italiano",
            Language::Pt => "Português",
            Language::Zh => "中文",
            Language::Ja => "日本語",
            Language::Ko => "한국어",
            Language::Ru => "Русский",
        }
    }

    /// Resuelve `Auto` inspeccionando las variables de entorno del sistema.
    pub fn resolve(self) -> Language {
        if self != Language::Auto {
            return self;
        }
        for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(val) = std::env::var(var) {
                if let Some(lang) = Self::from_locale(&val) {
                    return lang;
                }
            }
        }
        Language::En
    }

    pub fn from_locale(locale: &str) -> Option<Language> {
        let code = locale
            .trim()
            .split(['_', '-', '.', '@'])
            .next()?
            .to_ascii_lowercase();
        match code.as_str() {
            "es" => Some(Language::Es),
            "en" => Some(Language::En),
            "fr" => Some(Language::Fr),
            "it" => Some(Language::It),
            "pt" => Some(Language::Pt),
            "zh" => Some(Language::Zh),
            "ja" => Some(Language::Ja),
            "ko" => Some(Language::Ko),
            "ru" => Some(Language::Ru),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MenuStrings {
    pub new_window: &'static str,
    pub open: &'static str,
    pub unpin: &'static str,
    pub pin: &'static str,
    pub remove_recent: &'static str,
    pub close: &'static str,
    pub force_quit: &'static str,
    pub remove_spacer: &'static str,
    pub open_launcher: &'static str,
    pub hide_launchpad: &'static str,
    pub show_launchpad: &'static str,
    pub open_trash: &'static str,
    pub open_drive: &'static str,
    pub eject_drive: &'static str,
    pub unpin_folder: &'static str,
    pub empty_folder: &'static str,
    pub empty_trash: &'static str,
    pub dock_settings: &'static str,
    pub quit_dock: &'static str,
    pub disable_autohide: &'static str,
    pub enable_autohide: &'static str,
    pub disable_magnification: &'static str,
    pub enable_magnification: &'static str,
    pub hide_recent_apps: &'static str,
    pub show_recent_apps: &'static str,
    pub show_all_apps: &'static str,
    pub active_apps_only: &'static str,
    pub add_spacer: &'static str,
}

pub fn menu_strings(lang: Language) -> MenuStrings {
    match lang.resolve() {
        Language::Es => MenuStrings {
            new_window: "Nueva ventana",
            open: "Abrir",
            unpin: "Quitar del dock",
            pin: "Fijar en el dock",
            remove_recent: "Quitar de recientes",
            close: "Cerrar",
            force_quit: "Forzar salida",
            remove_spacer: "Quitar espaciador",
            open_launcher: "Abrir lanzador de aplicaciones",
            hide_launchpad: "Ocultar botón Launchpad",
            show_launchpad: "Mostrar botón Launchpad",
            open_trash: "Abrir Papelera",
            open_drive: "Abrir",
            eject_drive: "Expulsar",
            unpin_folder: "Quitar carpeta del dock",
            empty_folder: "(Carpeta vacía)",
            empty_trash: "Vaciar Papelera",
            dock_settings: "Configuración de NeonPear Dock…",
            quit_dock: "Cerrar NeonPear Dock",
            disable_autohide: "Desactivar ocultamiento automático",
            enable_autohide: "Activar ocultamiento automático",
            disable_magnification: "Desactivar ampliación",
            enable_magnification: "Activar ampliación",
            hide_recent_apps: "Ocultar aplicaciones recientes",
            show_recent_apps: "Mostrar aplicaciones recientes",
            show_all_apps: "Mostrar todas las aplicaciones",
            active_apps_only: "Solo aplicaciones activas",
            add_spacer: "Añadir espaciador",
        },
        Language::En | Language::Auto => MenuStrings {
            new_window: "New window",
            open: "Open",
            unpin: "Remove from dock",
            pin: "Pin to dock",
            remove_recent: "Remove from recent",
            close: "Close",
            force_quit: "Force quit",
            remove_spacer: "Remove spacer",
            open_launcher: "Open application launcher",
            hide_launchpad: "Hide Launchpad button",
            show_launchpad: "Show Launchpad button",
            open_trash: "Open Trash",
            open_drive: "Open",
            eject_drive: "Eject",
            unpin_folder: "Remove folder from dock",
            empty_folder: "(Empty folder)",
            empty_trash: "Empty Trash",
            dock_settings: "NeonPear Dock Settings…",
            quit_dock: "Quit NeonPear Dock",
            disable_autohide: "Disable auto-hide",
            enable_autohide: "Enable auto-hide",
            disable_magnification: "Disable magnification",
            enable_magnification: "Enable magnification",
            hide_recent_apps: "Hide recent applications",
            show_recent_apps: "Show recent applications",
            show_all_apps: "Show all applications",
            active_apps_only: "Active applications only",
            add_spacer: "Add spacer",
        },
        Language::Fr => MenuStrings {
            new_window: "Nouvelle fenêtre",
            open: "Ouvrir",
            unpin: "Retirer du dock",
            pin: "Épingler au dock",
            remove_recent: "Retirer des récents",
            close: "Fermer",
            force_quit: "Forcer à quitter",
            remove_spacer: "Supprimer l'espaceur",
            open_launcher: "Ouvrir le lanceur d'applications",
            hide_launchpad: "Masquer le bouton Launchpad",
            show_launchpad: "Afficher le bouton Launchpad",
            open_trash: "Ouvrir la corbeille",
            open_drive: "Ouvrir",
            eject_drive: "Éjecter",
            unpin_folder: "Retirer le dossier du dock",
            empty_folder: "(Dossier vide)",
            empty_trash: "Vider la corbeille",
            dock_settings: "Paramètres de NeonPear Dock…",
            quit_dock: "Quitter NeonPear Dock",
            disable_autohide: "Désactiver le masquage automatique",
            enable_autohide: "Activer le masquage automatique",
            disable_magnification: "Désactiver l'agrandissement",
            enable_magnification: "Activer l'agrandissement",
            hide_recent_apps: "Masquer les applications récentes",
            show_recent_apps: "Afficher les applications récentes",
            show_all_apps: "Afficher toutes les applications",
            active_apps_only: "Applications actives uniquement",
            add_spacer: "Ajouter un espaceur",
        },
        Language::It => MenuStrings {
            new_window: "Nuova finestra",
            open: "Apri",
            unpin: "Rimuovi dal dock",
            pin: "Fissa nel dock",
            remove_recent: "Rimuovi dai recenti",
            close: "Chiudi",
            force_quit: "Uscita forzata",
            remove_spacer: "Rimuovi spaziatore",
            open_launcher: "Apri lanciatore applicazioni",
            hide_launchpad: "Nascondi pulsante Launchpad",
            show_launchpad: "Mostra pulsante Launchpad",
            open_trash: "Apri Cestino",
            open_drive: "Apri",
            eject_drive: "Espelli",
            unpin_folder: "Rimuovi cartella dal dock",
            empty_folder: "(Cartella vuota)",
            empty_trash: "Svuota Cestino",
            dock_settings: "Impostazioni di NeonPear Dock…",
            quit_dock: "Chiudi NeonPear Dock",
            disable_autohide: "Disattiva scomparsa automatica",
            enable_autohide: "Attiva scomparsa automatica",
            disable_magnification: "Disattiva ingrandimento",
            enable_magnification: "Attiva ingrandimento",
            hide_recent_apps: "Nascondi app recenti",
            show_recent_apps: "Mostra app recenti",
            show_all_apps: "Mostra tutte le applicazioni",
            active_apps_only: "Solo applicazioni attive",
            add_spacer: "Aggiungi spaziatore",
        },
        Language::Pt => MenuStrings {
            new_window: "Nova janela",
            open: "Abrir",
            unpin: "Remover do dock",
            pin: "Fixar no dock",
            remove_recent: "Remover dos recentes",
            close: "Fechar",
            force_quit: "Forçar encerramento",
            remove_spacer: "Remover espaçador",
            open_launcher: "Abrir lançador de aplicativos",
            hide_launchpad: "Ocultar botão Launchpad",
            show_launchpad: "Mostrar botão Launchpad",
            open_trash: "Abrir Lixeira",
            open_drive: "Abrir",
            eject_drive: "Ejetar",
            unpin_folder: "Remover pasta do dock",
            empty_folder: "(Pasta vazia)",
            empty_trash: "Esvaziar Lixeira",
            dock_settings: "Configurações do NeonPear Dock…",
            quit_dock: "Sair do NeonPear Dock",
            disable_autohide: "Desativar ocultação automática",
            enable_autohide: "Ativar ocultação automática",
            disable_magnification: "Desativar ampliação",
            enable_magnification: "Ativar ampliação",
            hide_recent_apps: "Ocultar aplicativos recentes",
            show_recent_apps: "Mostrar aplicativos recentes",
            show_all_apps: "Mostrar todos os aplicativos",
            active_apps_only: "Apenas aplicativos ativos",
            add_spacer: "Adicionar espaçador",
        },
        Language::Zh => MenuStrings {
            new_window: "新建窗口",
            open: "打开",
            unpin: "从程序坞移除",
            pin: "固定到程序坞",
            remove_recent: "从最近移除",
            close: "关闭",
            force_quit: "强制退出",
            remove_spacer: "移除分隔符",
            open_launcher: "打开应用启动器",
            hide_launchpad: "隐藏启动台按钮",
            show_launchpad: "显示启动台按钮",
            open_trash: "打开回收站",
            open_drive: "打开",
            eject_drive: "弹出",
            unpin_folder: "从程序坞移除文件夹",
            empty_folder: "(空文件夹)",
            empty_trash: "清空回收站",
            dock_settings: "NeonPear Dock 设置…",
            quit_dock: "退出 NeonPear Dock",
            disable_autohide: "禁用自动隐藏",
            enable_autohide: "启用自动隐藏",
            disable_magnification: "禁用放大效果",
            enable_magnification: "启用放大效果",
            hide_recent_apps: "隐藏最近使用的应用",
            show_recent_apps: "显示最近使用的应用",
            show_all_apps: "显示所有应用",
            active_apps_only: "仅显示活动应用",
            add_spacer: "添加分隔符",
        },
        Language::Ja => MenuStrings {
            new_window: "新しいウィンドウ",
            open: "開く",
            unpin: "ドックから削除",
            pin: "ドックに固定",
            remove_recent: "最近使った項目から削除",
            close: "閉じる",
            force_quit: "強制終了",
            remove_spacer: "スペーサーを削除",
            open_launcher: "アプリランチャーを開く",
            hide_launchpad: "Launchpadボタンを隠す",
            show_launchpad: "Launchpadボタンを表示",
            open_trash: "ゴミ箱を開く",
            open_drive: "開く",
            eject_drive: "取り出す",
            unpin_folder: "ドックからフォルダを削除",
            empty_folder: "(空のフォルダ)",
            empty_trash: "ゴミ箱を空にする",
            dock_settings: "NeonPear Dock の設定…",
            quit_dock: "NeonPear Dock を終了",
            disable_autohide: "自動非表示を無効化",
            enable_autohide: "自動非表示を有効化",
            disable_magnification: "拡大表示を無効化",
            enable_magnification: "拡大表示を有効化",
            hide_recent_apps: "最近使ったアプリを隠す",
            show_recent_apps: "最近使ったアプリを表示",
            show_all_apps: "すべてのアプリを表示",
            active_apps_only: "起動中のアプリのみ",
            add_spacer: "スペーサーを追加",
        },
        Language::Ko => MenuStrings {
            new_window: "새 창",
            open: "열기",
            unpin: "독에서 제거",
            pin: "독에 고정",
            remove_recent: "최근 항목에서 제거",
            close: "닫기",
            force_quit: "강제 종료",
            remove_spacer: "구분 공간 제거",
            open_launcher: "앱 실행기 열기",
            hide_launchpad: "Launchpad 버튼 숨기기",
            show_launchpad: "Launchpad 버튼 표시",
            open_trash: "휴지통 열기",
            open_drive: "열기",
            eject_drive: "꺼내기",
            unpin_folder: "독에서 폴더 제거",
            empty_folder: "(빈 폴더)",
            empty_trash: "휴지통 비우기",
            dock_settings: "NeonPear Dock 설정…",
            quit_dock: "NeonPear Dock 종료",
            disable_autohide: "자동 숨기기 비활성화",
            enable_autohide: "자동 숨기기 활성화",
            disable_magnification: "확대 효과 비활성화",
            enable_magnification: "확대 효과 활성화",
            hide_recent_apps: "최근 앱 숨기기",
            show_recent_apps: "최근 앱 표시",
            show_all_apps: "모든 앱 표시",
            active_apps_only: "실행 중인 앱만 표시",
            add_spacer: "구분 공간 추가",
        },
        Language::Ru => MenuStrings {
            new_window: "Новое окно",
            open: "Открыть",
            unpin: "Убрать из дока",
            pin: "Закрепить в доке",
            remove_recent: "Убрать из недавних",
            close: "Закрыть",
            force_quit: "Принудительно завершить",
            remove_spacer: "Удалить разделитель",
            open_launcher: "Открыть меню приложений",
            hide_launchpad: "Скрыть кнопку Launchpad",
            show_launchpad: "Показать кнопку Launchpad",
            open_trash: "Открыть корзину",
            open_drive: "Открыть",
            eject_drive: "Извлечь",
            unpin_folder: "Убрать папку с дока",
            empty_folder: "(Пустая папка)",
            empty_trash: "Очистить корзину",
            dock_settings: "Настройки NeonPear Dock…",
            quit_dock: "Закрыть NeonPear Dock",
            disable_autohide: "Отключить автоскрытие",
            enable_autohide: "Включить автоскрытие",
            disable_magnification: "Отключить увеличение",
            enable_magnification: "Включить увеличение",
            hide_recent_apps: "Скрыть недавние приложения",
            show_recent_apps: "Показать недавние приложения",
            show_all_apps: "Показать все приложения",
            active_apps_only: "Только активные приложения",
            add_spacer: "Добавить разделитель",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_nine_locales() {
        assert_eq!(Language::from_locale("es_MX.UTF-8"), Some(Language::Es));
        assert_eq!(Language::from_locale("en_US.UTF-8"), Some(Language::En));
        assert_eq!(Language::from_locale("fr_FR.UTF-8"), Some(Language::Fr));
        assert_eq!(Language::from_locale("it_IT"), Some(Language::It));
        assert_eq!(Language::from_locale("pt_BR.UTF-8"), Some(Language::Pt));
        assert_eq!(Language::from_locale("zh_CN.UTF-8"), Some(Language::Zh));
        assert_eq!(Language::from_locale("ja_JP.UTF-8"), Some(Language::Ja));
        assert_eq!(Language::from_locale("ko_KR.UTF-8"), Some(Language::Ko));
        assert_eq!(Language::from_locale("ru_RU.UTF-8"), Some(Language::Ru));
        assert_eq!(Language::from_locale("C.UTF-8"), None);
    }

    #[test]
    fn all_languages_have_non_empty_menu_strings() {
        for &lang in Language::ALL {
            let s = menu_strings(lang);
            assert!(!s.open.is_empty());
            assert!(!s.dock_settings.is_empty());
            assert!(!s.add_spacer.is_empty());
        }
    }
}
