//! Physical Elgato Stream Decks plugged into this PC. Each one shows the active profile's
//! pages like any other deck, with keys drawn here as images.

use std::sync::Arc;

use crate::app::App;

/// Watches for Stream Decks being plugged in. Does nothing without the "hardware" feature.
pub fn spawn(app: Arc<App>) {
    #[cfg(feature = "hardware")]
    imp::spawn(app);
    #[cfg(not(feature = "hardware"))]
    let _ = app;
}

#[cfg(feature = "render")]
pub mod render;

#[cfg(feature = "hardware")]
mod imp {
    use std::collections::{HashMap, HashSet};
    use std::hash::{Hash, Hasher};
    use std::sync::Arc;
    use std::sync::mpsc as std_mpsc;
    use std::time::{Duration, Instant};

    use elgato_streamdeck::info::Kind;
    use elgato_streamdeck::{DeviceStateUpdate, StreamDeck};
    use image::DynamicImage;
    use tokio::sync::mpsc;

    use crate::actions::{self, Ctx, Nav};
    use crate::app::App;
    use crate::model::{FormFactor, KeyKind, Tone};
    use crate::state::view::KeyView;
    use crate::state::{Change, key_path};

    const SCAN_EVERY: Duration = Duration::from_secs(3);
    const BRIGHTNESS: u8 = 70;

    enum Cmd {
        Image(u8, DynamicImage),
        Clear(u8),
    }

    enum Event {
        Down(u8),
        Up(u8),
        HoldDone(String, u64),
        Gone,
    }

    pub fn name(kind: Kind) -> &'static str {
        match kind {
            Kind::Original | Kind::OriginalV2 | Kind::Mk2 | Kind::Mk2Scissor | Kind::Mk2Module | Kind::Mk2ModuleScissor => "Stream Deck",
            Kind::Mini | Kind::MiniMk2 | Kind::MiniDiscord | Kind::MiniMk2Module => "Stream Deck Mini",
            Kind::Xl | Kind::XlV2 | Kind::XlV2Module => "Stream Deck XL",
            Kind::Neo => "Stream Deck Neo",
            Kind::Pedal => "Stream Deck Pedal",
            Kind::Plus => "Stream Deck +",
            Kind::PlusXl => "Stream Deck + XL",
        }
    }

    pub fn spawn(app: Arc<App>) {
        let rt = tokio::runtime::Handle::current();
        std::thread::Builder::new()
            .name("stream-deck-scan".into())
            .spawn(move || scan(app, rt))
            .ok();
    }

    struct Open {
        deck: Arc<StreamDeck>,
        reader: Arc<elgato_streamdeck::DeviceStateReader>,
        cmds: std_mpsc::Receiver<Cmd>,
        events: mpsc::Sender<Event>,
    }

    /// Runs on its own thread, since HID calls block and device handles can't move between
    /// threads. Finds new decks, sends images and reads key presses for all of them.
    fn scan(app: Arc<App>, rt: tokio::runtime::Handle) {
        let mut hid = match elgato_streamdeck::new_hidapi() {
            Ok(h) => h,
            Err(e) => {
                tracing::info!("Stream Deck support is off: {e}");
                return;
            }
        };
        let mut decks: HashMap<String, Open> = HashMap::new();
        let mut last_scan: Option<Instant> = None;
        loop {
            if last_scan.is_none_or(|t| t.elapsed() >= SCAN_EVERY) {
                last_scan = Some(Instant::now());
                if !app.settings.read().hardware_decks {
                    for (_, d) in decks.drain() {
                        let _ = d.deck.reset();
                        let _ = d.events.blocking_send(Event::Gone);
                    }
                } else if elgato_streamdeck::refresh_device_list(&mut hid).is_ok() {
                    for (kind, serial) in elgato_streamdeck::list_devices(&hid) {
                        if decks.contains_key(&serial) {
                            continue;
                        }
                        let deck = match StreamDeck::connect(&hid, kind, &serial) {
                            Ok(d) => Arc::new(d),
                            Err(e) => {
                                tracing::warn!("couldn't open {} ({serial}): {e}", name(kind));
                                continue;
                            }
                        };
                        let _ = deck.set_brightness(BRIGHTNESS);
                        let _ = deck.clear_all_button_images();
                        let (cmd_tx, cmd_rx) = std_mpsc::channel();
                        let (ev_tx, ev_rx) = mpsc::channel(64);
                        tracing::info!("{} connected", name(kind));
                        app.engine.lock().toast(format!("{} connected", name(kind)), Tone::Success);
                        let ctl = Controller::new(app.clone(), kind, serial.clone(), cmd_tx, ev_tx.clone());
                        rt.spawn(ctl.run(ev_rx));
                        decks.insert(serial, Open { reader: deck.get_reader(), deck, cmds: cmd_rx, events: ev_tx });
                    }
                }
            }
            if decks.is_empty() {
                std::thread::sleep(Duration::from_millis(250));
                continue;
            }
            let gone: Vec<String> = decks.iter().filter(|(_, d)| !service(d)).map(|(s, _)| s.clone()).collect();
            for s in gone {
                if let Some(d) = decks.remove(&s) {
                    let _ = d.events.blocking_send(Event::Gone);
                }
            }
        }
    }

    /// One round for one deck. False when it was unplugged or its controller stopped.
    fn service(d: &Open) -> bool {
        let mut wrote = false;
        loop {
            match d.cmds.try_recv() {
                Ok(Cmd::Image(i, img)) => {
                    if d.deck.set_button_image(i, img).is_err() {
                        return false;
                    }
                    wrote = true;
                }
                Ok(Cmd::Clear(i)) => {
                    let _ = d.deck.clear_button_image(i);
                    wrote = true;
                }
                Err(std_mpsc::TryRecvError::Empty) => break,
                Err(std_mpsc::TryRecvError::Disconnected) => {
                    let _ = d.deck.reset();
                    return false;
                }
            }
        }
        if wrote && d.deck.flush().is_err() {
            return false;
        }
        match d.reader.read(Some(Duration::from_millis(10))) {
            Ok(updates) => {
                for u in updates {
                    let e = match u {
                        DeviceStateUpdate::ButtonDown(i) => Event::Down(i),
                        DeviceStateUpdate::ButtonUp(i) => Event::Up(i),
                        _ => continue,
                    };
                    if d.events.blocking_send(e).is_err() {
                        return false;
                    }
                }
                true
            }
            Err(_) => false,
        }
    }

    struct Controller {
        app: Arc<App>,
        kind: Kind,
        device: String,
        profile: String,
        page: String,
        stack: Vec<String>,
        cmds: std_mpsc::Sender<Cmd>,
        events: mpsc::Sender<Event>,
        sent: HashMap<u8, u64>,
        fired: HashSet<String>,
        holds: HashMap<String, u64>,
        hold_gen: u64,
        clocks: bool,
    }

    impl Controller {
        fn new(app: Arc<App>, kind: Kind, serial: String, cmds: std_mpsc::Sender<Cmd>, events: mpsc::Sender<Event>) -> Self {
            let mut c = Self {
                app,
                kind,
                device: format!("hw:{serial}"),
                profile: String::new(),
                page: String::new(),
                stack: Vec::new(),
                cmds,
                events,
                sent: HashMap::new(),
                fired: HashSet::new(),
                holds: HashMap::new(),
                hold_gen: 0,
                clocks: false,
            };
            c.reset();
            c
        }

        fn reset(&mut self) {
            let e = self.app.engine.lock();
            self.profile = e.active_profile().to_string();
            self.page = e.profile(&self.profile).map(|p| p.home_page.clone()).unwrap_or_default();
            self.stack.clear();
        }

        fn grid(&self) -> (u8, u8) {
            let (rows, cols) = self.kind.key_layout();
            (cols, rows)
        }

        async fn run(mut self, mut events: mpsc::Receiver<Event>) {
            let mut changes = self.app.engine.lock().subscribe();
            let mut navs = self.app.nav.subscribe();
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            self.draw();
            loop {
                tokio::select! {
                    ev = events.recv() => match ev {
                        Some(Event::Down(i)) => self.down(i),
                        Some(Event::Up(i)) => self.up(i),
                        Some(Event::HoldDone(key, generation)) => {
                            if self.holds.get(&key) == Some(&generation) {
                                self.holds.remove(&key);
                                self.fire(&key);
                            }
                        }
                        Some(Event::Gone) | None => break,
                    },
                    c = changes.recv() => match c {
                        Ok(Change::Profile) => {
                            self.reset();
                            self.draw();
                        }
                        Ok(Change::Layout) => {
                            self.fix_position();
                            self.draw();
                        }
                        Ok(Change::Keys | Change::Vars | Change::Plugins) => self.draw(),
                        Ok(_) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => self.draw(),
                        Err(_) => break,
                    },
                    n = navs.recv() => {
                        if let Ok((device, Nav::Page(p))) = n
                            && device == self.device
                            && self.goto(&p)
                        {
                            self.draw();
                        }
                    }
                    _ = tick.tick() => {
                        if self.clocks {
                            self.draw();
                        }
                    }
                }
            }
            tracing::info!("{} disconnected", name(self.kind));
        }

        fn fix_position(&mut self) {
            let ok = {
                let e = self.app.engine.lock();
                e.page(&self.profile, &self.page).is_some()
            };
            if !ok {
                self.reset();
            }
        }

        fn goto(&mut self, page: &str) -> bool {
            let folder = {
                let e = self.app.engine.lock();
                match e.page(&self.profile, page) {
                    Some(p) => p.folder,
                    None => return false,
                }
            };
            if page == self.page {
                return false;
            }
            if folder {
                self.stack.push(self.page.clone());
            } else {
                self.stack.clear();
            }
            self.page = page.to_string();
            true
        }

        /// Key views on this page by button index. Wide keys show on their first button.
        fn layout(&self) -> HashMap<u8, KeyView> {
            let (cols, rows) = self.grid();
            let view = self.app.engine.lock().page_view(&self.profile, &self.page, FormFactor::Desktop, None);
            let mut out = HashMap::new();
            for k in view.map(|v| v.keys).unwrap_or_default() {
                if k.x < cols && k.y < rows {
                    out.insert(k.y * cols + k.x, k);
                }
            }
            out
        }

        fn draw(&mut self) {
            if !self.kind.is_visual() {
                return;
            }
            let keys = self.layout();
            let (w, h) = self.kind.key_image_format().size;
            self.clocks = keys.values().any(|k| k.since.is_some());
            for i in 0..self.kind.key_count() {
                let k = keys.get(&i);
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                serde_json::to_string(&k).unwrap_or_default().hash(&mut hasher);
                if k.is_some_and(|k| k.since.is_some()) {
                    (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()).hash(&mut hasher);
                }
                let hash = hasher.finish();
                if self.sent.get(&i) == Some(&hash) {
                    continue;
                }
                self.sent.insert(i, hash);
                let cmd = match k {
                    Some(k) => Cmd::Image(i, super::render::key(k, w as u32, h as u32)),
                    None => Cmd::Clear(i),
                };
                if self.cmds.send(cmd).is_err() {
                    return;
                }
            }
        }

        fn key_at(&self, i: u8) -> Option<crate::model::Key> {
            let id = self.layout().get(&i)?.id.clone();
            self.app.engine.lock().key(&self.profile, &self.page, &id).cloned()
        }

        fn down(&mut self, i: u8) {
            let Some(key) = self.key_at(i) else { return };
            match key.kind {
                KeyKind::Folder => {
                    if let Some(t) = &key.target
                        && self.goto(t)
                    {
                        self.draw();
                    }
                }
                KeyKind::Back => {
                    if let Some(p) = self.stack.pop() {
                        self.page = p;
                        self.draw();
                    }
                }
                KeyKind::Slider | KeyKind::Widget => {}
                _ => match key.hold.filter(|h| *h > 0) {
                    Some(ms) => {
                        self.hold_gen += 1;
                        let generation = self.hold_gen;
                        self.holds.insert(key.id.clone(), generation);
                        let tx = self.events.clone();
                        let id = key.id.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_millis(ms as u64)).await;
                            let _ = tx.send(Event::HoldDone(id, generation)).await;
                        });
                    }
                    None => self.fire(&key.id),
                },
            }
        }

        fn up(&mut self, i: u8) {
            let Some(key) = self.key_at(i) else { return };
            self.holds.remove(&key.id);
            if self.fired.remove(&key.id) {
                tokio::spawn(actions::release(self.app.clone(), self.ctx(&key.id), key.actions));
            }
        }

        fn ctx(&self, key: &str) -> Ctx {
            Ctx { device: self.device.clone(), profile: self.profile.clone(), page: self.page.clone(), key: key.to_string(), local: true }
        }

        fn fire(&mut self, id: &str) {
            let Some(key) = self.app.engine.lock().key(&self.profile, &self.page, id).cloned() else { return };
            let (list, off) = if key.kind == KeyKind::Toggle {
                let on = self.app.engine.lock().flip_toggle(&key_path(&self.profile, &self.page, id));
                if on || key.off_actions.is_empty() { (key.actions.clone(), false) } else { (key.off_actions.clone(), true) }
            } else {
                (key.actions.clone(), false)
            };
            self.app.recorder.record(&list);
            self.app.notify(Change::Pressed { profile: self.profile.clone(), page: self.page.clone(), key: id.to_string(), label: key.label.clone(), device: name(self.kind).into() });
            if list.is_empty() {
                return;
            }
            let path = key_path(&self.profile, &self.page, id);
            if !self.app.running.lock().insert(path.clone()) {
                return;
            }
            self.fired.insert(id.to_string());
            let app = self.app.clone();
            let ctx = self.ctx(id);
            let started = Instant::now();
            tokio::spawn(async move {
                actions::run_steps(app.clone(), ctx, list, off).await;
                app.running.lock().remove(&path);
                tracing::debug!("hardware key ran in {:?}", started.elapsed());
            });
        }
    }
}
