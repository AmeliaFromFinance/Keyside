//! Desktop app: starts the companion, opens the editor and lives in the tray.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use keyside::app::{App, Options};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

const BACKGROUND: &str = "--background";

struct Companion(Arc<App>);

/// Zoom that keeps the editor readable on big screens: 1080 logical pixels tall is 100%.
fn auto_zoom(monitor: Option<tauri::Monitor>) -> f64 {
    let Some(m) = monitor else { return 1.0 };
    let tall = f64::from(m.size().height) / m.scale_factor();
    ((tall / 1080.0 * 4.0).round() / 4.0).clamp(1.0, 2.0)
}

/// Zooms the editor window to the saved level, or the automatic one. Returns the level used.
fn apply_zoom(handle: &AppHandle, saved: Option<f64>) -> f64 {
    let Some(w) = handle.get_webview_window("editor") else { return saved.unwrap_or(1.0) };
    let zoom = saved.unwrap_or_else(|| auto_zoom(w.current_monitor().ok().flatten()));
    if let Err(e) = w.set_zoom(zoom) {
        tracing::warn!("couldn't zoom the editor: {e}");
    }
    *handle.state::<Companion>().0.editor_zoom.write() = zoom;
    zoom
}

/// The editor is the companion's own local page. It gets no access to Tauri's APIs; zoom
/// requests reach this app through the companion instead.
fn open_editor(handle: &AppHandle) {
    if let Some(w) = handle.get_webview_window("editor") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let app = handle.state::<Companion>().0.clone();
    let Ok(url) = keyside::editor_url(&app).parse() else { return };
    // Size the window for the zoom it will get, within the screen.
    let monitor = handle.primary_monitor().ok().flatten();
    let zoom = app.settings.read().editor_zoom.unwrap_or_else(|| auto_zoom(monitor.clone()));
    let (max_w, max_h) = monitor.map(|m| (f64::from(m.size().width) / m.scale_factor() * 0.9, f64::from(m.size().height) / m.scale_factor() * 0.9)).unwrap_or((f64::MAX, f64::MAX));
    let built = WebviewWindowBuilder::new(handle, "editor", WebviewUrl::External(url))
        .title("Keyside")
        .inner_size((1360.0 * zoom).min(max_w), (860.0 * zoom).min(max_h))
        .min_inner_size(960.0, 600.0)
        .initialization_script("window.__KEYSIDE_DESKTOP__ = true;")
        // Some webviews reset zoom when a page loads.
        .on_page_load(|w, p| {
            if matches!(p.event(), tauri::webview::PageLoadEvent::Finished) {
                let saved = w.app_handle().state::<Companion>().0.settings.read().editor_zoom;
                apply_zoom(w.app_handle(), saved);
            }
        })
        .build();
    match built {
        Ok(_) => {
            let saved = app.settings.read().editor_zoom;
            apply_zoom(handle, saved);
        }
        Err(e) => tracing::error!("couldn't open the editor window: {e}"),
    }
}

fn main() {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_env("KEYSIDE_LOG").unwrap_or_else(|_| "info".into())).init();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| open_editor(app)))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec![BACKGROUND])))
        .setup(|app| {
            let plugins = app.path().resource_dir().ok().map(|d| d.join("plugins")).filter(|d| d.is_dir());
            let companion = tauri::async_runtime::block_on(keyside::start(Options { data_dir: None, bundled_plugins: plugins }))?;
            let handle = app.handle().clone();
            *companion.zoom_hook.write() = Some(Box::new(move |saved| apply_zoom(&handle, saved)));
            app.manage(Companion(companion));

            let open = MenuItem::with_id(app, "open", "Open Keyside", true, None::<&str>)?;
            let login = CheckMenuItem::with_id(app, "autostart", "Start when I sign in", true, app.autolaunch().is_enabled().unwrap_or(false), None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit Keyside", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &login, &PredefinedMenuItem::separator(app)?, &quit])?;
            let login_item = login.clone();
            TrayIconBuilder::with_id("tray")
                .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
                .icon_as_template(true)
                .tooltip("Keyside")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "open" => open_editor(app),
                    "autostart" => {
                        let auto = app.autolaunch();
                        let on = auto.is_enabled().unwrap_or(false);
                        let _ = if on { auto.disable() } else { auto.enable() };
                        let _ = login_item.set_checked(auto.is_enabled().unwrap_or(false));
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        open_editor(tray.app_handle());
                    }
                })
                .build(app)?;

            if !std::env::args().any(|a| a == BACKGROUND) {
                open_editor(app.handle());
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Keyside couldn't start");

    app.run(|_app, event| {
        // Closing the editor keeps Keyside running in the tray. Quit comes from the tray menu.
        if let RunEvent::ExitRequested { code: None, api, .. } = event {
            api.prevent_exit();
        }
    });
}
