//! Desktop app: starts the companion, opens the editor and lives in the tray.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use phonedeck::app::{App, Options};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

const BACKGROUND: &str = "--background";

struct Companion(Arc<App>);

/// The editor is the companion's own local page. It gets no access to Tauri's APIs.
fn open_editor(handle: &AppHandle) {
    if let Some(w) = handle.get_webview_window("editor") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let app = handle.state::<Companion>().0.clone();
    let Ok(url) = phonedeck::editor_url(&app).parse() else { return };
    let built = WebviewWindowBuilder::new(handle, "editor", WebviewUrl::External(url)).title("PhoneDeck").inner_size(1360.0, 860.0).min_inner_size(960.0, 600.0).build();
    if let Err(e) = built {
        tracing::error!("couldn't open the editor window: {e}");
    }
}

fn main() {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_env("PHONEDECK_LOG").unwrap_or_else(|_| "info".into())).init();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| open_editor(app)))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec![BACKGROUND])))
        .setup(|app| {
            let plugins = app.path().resource_dir().ok().map(|d| d.join("plugins")).filter(|d| d.is_dir());
            let companion = tauri::async_runtime::block_on(phonedeck::start(Options { data_dir: None, bundled_plugins: plugins }))?;
            app.manage(Companion(companion));

            let open = MenuItem::with_id(app, "open", "Open PhoneDeck", true, None::<&str>)?;
            let login = CheckMenuItem::with_id(app, "autostart", "Start when I sign in", true, app.autolaunch().is_enabled().unwrap_or(false), None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit PhoneDeck", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &login, &PredefinedMenuItem::separator(app)?, &quit])?;
            let login_item = login.clone();
            TrayIconBuilder::with_id("tray")
                .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
                .icon_as_template(true)
                .tooltip("PhoneDeck")
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
        .expect("PhoneDeck couldn't start");

    app.run(|_app, event| {
        // Closing the editor keeps PhoneDeck running in the tray. Quit comes from the tray menu.
        if let RunEvent::ExitRequested { code: None, api, .. } = event {
            api.prevent_exit();
        }
    });
}
