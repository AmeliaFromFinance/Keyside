//! Physical Elgato Stream Decks plugged into this PC. Each one gets its own layout the first
//! time it's plugged in, sized to its keys, and shows the active layout if that's deleted.
//! A deck can have more layouts, picked by the focused app (see `focus::deck_layout`).
//! Keys and the touch strip of the Stream Deck + are drawn here as images.
//!
//! Dials: on decks with dials, layout pages have one row more than the deck has keys. Key
//! number N of that row belongs to dial N. Turning a dial moves its slider, pressing it or
//! tapping its part of the touch strip presses the key.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::app::App;
use crate::model::{LayoutFile, Slider};

/// Share of a slider one dial click moves when the slider has no step of its own.
const DIAL_STEP: f64 = 0.02;

/// Slider position (0 to 1) after turning a dial by `ticks` clicks, positive is clockwise.
/// A slider with a step moves one step per click.
pub fn dial_value(current: f64, ticks: i8, slider: Option<&Slider>) -> f64 {
    let step = slider.and_then(|s| s.step.map(|st| st / (s.max - s.min))).filter(|st| st.is_finite() && *st > 0.0).unwrap_or(DIAL_STEP);
    let next = current.clamp(0.0, 1.0) + step * ticks as f64;
    // Rounding keeps values like 0.1 + 0.2 on the step grid.
    ((next * 1e6).round() / 1e6).clamp(0.0, 1.0)
}

/// Which dial a tap at `x` on a touch strip `len` pixels wide belongs to.
pub fn strip_segment(x: u16, len: u16, dials: u8) -> Option<u8> {
    if dials == 0 || len == 0 {
        return None;
    }
    Some(((x.min(len - 1) as u32 * dials as u32) / len as u32) as u8)
}

/// Turns a deck dark after a while without presses. The press that wakes it does nothing else.
pub struct Idle {
    last: Instant,
    asleep: bool,
}

impl Idle {
    pub fn new(now: Instant) -> Self {
        Self { last: now, asleep: false }
    }

    pub fn asleep(&self) -> bool {
        self.asleep
    }

    /// A press, turn or tap. False when it only woke the deck.
    pub fn input(&mut self, now: Instant) -> bool {
        self.last = now;
        !std::mem::replace(&mut self.asleep, false)
    }

    /// Starts the wait again, for example when the sleep time was changed.
    pub fn restart(&mut self, now: Instant) {
        self.last = now;
    }

    /// True once, when the deck should go dark now. 0 minutes: never.
    pub fn due(&mut self, now: Instant, minutes: u16) -> bool {
        if minutes == 0 || self.asleep || now.saturating_duration_since(self.last) < Duration::from_secs(minutes as u64 * 60) {
            return false;
        }
        self.asleep = true;
        true
    }
}

/// Gives deck layouts made before dials were supported their dial row. True when changed.
pub fn add_dial_rows(layout: &mut LayoutFile, serial: &str) -> bool {
    let mut changed = false;
    for p in &mut layout.profiles {
        let Some(d) = p.deck.as_ref().filter(|d| d.serial == serial && d.dials() > 0) else { continue };
        let rows = d.rows;
        for pg in p.pages.iter_mut().filter(|pg| pg.rows == rows && rows < crate::model::MAX_GRID) {
            pg.rows += 1;
            changed = true;
        }
    }
    changed
}

/// Watches for Stream Decks being plugged in. Does nothing without the "hardware" feature.
pub fn spawn(app: Arc<App>) {
    #[cfg(feature = "hardware")]
    imp::spawn(app);
    #[cfg(not(feature = "hardware"))]
    let _ = app;
}

/// Gives a deck seen for the first time its own layout, sized to its keys. Decks seen before
/// are left alone, so a layout someone deleted stays deleted. True when one was made.
pub fn layout_for_new_deck(app: &App, model: &str, serial: &str, cols: u8, rows: u8) -> bool {
    use crate::model::{DeckLink, Page, Profile};
    let file = app.paths.file("known-decks.json");
    let mut known: Vec<String> = crate::store::read_json(&file).ok().flatten().unwrap_or_default();
    if known.iter().any(|k| k == serial) {
        let mut e = app.engine.lock();
        let mut layout = e.layout().clone();
        if add_dial_rows(&mut layout, serial) && crate::model::validate(&layout).is_ok() {
            let _ = e.replace_layout(layout);
        }
        return false;
    }
    known.push(serial.to_string());
    let _ = crate::store::write_json(&file, &known);

    let mut e = app.engine.lock();
    let mut layout = e.layout().clone();
    if layout.profiles.iter().any(|p| p.deck.as_ref().is_some_and(|d| d.serial == serial)) {
        return false;
    }
    let mut name = model.to_string();
    let mut n = 2;
    while layout.profiles.iter().any(|p| p.name == name) {
        name = format!("{model} {n}");
        n += 1;
    }
    let dial_row = u8::from(crate::model::model_dials(model) > 0);
    let page = Page { id: "main".into(), name: "Main".into(), subtitle: None, folder: false, cols, rows: rows + dial_row, keys: Vec::new(), variants: Default::default() };
    layout.profiles.push(Profile {
        id: format!("deck-{}", crate::ids::random_b64(6)),
        name: name.clone(),
        apps: Vec::new(),
        home_page: "main".into(),
        pages: vec![page],
        deck: Some(DeckLink { serial: serial.into(), model: model.into(), cols, rows, brightness: None, sleep: None }),
    });
    if crate::model::validate(&layout).is_err() || e.replace_layout(layout).is_err() {
        return false;
    }
    e.toast(format!("Made the layout \"{name}\" for your {model}"), crate::model::Tone::Success);
    true
}

/// Hands the decks back on quit: keys go blank and show the Elgato logo again.
pub async fn release() {
    #[cfg(feature = "hardware")]
    imp::release().await;
}

#[cfg(feature = "render")]
pub mod render;

#[cfg(feature = "hardware")]
mod imp {
    use std::collections::{HashMap, HashSet};
    use std::hash::{Hash, Hasher};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc as std_mpsc;
    use std::time::{Duration, Instant};

    use elgato_streamdeck::info::{ImageRotation, Kind};
    use elgato_streamdeck::{DeviceStateUpdate, StreamDeck};
    use image::DynamicImage;
    use serde_json::Value;
    use tokio::sync::mpsc;

    use super::Idle;
    use crate::actions::{self, Ctx, Nav};
    use crate::app::App;
    use crate::model::{DeckLink, FormFactor, KeyKind, Tone};
    use crate::state::view::KeyView;
    use crate::state::{Change, key_path};

    const SCAN_EVERY: Duration = Duration::from_secs(3);
    /// A value turned in on the deck wins over the one the key reports for this long, so
    /// quick turns don't jump back while a plugin catches up.
    const TURNED_FRESH: Duration = Duration::from_secs(2);

    enum Cmd {
        Image(u8, DynamicImage),
        Clear(u8),
        Strip(DynamicImage),
        Brightness(u8),
    }

    /// Input from a deck. Dials and the touch strip use button numbers after the keys:
    /// dial N is button `key_count + N`.
    #[derive(Debug, PartialEq)]
    enum Event {
        Down(u8),
        Up(u8),
        Turn(u8, i8),
        Tap(u8),
        HoldDone(String, u64, u8),
        Gone,
    }

    /// Maps what the deck reports to an event. `strip` is the long side of the touch strip.
    fn event_for(u: DeviceStateUpdate, keys: u8, dials: u8, strip: u16) -> Option<Event> {
        let tap = |x: u16| super::strip_segment(x, strip, dials).map(|d| Event::Tap(keys + d));
        match u {
            DeviceStateUpdate::ButtonDown(i) if i < keys => Some(Event::Down(i)),
            DeviceStateUpdate::ButtonUp(i) if i < keys => Some(Event::Up(i)),
            DeviceStateUpdate::EncoderDown(d) if d < dials => Some(Event::Down(keys + d)),
            DeviceStateUpdate::EncoderUp(d) if d < dials => Some(Event::Up(keys + d)),
            DeviceStateUpdate::EncoderTwist(d, n) if d < dials && n != 0 => Some(Event::Turn(keys + d, n)),
            DeviceStateUpdate::TouchScreenPress(x, _) | DeviceStateUpdate::TouchScreenLongPress(x, _) => tap(x),
            _ => None,
        }
    }

    /// Size of the touch strip image before the device turns it, landscape.
    fn strip_size(kind: Kind) -> Option<(u32, u32)> {
        let f = kind.lcd_image_format().filter(|_| kind.encoder_count() > 0)?;
        let (w, h) = (f.size.0 as u32, f.size.1 as u32);
        Some(match f.rotation {
            ImageRotation::Rot90 | ImageRotation::Rot270 => (h, w),
            _ => (w, h),
        })
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

    static RUNNING: AtomicBool = AtomicBool::new(false);
    static STOP: AtomicBool = AtomicBool::new(false);

    pub fn spawn(app: Arc<App>) {
        let rt = tokio::runtime::Handle::current();
        RUNNING.store(true, Ordering::SeqCst);
        if std::thread::Builder::new().name("stream-deck-scan".into()).spawn(move || scan(app, rt)).is_err() {
            RUNNING.store(false, Ordering::SeqCst);
        }
    }

    pub async fn release() {
        STOP.store(true, Ordering::SeqCst);
        let end = Instant::now() + Duration::from_millis(1500);
        while RUNNING.load(Ordering::SeqCst) && Instant::now() < end {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
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
                RUNNING.store(false, Ordering::SeqCst);
                return;
            }
        };
        let mut decks: HashMap<String, Open> = HashMap::new();
        let mut last_scan: Option<Instant> = None;
        loop {
            if STOP.load(Ordering::SeqCst) {
                for (_, d) in decks.drain() {
                    let _ = d.deck.reset();
                }
                RUNNING.store(false, Ordering::SeqCst);
                return;
            }
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
                            // Only this thread touches the device; the reader API needs an Arc.
                            #[allow(clippy::arc_with_non_send_sync)]
                            Ok(d) => Arc::new(d),
                            Err(e) => {
                                tracing::warn!("couldn't open {} ({serial}): {e}", name(kind));
                                continue;
                            }
                        };
                        let _ = deck.clear_all_button_images();
                        let (rows, cols) = kind.key_layout();
                        super::layout_for_new_deck(&app, name(kind), &serial, cols, rows);
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
        let kind = d.deck.kind();
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
                Ok(Cmd::Strip(img)) => {
                    let Some(format) = kind.lcd_image_format() else { continue };
                    if let Ok(data) = elgato_streamdeck::images::convert_image_with_format(format, img)
                        && d.deck.write_lcd_fill(&data).is_err()
                    {
                        return false;
                    }
                }
                Ok(Cmd::Brightness(b)) => {
                    let _ = d.deck.set_brightness(b);
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
        let strip = kind.lcd_strip_size().map(|(w, h)| w.max(h) as u16).unwrap_or(0);
        match d.reader.read(Some(Duration::from_millis(10))) {
            Ok(updates) => {
                for u in updates {
                    let Some(e) = event_for(u, kind.key_count(), kind.encoder_count(), strip) else { continue };
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
        serial: String,
        profile: String,
        page: String,
        stack: Vec<String>,
        cmds: std_mpsc::Sender<Cmd>,
        events: mpsc::Sender<Event>,
        sent: HashMap<u8, u64>,
        strip_sent: Option<u64>,
        presses: actions::Presses<u8>,
        holds: HashMap<String, u64>,
        hold_gen: u64,
        clocks: bool,
        /// Slider positions turned in on this deck, by key path, and when.
        turned: HashMap<String, (f64, Instant)>,
        idle: Idle,
        /// Buttons whose press only woke the deck, so their release is ignored too.
        woke_by: HashSet<u8>,
        brightness: Option<u8>,
        sleep: u16,
    }

    impl Controller {
        fn new(app: Arc<App>, kind: Kind, serial: String, cmds: std_mpsc::Sender<Cmd>, events: mpsc::Sender<Event>) -> Self {
            let mut c = Self {
                app,
                kind,
                device: format!("hw:{serial}"),
                serial,
                profile: String::new(),
                page: String::new(),
                stack: Vec::new(),
                cmds,
                events,
                sent: HashMap::new(),
                strip_sent: None,
                presses: actions::Presses::default(),
                holds: HashMap::new(),
                hold_gen: 0,
                clocks: false,
                turned: HashMap::new(),
                idle: Idle::new(Instant::now()),
                woke_by: HashSet::new(),
                brightness: None,
                sleep: 0,
            };
            c.reset();
            c.apply_settings();
            c
        }

        /// The layout made for this deck that fits the focused app, if there is one.
        fn own_profile(&self) -> Option<String> {
            let e = self.app.engine.lock();
            let focused = e.var("core.app").and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
            crate::focus::deck_layout(e.layout(), &self.serial, &focused)
        }

        fn link(&self) -> Option<DeckLink> {
            let e = self.app.engine.lock();
            e.profile(&self.profile).and_then(|p| p.deck.clone()).filter(|d| d.serial == self.serial)
        }

        fn reset(&mut self) {
            let own = self.own_profile();
            let e = self.app.engine.lock();
            self.profile = own.unwrap_or_else(|| e.active_profile().to_string());
            self.page = e.profile(&self.profile).map(|p| p.home_page.clone()).unwrap_or_default();
            self.stack.clear();
        }

        /// Brightness and sleep time of the layout shown. Others use the defaults.
        fn apply_settings(&mut self) {
            let link = self.link();
            let sleep = link.as_ref().map_or(0, DeckLink::sleep_minutes);
            if sleep != self.sleep {
                self.sleep = sleep;
                self.idle.restart(Instant::now());
            }
            let want = if self.idle.asleep() { 0 } else { link.as_ref().map_or(DeckLink::BRIGHTNESS, DeckLink::brightness) };
            if self.brightness != Some(want) && self.cmds.send(Cmd::Brightness(want)).is_ok() {
                self.brightness = Some(want);
            }
        }

        /// A press, turn or tap. False when it only woke the deck.
        fn input(&mut self, button: u8) -> bool {
            if self.idle.input(Instant::now()) {
                return true;
            }
            self.woke_by.insert(button);
            self.apply_settings();
            false
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
                        Some(Event::Down(i)) => {
                            if self.input(i) {
                                self.down(i);
                            }
                        }
                        Some(Event::Up(i)) => {
                            if !self.woke_by.remove(&i) {
                                self.up(i);
                            }
                        }
                        // No release follows a turn or a tap.
                        Some(Event::Turn(i, ticks)) => {
                            let ok = self.input(i);
                            self.woke_by.remove(&i);
                            if ok {
                                self.turn(i, ticks);
                            }
                        }
                        Some(Event::Tap(i)) => {
                            let ok = self.input(i);
                            self.woke_by.remove(&i);
                            if ok {
                                self.down(i);
                                self.up(i);
                            }
                        }
                        Some(Event::HoldDone(key, generation, button)) => {
                            if self.holds.get(&key) == Some(&generation) {
                                self.holds.remove(&key);
                                self.fire(&key, button);
                            }
                        }
                        Some(Event::Gone) | None => break,
                    },
                    c = changes.recv() => match c {
                        // A deck with its own layout keeps it when the active one changes.
                        Ok(Change::Profile) if self.own_profile().is_none() => {
                            self.reset();
                            self.apply_settings();
                            self.draw();
                        }
                        // The focused app is a variable, and may pick another layout.
                        Ok(Change::Layout | Change::Vars) => {
                            self.fix_position();
                            self.apply_settings();
                            self.draw();
                        }
                        Ok(Change::Keys | Change::Plugins) => self.draw(),
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
                        if self.idle.due(Instant::now(), self.sleep) {
                            self.apply_settings();
                        }
                        if self.clocks {
                            self.draw();
                        }
                    }
                }
            }
            self.presses.release_all(&self.app);
            tracing::info!("{} disconnected", name(self.kind));
        }

        fn fix_position(&mut self) {
            let ok = {
                let e = self.app.engine.lock();
                e.page(&self.profile, &self.page).is_some()
            };
            // Also switch when this deck's own layout was added or removed, or another one
            // of its layouts fits the focused app now.
            let own = self.own_profile();
            let moved = match &own {
                Some(id) => *id != self.profile,
                None => self.app.engine.lock().profile(&self.profile).is_some_and(|p| p.deck.is_some()),
            };
            if !ok || moved {
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

        /// Key views on this page by button number. Wide keys show on their first button.
        /// Keys in the row below the key rows belong to the dials, numbered after the keys.
        fn layout(&self) -> HashMap<u8, KeyView> {
            let (cols, rows) = self.grid();
            let (keys, dials) = (self.kind.key_count(), self.kind.encoder_count());
            let view = self.app.engine.lock().page_view(&self.profile, &self.page, FormFactor::Desktop, None);
            let mut out = HashMap::new();
            for k in view.map(|v| v.keys).unwrap_or_default() {
                if k.x < cols && k.y < rows {
                    out.insert(k.y * cols + k.x, k);
                } else if k.y == rows && k.x < dials && k.w == 1 && k.h == 1 {
                    out.insert(keys + k.x, k);
                }
            }
            out
        }

        fn draw(&mut self) {
            if !self.kind.is_visual() {
                return;
            }
            let mut keys = self.layout();
            let (w, h) = self.kind.key_image_format().size;
            self.clocks = keys.values().any(|k| k.since.is_some());
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
            let hash = |k: Option<&KeyView>| {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                serde_json::to_string(&k).unwrap_or_default().hash(&mut hasher);
                if k.is_some_and(|k| k.since.is_some()) {
                    now.hash(&mut hasher);
                }
                hasher.finish()
            };
            for i in 0..self.kind.key_count() {
                let k = keys.get(&i);
                let hash = hash(k);
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
            let Some((sw, sh)) = strip_size(self.kind) else { return };
            let first = self.kind.key_count();
            let dials: Vec<Option<KeyView>> = (0..self.kind.encoder_count())
                .map(|d| {
                    let mut v = keys.remove(&(first + d))?;
                    if v.kind == KeyKind::Slider {
                        v.value = self.slider_value(&v.id, v.value);
                    }
                    Some(v)
                })
                .collect();
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            serde_json::to_string(&dials).unwrap_or_default().hash(&mut hasher);
            if dials.iter().flatten().any(|k| k.since.is_some()) {
                now.hash(&mut hasher);
            }
            let hash = hasher.finish();
            if self.strip_sent != Some(hash) {
                self.strip_sent = Some(hash);
                let _ = self.cmds.send(Cmd::Strip(super::render::strip(&dials, sw, sh)));
            }
        }

        fn key_at(&self, i: u8) -> Option<crate::model::Key> {
            let id = self.layout().get(&i)?.id.clone();
            self.app.engine.lock().key(&self.profile, &self.page, &id).cloned()
        }

        /// Where a slider stands: the value turned in here moments ago, the one the key
        /// reports, the variable it sets, or the value last turned in.
        fn slider_value(&self, id: &str, reported: Option<f64>) -> Option<f64> {
            let path = key_path(&self.profile, &self.page, id);
            let turned = self.turned.get(&path);
            if let Some((v, at)) = turned
                && at.elapsed() < TURNED_FRESH
            {
                return Some(*v);
            }
            reported.or_else(|| self.var_value(id)).or(turned.map(|(v, _)| *v))
        }

        /// Position of a slider that sets a variable, from the value of that variable.
        fn var_value(&self, id: &str) -> Option<f64> {
            let e = self.app.engine.lock();
            let key = e.key(&self.profile, &self.page, id)?;
            let name = key.actions.iter().find(|a| a.kind == "core.var")?.settings.get("name").and_then(Value::as_str)?;
            let n = e.var(name)?.as_f64()?;
            let (min, max) = key.slider.as_ref().map_or((0.0, 1.0), |s| (s.min, s.max));
            Some(((n - min) / (max - min)).clamp(0.0, 1.0))
        }

        /// A dial was turned: its slider moves the way a phone slider does.
        fn turn(&mut self, i: u8, ticks: i8) {
            let Some(key) = self.key_at(i) else { return };
            if key.kind != KeyKind::Slider {
                return;
            }
            let reported = self.layout().get(&i).and_then(|v| v.value);
            let current = self.slider_value(&key.id, reported).unwrap_or(0.0);
            let value = super::dial_value(current, ticks, key.slider.as_ref());
            let path = key_path(&self.profile, &self.page, &key.id);
            self.turned.insert(path.clone(), (value, Instant::now()));
            let (profile, page) = (self.profile.clone(), self.page.clone());
            actions::slide_one_at_a_time(&self.app, path, value, move |app, v| {
                let (profile, page, key) = (profile.clone(), page.clone(), key.clone());
                async move { crate::server::conn::apply_slide(&app, &profile, &page, &key.id, &key, v).await }
            });
            self.draw();
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
                _ if self.presses.is_down(&i) => {}
                _ => {
                    self.presses.down(&self.app, i, key_path(&self.profile, &self.page, &key.id), key.repeat.is_some());
                    match key.hold.filter(|h| *h > 0) {
                        Some(ms) => {
                            self.hold_gen += 1;
                            let generation = self.hold_gen;
                            self.holds.insert(key.id.clone(), generation);
                            let tx = self.events.clone();
                            let id = key.id.clone();
                            tokio::spawn(async move {
                                tokio::time::sleep(Duration::from_millis(ms as u64)).await;
                                let _ = tx.send(Event::HoldDone(id, generation, i)).await;
                            });
                        }
                        None => self.fire(&key.id, i),
                    }
                }
            }
        }

        fn up(&mut self, i: u8) {
            self.presses.up(&self.app, &i);
            if let Some(key) = self.key_at(i) {
                self.holds.remove(&key.id);
            }
        }

        fn ctx(&self, key: &str) -> Ctx {
            Ctx { device: self.device.clone(), profile: self.profile.clone(), page: self.page.clone(), key: key.to_string(), local: true }
        }

        fn fire(&mut self, id: &str, button: u8) {
            let Some(key) = self.app.engine.lock().key(&self.profile, &self.page, id).cloned() else { return };
            let (list, off) = if key.kind == KeyKind::Toggle {
                let on = self.app.engine.lock().flip_toggle(&key_path(&self.profile, &self.page, id));
                if on || key.off_actions.is_empty() { (key.actions.clone(), false) } else { (key.off_actions.clone(), true) }
            } else {
                (key.actions.clone(), false)
            };
            self.app.recorder.record(&list);
            self.app.notify(Change::Pressed {
                profile: self.profile.clone(),
                page: self.page.clone(),
                key: id.to_string(),
                label: key.label.clone(),
                device: name(self.kind).into(),
            });
            if list.is_empty() {
                return;
            }
            let Some(running) = actions::Running::claim(&self.app, key_path(&self.profile, &self.page, id)) else { return };
            let app = self.app.clone();
            let ctx = self.ctx(id);
            self.presses.fired(&button, ctx.clone(), key.actions);
            let started = Instant::now();
            let repeat = key.repeat;
            tokio::spawn(async move {
                actions::run_key(app, ctx, list, off, repeat).await;
                drop(running);
                tracing::debug!("hardware key ran in {:?}", started.elapsed());
            });
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn deck_input_maps_to_buttons_and_dials() {
            // A Stream Deck +: 8 keys, 4 dials, an 800 pixel strip.
            let ev = |u| event_for(u, 8, 4, 800);
            assert_eq!(ev(DeviceStateUpdate::ButtonDown(3)), Some(Event::Down(3)));
            assert_eq!(ev(DeviceStateUpdate::ButtonUp(7)), Some(Event::Up(7)));
            assert_eq!(ev(DeviceStateUpdate::EncoderDown(0)), Some(Event::Down(8)));
            assert_eq!(ev(DeviceStateUpdate::EncoderUp(3)), Some(Event::Up(11)));
            assert_eq!(ev(DeviceStateUpdate::EncoderTwist(1, -2)), Some(Event::Turn(9, -2)));
            assert_eq!(ev(DeviceStateUpdate::EncoderTwist(1, 0)), None);
            assert_eq!(ev(DeviceStateUpdate::EncoderDown(4)), None, "no such dial");
            assert_eq!(ev(DeviceStateUpdate::TouchScreenPress(0, 50)), Some(Event::Tap(8)));
            assert_eq!(ev(DeviceStateUpdate::TouchScreenPress(450, 50)), Some(Event::Tap(10)));
            assert_eq!(ev(DeviceStateUpdate::TouchScreenLongPress(799, 50)), Some(Event::Tap(11)));
            assert_eq!(ev(DeviceStateUpdate::TouchScreenSwipe((0, 0), (300, 0))), None);
            // Decks without dials ignore the rest.
            assert_eq!(event_for(DeviceStateUpdate::TouchScreenPress(10, 10), 15, 0, 0), None);
            assert_eq!(event_for(DeviceStateUpdate::ButtonDown(14), 15, 0, 0), Some(Event::Down(14)));
        }

        #[test]
        fn strip_images_are_drawn_wide() {
            assert_eq!(strip_size(Kind::Plus), Some((800, 100)));
            assert_eq!(strip_size(Kind::PlusXl), Some((1200, 100)));
            assert_eq!(strip_size(Kind::Neo), None, "the Neo has no dials");
            assert_eq!(strip_size(Kind::Xl), None);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn new_decks_get_one_layout_once() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::app::App::new(crate::app::Options { data_dir: Some(dir.path().into()), bundled_plugins: None }).unwrap();
        let before = app.engine.lock().layout().profiles.len();
        assert!(super::layout_for_new_deck(&app, "Stream Deck Mini", "AB12", 3, 2));
        let made = app.engine.lock().layout().profiles.last().cloned().unwrap();
        assert_eq!((made.name.as_str(), made.pages[0].cols, made.pages[0].rows), ("Stream Deck Mini", 3, 2));
        assert_eq!(made.deck.unwrap().serial, "AB12");
        assert!(!super::layout_for_new_deck(&app, "Stream Deck Mini", "AB12", 3, 2), "only once per deck");

        // A second Mini gets its own, and a deleted layout isn't made again.
        assert!(super::layout_for_new_deck(&app, "Stream Deck Mini", "CD34", 3, 2));
        assert_eq!(app.engine.lock().layout().profiles.last().unwrap().name, "Stream Deck Mini 2");
        let mut l = app.engine.lock().layout().clone();
        l.profiles.retain(|p| p.deck.is_none());
        app.engine.lock().replace_layout(l).unwrap();
        assert!(!super::layout_for_new_deck(&app, "Stream Deck Mini", "AB12", 3, 2));
        assert_eq!(app.engine.lock().layout().profiles.len(), before);
    }

    #[test]
    fn decks_with_dials_get_a_dial_row() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::app::App::new(crate::app::Options { data_dir: Some(dir.path().into()), bundled_plugins: None }).unwrap();
        assert!(super::layout_for_new_deck(&app, "Stream Deck +", "PL1", 4, 2));
        let made = app.engine.lock().layout().profiles.last().cloned().unwrap();
        assert_eq!((made.pages[0].cols, made.pages[0].rows), (4, 3));
        assert_eq!(made.deck.as_ref().unwrap().dials(), 4);

        // A layout made before dials existed gets the row the next time the deck connects.
        let mut l = app.engine.lock().layout().clone();
        l.profiles.last_mut().unwrap().pages[0].rows = 2;
        app.engine.lock().replace_layout(l).unwrap();
        assert!(!super::layout_for_new_deck(&app, "Stream Deck +", "PL1", 4, 2));
        assert_eq!(app.engine.lock().layout().profiles.last().unwrap().pages[0].rows, 3);
    }

    #[test]
    fn dial_keys_must_sit_on_a_dial() {
        let mut l = crate::state::default_layout();
        let p = &mut l.profiles[0];
        p.deck = Some(serde_json::from_value(serde_json::json!({ "serial": "S", "model": "Stream Deck +", "cols": 4, "rows": 2, "brightness": 40, "sleep": 5 })).unwrap());
        p.pages.truncate(1);
        p.home_page = p.pages[0].id.clone();
        let pg = &mut p.pages[0];
        (pg.cols, pg.rows) = (4, 3);
        pg.variants.clear();
        let mut k: crate::model::Key =
            serde_json::from_value(serde_json::json!({ "id": "d", "x": 3, "y": 2, "kind": "slider", "label": "Vol", "slider": { "min": 0, "max": 100 } })).unwrap();
        pg.keys = vec![k.clone()];
        crate::model::validate(&l).unwrap();
        k.y = 1;
        k.h = Some(2);
        l.profiles[0].pages[0].keys = vec![k];
        assert!(crate::model::validate(&l).is_err(), "a tall key can't reach into the dial row");
        l.profiles[0].pages[0].keys.clear();
        l.profiles[0].deck.as_mut().unwrap().brightness = Some(101);
        assert!(crate::model::validate(&l).is_err());
    }

    #[test]
    fn dials_step_sliders() {
        use super::dial_value;
        let s = |min: f64, max: f64, step: Option<f64>| crate::model::Slider { min, max, step };
        assert_eq!(dial_value(0.5, 1, None), 0.52);
        assert_eq!(dial_value(0.5, -3, None), 0.44);
        assert_eq!(dial_value(0.99, 5, None), 1.0, "stops at the ends");
        assert_eq!(dial_value(0.01, -5, None), 0.0);
        assert_eq!(dial_value(0.5, 1, Some(&s(0.0, 100.0, Some(5.0)))), 0.55, "one slider step per click");
        assert_eq!(dial_value(0.5, -2, Some(&s(0.0, 10.0, None))), 0.46);
        assert_eq!(super::strip_segment(199, 800, 4), Some(0));
        assert_eq!(super::strip_segment(200, 800, 4), Some(1));
        assert_eq!(super::strip_segment(5000, 800, 4), Some(3));
        assert_eq!(super::strip_segment(10, 800, 0), None);
    }

    #[test]
    fn idle_decks_sleep_and_the_waking_press_does_nothing() {
        use std::time::{Duration, Instant};
        let t0 = Instant::now();
        let min = |m: u64| t0 + Duration::from_secs(m * 60);
        let mut idle = super::Idle::new(t0);
        assert!(!idle.due(min(100), 0), "0 minutes: never");
        assert!(!idle.due(min(4), 5));
        assert!(idle.input(min(4)), "a press while awake counts");
        assert!(!idle.due(min(8), 5), "the press started the wait again");
        assert!(idle.due(min(9), 5));
        assert!(idle.asleep());
        assert!(!idle.due(min(20), 5), "goes dark once");
        assert!(!idle.input(min(21)), "the waking press only wakes");
        assert!(!idle.asleep());
        assert!(idle.input(min(21)), "the next one counts");
    }
}
