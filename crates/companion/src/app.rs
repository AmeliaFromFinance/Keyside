//! Shared context for every task in the companion.

use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use parking_lot::{Mutex, RwLock};
use tokio::sync::{broadcast, oneshot};

use crate::actions::Nav;
use crate::actions::input::Input;
use crate::actions::timers::Timers;
use crate::assets::Assets;
use crate::config::{Identity, Paths, Settings};
use crate::devices::Devices;
use crate::plugins::Host;
use crate::plugins::registry::Registry;
use crate::recorder::Recorder;
use crate::state::{Change, Engine};
use crate::tokens::Tokens;

pub struct Options {
    pub data_dir: Option<PathBuf>,
    /// Folder holding bundled plugins (unpacked folders or .pdp files).
    pub bundled_plugins: Option<PathBuf>,
}

pub struct App {
    pub paths: Paths,
    pub settings: RwLock<Settings>,
    pub identity: RwLock<Identity>,
    pub engine: Mutex<Engine>,
    pub devices: Mutex<Devices>,
    pub assets: Assets,
    pub plugins: Host,
    pub streamdeck: crate::streamdeck::Hub,
    pub registry: Registry,
    pub tokens: Mutex<Tokens>,
    pub input: Input,
    #[cfg(feature = "sound")]
    pub sound: crate::actions::sound::Sound,
    pub timers: Timers,
    pub recorder: Recorder,
    pub http: reqwest::Client,
    /// Device id -> open connections.
    pub online: Mutex<HashMap<String, usize>>,
    /// Pairing id -> waiting connection, answered when the person approves or denies.
    pub approvals: Mutex<HashMap<String, oneshot::Sender<bool>>>,
    /// Navigation for one device, from a key step like "go to page".
    pub nav: broadcast::Sender<(String, Nav)>,
    /// Keys whose steps are running, so a double tap doesn't start them twice.
    pub running: Mutex<HashSet<String>>,
    pub addresses: RwLock<Vec<IpAddr>>,
    pub tls_fingerprint: RwLock<String>,
    pub bundled_plugins: Option<PathBuf>,
    pub started: std::time::Instant,
    /// Signed-in local editor sessions (cookie value -> last use).
    pub admin_sessions: Mutex<HashMap<String, std::time::Instant>>,
    /// Failed editor sign-ins and the time sign-in is blocked until.
    pub login_fails: Mutex<(u32, Option<std::time::Instant>)>,
}

impl App {
    pub fn new(opts: Options) -> Result<Arc<Self>> {
        let paths = Paths::new(opts.data_dir)?;
        let settings = Settings::load(&paths)?;
        let identity = Identity::load_or_create(&paths)?;
        let (engine, _) = Engine::load(paths.clone())?;
        let devices = Devices::load(paths.file("devices.json"))?;
        let assets = Assets::open(paths.dir("assets")?)?;
        let plugins = Host::open(paths.root())?;
        let streamdeck = crate::streamdeck::Hub::open(paths.root())?;
        let tokens = Tokens::load(paths.file("tokens.json"))?;
        let http = reqwest::Client::builder().user_agent(concat!("PhoneDeck/", env!("CARGO_PKG_VERSION"))).redirect(reqwest::redirect::Policy::limited(5)).build()?;
        let (nav, _) = broadcast::channel(64);
        let app = Arc::new(Self {
            paths,
            settings: RwLock::new(settings),
            identity: RwLock::new(identity),
            engine: Mutex::new(engine),
            devices: Mutex::new(devices),
            assets,
            plugins,
            streamdeck,
            registry: Registry::default(),
            tokens: Mutex::new(tokens),
            input: Input::start(),
            #[cfg(feature = "sound")]
            sound: crate::actions::sound::Sound::start(),
            timers: Timers::default(),
            recorder: Recorder::default(),
            http,
            online: Mutex::new(HashMap::new()),
            approvals: Mutex::new(HashMap::new()),
            nav,
            running: Mutex::new(HashSet::new()),
            addresses: RwLock::new(Vec::new()),
            tls_fingerprint: RwLock::new(String::new()),
            bundled_plugins: opts.bundled_plugins,
            started: std::time::Instant::now(),
            admin_sessions: Mutex::new(HashMap::new()),
            login_fails: Mutex::new((0, None)),
        });
        app.plugins.attach(&app);
        Ok(app)
    }

    pub fn navigate(&self, device: &str, nav: Nav) {
        match nav {
            Nav::Profile(p) => {
                self.engine.lock().set_active_profile(&p);
            }
            page @ Nav::Page(_) => {
                let _ = self.nav.send((device.to_string(), page));
            }
        }
    }

    pub fn is_online(&self, device: &str) -> bool {
        self.online.lock().get(device).is_some_and(|n| *n > 0)
    }

    pub fn notify(&self, c: Change) {
        self.engine.lock().notify(c);
    }
}
