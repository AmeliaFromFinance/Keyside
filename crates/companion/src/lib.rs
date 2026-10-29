//! PhoneDeck companion.

pub mod actions;
pub mod app;
pub mod assets;
pub mod config;
pub mod crypto;
pub mod devices;
pub mod focus;
pub mod ids;
pub mod mdns;
pub mod model;
pub mod plugins;
pub mod recorder;
pub mod server;
pub mod share;
pub mod state;
pub mod store;
pub mod streamdeck;
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
    focus::spawn(app.clone());

    // Keep plugins in step with layout changes.
    let mut rx = app.engine.lock().subscribe();
    let a = app.clone();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(Change::Layout) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => a.plugins.sync_all().await,
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
    Ok(app)
}

pub fn editor_url(app: &App) -> String {
    format!("http://127.0.0.1:{}/#login={}", app.settings.read().admin_port, app.identity.read().admin_token)
}
