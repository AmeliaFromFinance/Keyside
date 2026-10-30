//! Keyside companion.

pub mod actions;
pub mod app;
pub mod assets;
pub mod config;
pub mod crypto;
pub mod devices;
pub mod focus;
pub mod iconpack;
pub mod ids;
pub mod mdns;
pub mod model;
pub mod plugins;
pub mod programs;
pub mod recorder;
pub mod server;
pub mod share;
pub mod state;
pub mod store;
pub mod streamdeck;
pub mod templates;
pub mod tokens;

use std::sync::Arc;

use app::{App, Options};
use state::Change;

/// Starts everything and returns the shared context. The desktop app calls this too.
pub async fn start(opts: Options) -> anyhow::Result<Arc<App>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let app = App::new(opts)?;
    server::start(app.clone()).await?;
    if let Some(dir) = app.bundled_plugins.clone() {
        app.plugins.seed_bundled(&dir);
    }
    app.plugins.start_all();
    if let Err(e) = streamdeck::start_all(&app).await {
        tracing::warn!("Stream Deck plugins are off: {e:#}");
    }
    streamdeck::hardware::spawn(app.clone());
    actions::timers::Timers::spawn_ticker(app.clone());
    #[cfg(feature = "nowplaying")]
    actions::nowplaying::spawn(app.clone());
    #[cfg(any(target_os = "linux", all(windows, feature = "appaudio")))]
    actions::appaudio::spawn(app.clone());
    focus::spawn(app.clone());

    // Keep plugins in step with layout changes, and save user variables a little after they change.
    let mut rx = app.engine.lock().subscribe();
    let a = app.clone();
    tokio::spawn(async move {
        let mut save_at: Option<tokio::time::Instant> = None;
        loop {
            let due = async {
                match save_at {
                    Some(t) => tokio::time::sleep_until(t).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                c = rx.recv() => match c {
                    Ok(Change::Layout) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => a.plugins.sync_all().await,
                    Ok(Change::Vars) => {
                        save_at.get_or_insert(tokio::time::Instant::now() + std::time::Duration::from_millis(500));
                    }
                    Ok(_) => {}
                    Err(_) => break,
                },
                _ = due => {
                    save_at = None;
                    save_vars(&a).await;
                }
            }
        }
    });
    Ok(app)
}

async fn save_vars(app: &App) {
    let Some((path, vars)) = app.engine.lock().take_unsaved_vars() else { return };
    let r = tokio::task::spawn_blocking(move || store::write_json(&path, &vars)).await;
    if let Ok(Err(e)) = r {
        tracing::warn!("could not save variables: {e:#}");
    }
}

/// Stops every plugin process. Call before the process exits, or they live on without it.
pub async fn shutdown(app: &App) {
    save_vars(app).await;
    app.plugins.stop_all();
    streamdeck::stop_all(app);
    streamdeck::hardware::release().await;
    // Plugins get a shutdown message and a moment to leave before they're killed.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
}

/// Resolves when the system asks Keyside to quit: Ctrl+C, or SIGTERM at logout or from `kill`.
pub async fn quit_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = term.recv() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

pub fn editor_url(app: &App) -> String {
    format!("http://127.0.0.1:{}/#login={}", app.settings.read().admin_port, app.identity.read().admin_token)
}
