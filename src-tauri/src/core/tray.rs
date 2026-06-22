//! System tray icon and menu.
//!
//! Registers a tray icon so closing the main window hides it to the tray
//! instead of quitting the app (matching CC Switch's behavior). The tray
//! menu offers "Show main window" and "Quit"; clicking the icon shows the
//! window. Quitting via the tray shuts down the Local Gateway before exit.

use crate::core::gateway;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

pub const TRAY_ID: &str = "codestudio-lite";

/// Localized tray menu labels for the app's supported languages.
struct TrayLabels {
    show: &'static str,
    quit: &'static str,
    tooltip: &'static str,
}

impl TrayLabels {
    fn for_language(language: &str) -> Self {
        let normalized = language.trim();
        if normalized.eq_ignore_ascii_case("zh-CN") || normalized.starts_with("zh-Hans") {
            Self {
                show: "显示主窗口",
                quit: "退出",
                tooltip: "CodeStudio Lite",
            }
        } else if normalized.eq_ignore_ascii_case("zh-TW") || normalized.starts_with("zh-Hant") {
            Self {
                show: "顯示主視窗",
                quit: "退出",
                tooltip: "CodeStudio Lite",
            }
        } else {
            Self {
                show: "Show main window",
                quit: "Quit",
                tooltip: "CodeStudio Lite",
            }
        }
    }
}

/// Build and register the tray icon + menu. Called once during app setup.
/// Failure is non-fatal: the app still runs without a tray if the platform
/// rejects it.
pub fn setup(app: &AppHandle) {
    let labels = current_labels(app);
    let show = match MenuItem::with_id(app, "tray-show", labels.show, true, None::<&str>) {
        Ok(item) => item,
        Err(err) => {
            eprintln!("[tray] failed to build show item: {err}");
            return;
        }
    };
    let quit = match MenuItem::with_id(app, "tray-quit", labels.quit, true, None::<&str>) {
        Ok(item) => item,
        Err(err) => {
            eprintln!("[tray] failed to build quit item: {err}");
            return;
        }
    };
    let menu = match Menu::with_items(app, &[&show, &quit]) {
        Ok(menu) => menu,
        Err(err) => {
            eprintln!("[tray] failed to build menu: {err}");
            return;
        }
    };

    let icon = app
        .default_window_icon()
        .cloned()
        .or_else(|| tauri::image::Image::from_bytes(include_bytes!("../../icons/32x32.png")).ok());

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(labels.tooltip)
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "tray-show" => show_main_window(app),
            "tray-quit" => quit_app(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::DoubleClick { .. }
                    | TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        ..
                    }
            ) {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }

    if let Err(err) = builder.build(app) {
        eprintln!("[tray] failed to build tray icon: {err}");
    }
}

/// Rebuild the tray menu with the current app language. Call after the user
/// changes the UI language so the tray labels stay localized.
pub fn refresh(app: &AppHandle) {
    let labels = current_labels(app);
    let show = match MenuItem::with_id(app, "tray-show", labels.show, true, None::<&str>) {
        Ok(item) => item,
        Err(_) => return,
    };
    let quit = match MenuItem::with_id(app, "tray-quit", labels.quit, true, None::<&str>) {
        Ok(item) => item,
        Err(_) => return,
    };
    let menu = match Menu::with_items(app, &[&show, &quit]) {
        Ok(menu) => menu,
        Err(_) => return,
    };
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_menu(Some(menu));
        let _ = tray.set_tooltip(Some(labels.tooltip));
    }
}

fn current_labels(_app: &AppHandle) -> TrayLabels {
    let language = crate::core::profile::load_app_settings()
        .map(|settings| settings.language)
        .unwrap_or_default();
    TrayLabels::for_language(&language)
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        #[cfg(target_os = "windows")]
        {
            let _ = window.set_skip_taskbar(false);
        }
    }
}

fn quit_app(app: &AppHandle) {
    gateway::shutdown_for_app_exit();
    app.exit(0);
}
