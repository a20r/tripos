//! The game: three modes over one table, rendered straight into the DOM.
//!
//! Solo and Daily race the clock through a whole deck; Daily uses a seeded
//! deck shared by everyone that day. Party is for a tablet on a table: each
//! player has a bar on their edge, taps it to call a set, and then has a few
//! seconds to pick the three cards.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::{Rc, Weak};

use tripos_engine::speed::{self, SpeedStats, Trial};
use tripos_engine::{daily, Card, Claim, Game, Rng, Stats};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Element, HtmlButtonElement, HtmlElement, KeyboardEvent, MouseEvent};

use crate::cards;
use crate::dom;
use crate::net::{self, Net, NetEvent, Role};
use crate::storage::*;

const SAVE_V: u32 = 1;
const WRONG_MS: u32 = 10_000;
const HINT_MS: u32 = 15_000;
const BAD_DEAL_MS: u32 = 15_000;
/// How long a found set glows before its cards are replaced.
const GOOD_MS: i32 = 420;
/// How long a wrong pick shakes before the selection clears.
const BAD_MS: i32 = 480;
const BUZZ_MS: f64 = 7000.0;
const TICK_MS: i32 = 100;
const MIDDOT: &str = " \u{b7} ";
/// Keyboard row mapped onto table positions, for desktop play.
const KEY_ROW: &str = "qwertyuiasdfghjkzxcvbnm";
/// A category needs this many observations before the profile calls it a lean or a miss.
const MIN_SEEN: u32 = 6;
/// A lean or miss needs this much distance from the overall take rate, in rate points.
const MIN_BIAS: f64 = 0.08;
/// Fastest and slowest kinds need this many finds.
const MIN_FOUND: u32 = 3;
const PLAYER_NAMES: [&str; 4] = ["P1", "P2", "P3", "P4"];
const WIN_LINES: [&str; 4] = [
    "MORE HUMAN THAN HUMAN",
    "CELLS INTERLINKED",
    "A FINE PIECE OF WORK",
    "BASELINE: WITHIN NORMAL",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Solo,
    Party,
    Daily,
    Online,
    Speed,
}

impl Mode {
    fn key(self) -> &'static str {
        match self {
            Mode::Solo => "solo",
            Mode::Party => "party",
            Mode::Daily => "daily",
            Mode::Online => "online",
            Mode::Speed => "speed",
        }
    }
    fn from_key(s: &str) -> Mode {
        match s {
            "party" => Mode::Party,
            "daily" => Mode::Daily,
            "online" => Mode::Online,
            "speed" => Mode::Speed,
            _ => Mode::Solo,
        }
    }
    /// Solo and Daily race the table clock; Party and Online keep score; Speed has its own clock.
    fn is_timed(self) -> bool {
        !matches!(self, Mode::Party | Mode::Online | Mode::Speed)
    }
}

const MODES: [Mode; 5] = [
    Mode::Solo,
    Mode::Party,
    Mode::Daily,
    Mode::Online,
    Mode::Speed,
];

/// How far a swipe must travel to count as an answer.
const SWIPE_PX: f64 = 70.0;

/// The speed drill: a stack of triples answered one at a time against a clock.
struct Speed {
    /// Empty placeholder so the shared table code has something to read.
    game: Game,
    trials: Vec<Trial>,
    pos: usize,
    started: bool,
    over: bool,
    start_at: f64,
    shown_at: f64,
    penalty: u32,
    correct: u32,
    rts: Vec<u32>,
    total: u32,
}

impl Speed {
    fn idle() -> Speed {
        Speed {
            game: Game::from_deck(Vec::new()),
            trials: Vec::new(),
            pos: 0,
            started: false,
            over: false,
            start_at: 0.0,
            shown_at: 0.0,
            penalty: 0,
            correct: 0,
            rts: Vec::new(),
            total: 0,
        }
    }

    fn clock(&self) -> u32 {
        if !self.started {
            return 0;
        }
        if self.over {
            return self.total;
        }
        ((dom::now() - self.start_at).max(0.0) as u32).saturating_add(self.penalty)
    }
}

/// What travels over the data channel. The host owns the game: guests ask,
/// the host applies and broadcasts, and both sides replay the same moves on
/// the same seeded deck. Claims name cards, not positions, so each side may
/// shuffle its own table.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "t")]
enum Wire {
    Start { seed: u64 },
    Claim { cards: [u8; 3] },
    Claimed { by: u8, cards: [u8; 3] },
    Lost,
    Wrong { by: u8 },
    Deal,
    Dealt,
    NoDeal,
}

struct Online {
    game: Game,
    scores: [i32; 2],
    /// 0 for the host, 1 for the guest.
    me: usize,
    started: bool,
    over: bool,
    /// A guest claim awaiting the host's verdict.
    pending: Option<[Card; 3]>,
    net: Option<Rc<Net>>,
    mic: Option<bool>,
}

impl Online {
    fn idle() -> Online {
        Online {
            game: Game::from_deck(Vec::new()),
            scores: [0; 2],
            me: 0,
            started: false,
            over: false,
            pending: None,
            net: None,
            mic: None,
        }
    }
}

/// Visual theme: the Blade Runner neon look, a clean take on the original
/// card game, or an ultra-dark night mode for playing with the lights off.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Neon,
    Classic,
    Night,
}

impl Theme {
    fn key(self) -> &'static str {
        match self {
            Theme::Neon => "neon",
            Theme::Classic => "classic",
            Theme::Night => "night",
        }
    }
    fn from_key(s: &str) -> Theme {
        match s {
            "classic" => Theme::Classic,
            "night" => Theme::Night,
            _ => Theme::Neon,
        }
    }
    /// Browser chrome colour (the theme-color meta) for each theme.
    fn chrome(self) -> &'static str {
        match self {
            Theme::Neon => "#07020f",
            Theme::Classic => "#f6f4ee",
            Theme::Night => "#000000",
        }
    }
    /// The theme the T key cycles to, in switch order.
    fn next(self) -> Theme {
        match self {
            Theme::Neon => Theme::Classic,
            Theme::Classic => Theme::Night,
            Theme::Night => Theme::Neon,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Theme::Neon => "Neon theme.",
            Theme::Classic => "Classic theme.",
            Theme::Night => "Night theme.",
        }
    }
    /// Colour names for screen readers, matching the stylesheet's hues.
    fn color_names(self) -> [&'static str; 3] {
        match self {
            Theme::Neon => ["pink", "cyan", "violet"],
            Theme::Classic => ["red", "green", "purple"],
            Theme::Night => ["amber", "sage", "rose"],
        }
    }
}

const THEMES: [Theme; 3] = [Theme::Neon, Theme::Classic, Theme::Night];

/// A solo or daily game with its clock. Penalties are added straight to the
/// elapsed time and also counted so the summary can show them.
#[derive(Clone)]
struct Timed {
    game: Game,
    date: Option<String>,
    elapsed: u32,
    since: Option<f64>,
    penalty: u32,
    hints: u32,
    over: bool,
    /// Play time when the table last changed, so a find can be timed.
    mark: u32,
}

impl Timed {
    fn new(game: Game, date: Option<String>) -> Timed {
        Timed {
            game,
            date,
            elapsed: 0,
            since: None,
            penalty: 0,
            hints: 0,
            over: false,
            mark: 0,
        }
    }

    /// Time spent looking, with penalties taken back out.
    fn play_ms(&self) -> u32 {
        self.total().saturating_sub(self.penalty)
    }

    fn mark_table(&mut self) {
        self.mark = self.play_ms();
    }

    /// How long the current table has been in front of the player.
    fn search_ms(&self) -> u32 {
        self.play_ms().saturating_sub(self.mark)
    }

    fn total(&self) -> u32 {
        let run = self
            .since
            .map(|s| (dom::now() - s).max(0.0) as u32)
            .unwrap_or(0);
        self.elapsed.saturating_add(run)
    }

    fn start(&mut self) {
        if self.since.is_none() && !self.over {
            self.since = Some(dom::now());
        }
    }

    fn pause(&mut self) {
        if let Some(s) = self.since.take() {
            self.elapsed = self
                .elapsed
                .saturating_add((dom::now() - s).max(0.0) as u32);
        }
    }

    fn penalize(&mut self, ms: u32) {
        self.elapsed = self.elapsed.saturating_add(ms);
        self.penalty = self.penalty.saturating_add(ms);
    }

    fn to_save(&self) -> TimedSave {
        TimedSave {
            v: SAVE_V,
            date: self.date.clone(),
            deck: self.game.deck().iter().map(|c| c.0).collect(),
            table: self.game.table().iter().map(|c| c.0).collect(),
            found: self.game.found(),
            elapsed: self.total(),
            penalty: self.penalty,
            hints: self.hints,
            over: self.over,
        }
    }

    fn from_save(s: TimedSave) -> Option<Timed> {
        if s.v != SAVE_V {
            return None;
        }
        let deck: Vec<Card> = s.deck.iter().map(|&c| Card(c)).collect();
        let table: Vec<Card> = s.table.iter().map(|&c| Card(c)).collect();
        let all: HashSet<Card> = deck.iter().chain(table.iter()).copied().collect();
        if all.len() != deck.len() + table.len()
            || all.iter().any(|c| !c.is_valid())
            || (deck.len() + table.len() + 3 * s.found as usize) != tripos_engine::DECK_SIZE
        {
            return None;
        }
        Some(Timed {
            game: Game::restore(deck, table, s.found),
            date: s.date,
            elapsed: s.elapsed,
            since: None,
            penalty: s.penalty,
            hints: s.hints,
            over: s.over,
            mark: s.elapsed.saturating_sub(s.penalty),
        })
    }
}

struct Party {
    game: Game,
    players: usize,
    scores: [i32; 4],
    holder: Option<usize>,
    deadline: f64,
    over: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Overlay {
    None,
    Rules,
    Done,
    /// The speed drill's GO card.
    SpeedStart,
}

struct Els {
    app: HtmlElement,
    table: HtmlElement,
    stat_a: HtmlElement,
    stat_b: HtmlElement,
    stat_c: HtmlElement,
    stat_a_label: HtmlElement,
    stat_b_label: HtmlElement,
    stat_c_label: HtmlElement,
    status: HtmlElement,
    status_line: HtmlElement,
    tagline: HtmlElement,
    hint: HtmlButtonElement,
    deal: HtmlButtonElement,
    shuffle: HtmlButtonElement,
    stats_btn: HtmlButtonElement,
    table_wrap: HtmlElement,
    stats_panel: HtmlElement,
    stats_body: HtmlElement,
    options_btn: HtmlButtonElement,
    options: HtmlElement,
    options_scrim: HtmlElement,
    options_summary: HtmlElement,
    new_game: HtmlButtonElement,
    seg_players: HtmlElement,
    mode_btns: [HtmlButtonElement; 5],
    speed_card: HtmlElement,
    speed_stack: HtmlElement,
    mic: HtmlButtonElement,
    lobby: HtmlElement,
    lobby_title: HtmlElement,
    lobby_room: HtmlElement,
    lobby_status: HtmlElement,
    lobby_start: HtmlElement,
    lobby_live: HtmlElement,
    lobby_share: HtmlButtonElement,
    lobby_code: web_sys::HtmlInputElement,
    remote_audio: web_sys::HtmlAudioElement,
    theme_btns: [HtmlButtonElement; 3],
    party_top: HtmlElement,
    party_bottom: HtmlElement,
    party_pool: HtmlElement,
    pbars: [HtmlButtonElement; 4],
    overlay: HtmlElement,
    kicker: HtmlElement,
    title: HtmlElement,
    sub: HtmlElement,
    primary: HtmlButtonElement,
    secondary: HtmlButtonElement,
}

type Later = Box<dyn FnOnce(&mut App)>;

pub struct App {
    me: Weak<RefCell<App>>,
    els: Els,
    mode: Mode,
    theme: Theme,
    solo: Option<Timed>,
    daily: Option<Timed>,
    party: Party,
    solo_best: u32,
    history: History,
    selected: Vec<usize>,
    hinted: Vec<usize>,
    /// Cards dealt since the last render, so they can animate in.
    fresh: HashSet<Card>,
    /// Input is ignored while a pick resolves.
    locked: bool,
    pending: Option<Later>,
    pending_timer: Option<i32>,
    later_cb: Option<Closure<dyn FnMut()>>,
    tick_cb: Option<Closure<dyn FnMut()>>,
    tick_timer: Option<i32>,
    overlay: Overlay,
    shown_time: u32,
    stats: Stats,
    /// The table is flipped over to show the play profile.
    stats_open: bool,
    /// Reset asks for a second tap.
    reset_armed: bool,
    /// The options sheet is up over the footer.
    options_open: bool,
    online: Online,
    /// Transports closed from inside their own event handler; freed on the next tick.
    dead_nets: Vec<Rc<Net>>,
    speed: Speed,
    speed_stats: SpeedStats,
    /// An in-flight swipe: pointer id, start x, current dx.
    drag: Option<(i32, f64, f64)>,
}

pub fn boot() {
    let els = Els {
        app: dom::el("app"),
        table: dom::el("table"),
        stat_a: dom::el("stat-a"),
        stat_b: dom::el("stat-b"),
        stat_c: dom::el("stat-c"),
        stat_a_label: dom::el("stat-a-label"),
        stat_b_label: dom::el("stat-b-label"),
        stat_c_label: dom::el("stat-c-label"),
        status: dom::el("status"),
        status_line: dom::el("status-line"),
        tagline: dom::el("tagline"),
        hint: dom::btn("hint"),
        deal: dom::btn("deal"),
        shuffle: dom::btn("shuffle"),
        stats_btn: dom::btn("stats-btn"),
        table_wrap: dom::el("table-wrap"),
        stats_panel: dom::el("stats-panel"),
        options_btn: dom::btn("options-btn"),
        options: dom::el("options"),
        options_scrim: dom::el("options-scrim"),
        options_summary: dom::el("options-summary"),
        stats_body: dom::el("stats-body"),
        new_game: dom::btn("new-game"),
        seg_players: dom::el("seg-players"),
        mode_btns: [
            dom::btn("mode-solo"),
            dom::btn("mode-party"),
            dom::btn("mode-daily"),
            dom::btn("mode-online"),
            dom::btn("mode-speed"),
        ],
        speed_card: dom::el("speed-card"),
        speed_stack: dom::el("speed-stack"),
        mic: dom::btn("mic"),
        lobby: dom::el("lobby"),
        lobby_title: dom::el("lobby-title"),
        lobby_room: dom::el("lobby-room"),
        lobby_status: dom::el("lobby-status"),
        lobby_start: dom::el("lobby-start"),
        lobby_live: dom::el("lobby-live"),
        lobby_share: dom::btn("lobby-share"),
        lobby_code: dom::el("lobby-code")
            .dyn_into::<web_sys::HtmlInputElement>()
            .expect("#lobby-code is an input"),
        remote_audio: dom::el("remote-audio")
            .dyn_into::<web_sys::HtmlAudioElement>()
            .expect("#remote-audio is an audio element"),
        theme_btns: [
            dom::btn("theme-neon"),
            dom::btn("theme-classic"),
            dom::btn("theme-night"),
        ],
        party_top: dom::el("party-top"),
        party_bottom: dom::el("party-bottom"),
        party_pool: dom::el("party-pool"),
        pbars: [
            dom::btn("pbar-0"),
            dom::btn("pbar-1"),
            dom::btn("pbar-2"),
            dom::btn("pbar-3"),
        ],
        overlay: dom::el("overlay"),
        kicker: dom::el("overlay-kicker"),
        title: dom::el("overlay-title"),
        sub: dom::el("overlay-sub"),
        primary: dom::btn("overlay-primary"),
        secondary: dom::btn("overlay-secondary"),
    };
    let players = dom::local_get(PARTY_PLAYERS_KEY)
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| (2..=4).contains(n))
        .unwrap_or(2);
    let app = Rc::new(RefCell::new(App {
        me: Weak::new(),
        els,
        mode: Mode::Solo,
        theme: Theme::Neon,
        solo: None,
        daily: None,
        party: Party {
            game: Game::new(&mut Rng::new(dom::random_seed())),
            players,
            scores: [0; 4],
            holder: None,
            deadline: 0.0,
            over: false,
        },
        solo_best: 0,
        history: History::new(),
        selected: Vec::new(),
        hinted: Vec::new(),
        fresh: HashSet::new(),
        locked: false,
        pending: None,
        pending_timer: None,
        later_cb: None,
        tick_cb: None,
        tick_timer: None,
        overlay: Overlay::None,
        shown_time: u32::MAX,
        stats: Stats::default(),
        stats_open: false,
        reset_armed: false,
        options_open: false,
        online: Online::idle(),
        dead_nets: Vec::new(),
        speed: Speed::idle(),
        speed_stats: SpeedStats::default(),
        drag: None,
    }));
    {
        let mut a = app.borrow_mut();
        a.me = Rc::downgrade(&app);
        let weak = Rc::downgrade(&app);
        a.later_cb = Some(Closure::wrap(Box::new(move || {
            if let Some(app) = weak.upgrade() {
                app.borrow_mut().run_pending();
            }
        }) as Box<dyn FnMut()>));
        let weak = Rc::downgrade(&app);
        a.tick_cb = Some(Closure::wrap(Box::new(move || {
            if let Some(app) = weak.upgrade() {
                app.borrow_mut().tick();
            }
        }) as Box<dyn FnMut()>));
    }
    wire_events(&app);
    let mut a = app.borrow_mut();
    a.solo_best = dom::local_get(SOLO_BEST_KEY)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    a.history = parse(dom::local_get(DAILY_HIST_KEY)).unwrap_or_default();
    a.stats = parse::<StatsSave>(dom::local_get(STATS_KEY))
        .map(StatsSave::into_stats)
        .unwrap_or_default();
    a.speed_stats = parse::<SpeedSave>(dom::local_get(SPEED_KEY))
        .map(SpeedSave::into_stats)
        .unwrap_or_default();
    let theme = dom::local_get(THEME_KEY).unwrap_or_default();
    a.set_theme(Theme::from_key(&theme), true);
    a.arrange_bars();
    let saved = dom::local_get(MODE_KEY).unwrap_or_default();
    a.set_mode(Mode::from_key(&saved), true);
    // A shared room link lands straight in the lobby with the code filled in.
    if let Some(code) = net::code_from_url() {
        a.set_mode(Mode::Online, false);
        a.els.lobby_code.set_value(&code);
        a.lobby_status(&format!("TAP JOIN TO ENTER ROOM {code}"));
    }
    let cb = a.tick_cb.as_ref().expect("tick closure");
    a.tick_timer = Some(dom::set_interval(cb, TICK_MS));
    // The app lives for the page: keep the Rc alive.
    std::mem::forget(app.clone());
}

fn with_app<F: FnOnce(&mut App)>(weak: &Weak<RefCell<App>>, f: F) {
    if let Some(app) = weak.upgrade() {
        f(&mut app.borrow_mut());
    }
}

fn wire_events(app: &Rc<RefCell<App>>) {
    let doc = dom::document();
    let root = app.borrow().els.app.clone();

    // Every button on the page carries a data-act; cards carry data-i.
    let weak = Rc::downgrade(app);
    let on_click = Closure::wrap(Box::new(move |e: MouseEvent| {
        let Some(target) = e.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
            return;
        };
        let Ok(Some(btn)) = target.closest("button") else {
            return;
        };
        if let Some(i) = btn.get_attribute("data-i") {
            if let Ok(i) = i.parse::<usize>() {
                with_app(&weak, |a| a.on_card(i));
            }
            return;
        }
        if let Some(act) = btn.get_attribute("data-act") {
            with_app(&weak, |a| a.on_action(&act, &btn));
        }
    }) as Box<dyn FnMut(MouseEvent)>);
    let _ = root.add_event_listener_with_callback("click", on_click.as_ref().unchecked_ref());
    on_click.forget();

    let weak = Rc::downgrade(app);
    let on_key = Closure::wrap(Box::new(move |e: KeyboardEvent| {
        with_app(&weak, |a| a.on_key(&e));
    }) as Box<dyn FnMut(KeyboardEvent)>);
    let _ = doc.add_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref());
    on_key.forget();

    let weak = Rc::downgrade(app);
    let on_resize = Closure::wrap(Box::new(move || {
        with_app(&weak, |a| a.layout());
    }) as Box<dyn FnMut()>);
    let _ = dom::window()
        .add_event_listener_with_callback("resize", on_resize.as_ref().unchecked_ref());
    on_resize.forget();

    let weak = Rc::downgrade(app);
    let on_vis = Closure::wrap(Box::new(move || {
        with_app(&weak, |a| a.sync_clock());
    }) as Box<dyn FnMut()>);
    let _ =
        doc.add_event_listener_with_callback("visibilitychange", on_vis.as_ref().unchecked_ref());
    on_vis.forget();

    // Swipes on the speed card. Pointer capture keeps the drag even when the
    // finger leaves the card.
    let card = app.borrow().els.speed_card.clone();
    let weak = Rc::downgrade(app);
    let on_down = Closure::wrap(Box::new(move |e: web_sys::PointerEvent| {
        with_app(&weak, |a| a.swipe_start(&e));
    }) as Box<dyn FnMut(web_sys::PointerEvent)>);
    let _ = card.add_event_listener_with_callback("pointerdown", on_down.as_ref().unchecked_ref());
    on_down.forget();
    let weak = Rc::downgrade(app);
    let on_move = Closure::wrap(Box::new(move |e: web_sys::PointerEvent| {
        with_app(&weak, |a| a.swipe_move(&e));
    }) as Box<dyn FnMut(web_sys::PointerEvent)>);
    let _ = card.add_event_listener_with_callback("pointermove", on_move.as_ref().unchecked_ref());
    on_move.forget();
    let weak = Rc::downgrade(app);
    let on_up = Closure::wrap(Box::new(move |e: web_sys::PointerEvent| {
        with_app(&weak, |a| a.swipe_end(&e));
    }) as Box<dyn FnMut(web_sys::PointerEvent)>);
    let _ = card.add_event_listener_with_callback("pointerup", on_up.as_ref().unchecked_ref());
    let _ = card.add_event_listener_with_callback("pointercancel", on_up.as_ref().unchecked_ref());
    on_up.forget();

    let weak = Rc::downgrade(app);
    let on_hide = Closure::wrap(Box::new(move || {
        with_app(&weak, |a| {
            if let Some(t) = a.timed_mut() {
                t.pause();
            }
            a.save();
        });
    }) as Box<dyn FnMut()>);
    let _ = dom::window()
        .add_event_listener_with_callback("pagehide", on_hide.as_ref().unchecked_ref());
    on_hide.forget();
}

fn fmt_ms(ms: u32) -> String {
    let s = ms / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}

fn fmt_secs(ms: u32) -> String {
    format!("+{}S", ms / 1000)
}

/// Seconds with one decimal, for reaction times and the speed clock.
fn fmt_tenths(ms: u32) -> String {
    format!("{}.{}S", ms / 1000, (ms % 1000) / 100)
}

impl App {
    // ---------- state access ----------

    fn timed(&self) -> Option<&Timed> {
        match self.mode {
            Mode::Solo => self.solo.as_ref(),
            Mode::Daily => self.daily.as_ref(),
            Mode::Party | Mode::Online | Mode::Speed => None,
        }
    }

    fn timed_mut(&mut self) -> Option<&mut Timed> {
        match self.mode {
            Mode::Solo => self.solo.as_mut(),
            Mode::Daily => self.daily.as_mut(),
            Mode::Party | Mode::Online | Mode::Speed => None,
        }
    }

    fn game(&self) -> &Game {
        match self.mode {
            Mode::Party => &self.party.game,
            Mode::Online => &self.online.game,
            Mode::Speed => &self.speed.game,
            _ => &self.timed().expect("timed game").game,
        }
    }

    fn game_mut(&mut self) -> &mut Game {
        match self.mode {
            Mode::Party => &mut self.party.game,
            Mode::Online => &mut self.online.game,
            Mode::Speed => &mut self.speed.game,
            _ => &mut self.timed_mut().expect("timed game").game,
        }
    }

    fn is_over(&self) -> bool {
        match self.mode {
            Mode::Party => self.party.over,
            Mode::Online => self.online.over || !self.online.started,
            Mode::Speed => self.speed.over || !self.speed.started,
            _ => self.timed().map(|t| t.over).unwrap_or(false),
        }
    }

    // ---------- theme ----------

    /// Applies a theme: the data-theme attribute the stylesheet keys on, the
    /// switch state, the browser chrome colour, and the saved preference.
    fn set_theme(&mut self, t: Theme, boot: bool) {
        if !boot && t == self.theme {
            return;
        }
        self.theme = t;
        // Boot only reflects a stored choice; a first visit stays unwritten.
        if !boot {
            dom::local_set(THEME_KEY, t.key());
        }
        let doc = dom::document();
        if let Some(root) = doc.document_element() {
            let _ = root.set_attribute("data-theme", t.key());
        }
        if let Ok(Some(meta)) = doc.query_selector("meta[name=theme-color]") {
            let _ = meta.set_attribute("content", t.chrome());
        }
        for (i, b) in self.els.theme_btns.iter().enumerate() {
            let _ = b.set_attribute(
                "aria-checked",
                if THEMES[i] == t { "true" } else { "false" },
            );
        }
        self.options_summary();
        if !boot {
            // Card labels and the profile name colours, so they follow the theme.
            self.render_table();
            if self.stats_open {
                self.render_profile();
            }
            self.announce(t.label());
        }
    }

    // ---------- mode ----------

    fn set_mode(&mut self, m: Mode, boot: bool) {
        if !boot && m == self.mode {
            return;
        }
        if let Some(t) = self.timed_mut() {
            t.pause();
        }
        self.save();
        self.cancel_pending();
        self.close_stats();
        // Picking a mode means playing it: the sheet gets out of the way. This runs
        // before the switch because closing refreshes the display of the current game.
        self.close_options();
        self.locked = false;
        self.selected.clear();
        self.hinted.clear();
        self.fresh.clear();
        self.party.holder = None;
        if self.mode == Mode::Online && m != Mode::Online {
            self.online_leave();
        }
        self.mode = m;
        dom::local_set(MODE_KEY, m.key());
        let _ = self.els.app.set_attribute("data-mode", m.key());
        for (i, b) in self.els.mode_btns.iter().enumerate() {
            let _ = b.set_attribute("aria-checked", if MODES[i] == m { "true" } else { "false" });
        }
        self.options_summary();
        dom::set_hidden(&self.els.hint, !m.is_timed());
        dom::set_hidden(&self.els.seg_players, m != Mode::Party);
        dom::set_hidden(&self.els.mic, m != Mode::Online);
        dom::set_hidden(&self.els.deal, m == Mode::Speed);
        self.els.shuffle.set_disabled(m == Mode::Speed);
        self.els.new_game.set_text_content(Some(match m {
            Mode::Daily => "RESTART",
            Mode::Party | Mode::Online => "NEW GAME",
            Mode::Speed => "NEW ROUND",
            Mode::Solo => "NEW DEAL",
        }));
        self.ensure_game();
        self.close_overlay();
        self.tagline();
        self.render_all();
        self.fresh.clear();
        dom::set_hidden(&self.els.lobby, m != Mode::Online || self.online.started);
        if m == Mode::Online {
            self.render_mic();
            if !self.online.started {
                self.lobby_reset();
            }
            self.say(&self.idle_line());
        } else if m == Mode::Speed {
            self.speed_new_round();
        } else if self.is_over() {
            self.show_done();
        } else {
            self.say(&self.idle_line());
        }
        self.sync_clock();
    }

    /// Loads or creates the current mode's game.
    fn ensure_game(&mut self) {
        match self.mode {
            Mode::Solo => {
                if self.solo.is_none() {
                    self.solo =
                        parse::<TimedSave>(dom::local_get(SOLO_KEY)).and_then(Timed::from_save);
                }
                if self.solo.is_none() {
                    self.solo = Some(Timed::new(
                        Game::new(&mut Rng::new(dom::random_seed())),
                        None,
                    ));
                }
            }
            Mode::Daily => {
                let today = dom::today_key();
                if self
                    .daily
                    .as_ref()
                    .map(|d| d.date.as_deref() != Some(&today))
                    .unwrap_or(true)
                {
                    self.daily = parse::<TimedSave>(dom::local_get(DAILY_KEY))
                        .filter(|s| s.date.as_deref() == Some(&today))
                        .and_then(Timed::from_save);
                }
                if self.daily.is_none() {
                    self.daily = Some(Timed::new(daily::daily(&today), Some(today)));
                }
            }
            Mode::Party | Mode::Online | Mode::Speed => {}
        }
    }

    fn tagline(&self) {
        let line = match self.mode {
            Mode::Solo => "ALL THE SAME OR ALL DIFFERENT",
            Mode::Party => "CALL IT. PICK THREE. SEVEN SECONDS.",
            Mode::Daily => "ONE DECK A DAY, THE SAME FOR EVERYONE",
            Mode::Online => "TWO DEVICES, ONE DECK, LIVE VOICE",
            Mode::Speed => "SET OR NOT. HOW FAST CAN YOU TELL?",
        };
        self.els.tagline.set_text_content(Some(line));
    }

    fn idle_line(&self) -> String {
        match self.mode {
            Mode::Party => "TAP YOUR BAR TO CALL A SET".to_string(),
            Mode::Online if !self.online.started => "HOST A ROOM OR JOIN ONE".to_string(),
            Mode::Online => "FIRST TO TAP A SET TAKES IT".to_string(),
            Mode::Speed => "SWIPE RIGHT FOR A SET, LEFT FOR NOT".to_string(),
            _ => "TAP THREE CARDS THAT MAKE A SET".to_string(),
        }
    }

    fn new_game(&mut self) {
        self.cancel_pending();
        self.close_stats();
        self.locked = false;
        self.selected.clear();
        self.hinted.clear();
        self.fresh.clear();
        match self.mode {
            Mode::Solo => {
                self.solo = Some(Timed::new(
                    Game::new(&mut Rng::new(dom::random_seed())),
                    None,
                ));
            }
            Mode::Daily => {
                let today = dom::today_key();
                self.daily = Some(Timed::new(daily::daily(&today), Some(today)));
            }
            Mode::Party => {
                self.party.game = Game::new(&mut Rng::new(dom::random_seed()));
                self.party.scores = [0; 4];
                self.party.holder = None;
                self.party.over = false;
            }
            Mode::Speed => {
                self.speed_new_round();
                return;
            }
            Mode::Online => {
                // Only the host deals; a guest just clears the end card.
                if self.online.me == 0 && self.online.net.is_some() {
                    let seed = dom::random_seed();
                    self.online_start(seed);
                    self.net_send(&Wire::Start { seed });
                } else {
                    self.close_overlay();
                    self.say("ONLY THE HOST DEALS A NEW GAME");
                }
                return;
            }
        }
        self.close_overlay();
        self.render_all();
        self.say(&self.idle_line());
        self.save();
        self.sync_clock();
    }

    // ---------- persistence ----------

    fn save(&self) {
        if let Some(s) = &self.solo {
            dom::local_set(SOLO_KEY, &to_json(&s.to_save()));
        }
        if let Some(d) = &self.daily {
            dom::local_set(DAILY_KEY, &to_json(&d.to_save()));
        }
        dom::local_set(PARTY_PLAYERS_KEY, &self.party.players.to_string());
    }

    fn save_stats(&self) {
        dom::local_set(STATS_KEY, &to_json(&StatsSave::from_stats(&self.stats)));
    }

    // ---------- clock ----------

    /// Runs the clock only while a timed, unfinished game is on screen.
    fn sync_clock(&mut self) {
        let run = self.mode.is_timed()
            && self.overlay == Overlay::None
            && !self.stats_open
            && !self.options_open
            && !dom::page_hidden()
            && !self.is_over();
        if let Some(t) = self.timed_mut() {
            if run {
                t.start();
            } else {
                t.pause();
            }
        }
        self.render_stats();
    }

    fn tick(&mut self) {
        self.dead_nets.clear();
        if self.mode == Mode::Speed {
            if self.speed.started && !self.speed.over {
                self.els
                    .stat_a
                    .set_text_content(Some(&fmt_tenths(self.speed.clock())));
            }
            return;
        }
        if self.mode.is_timed() {
            let total = self.timed().map(|t| t.total()).unwrap_or(0);
            if total / 1000 != self.shown_time / 1000 {
                self.shown_time = total;
                self.els.stat_a.set_text_content(Some(&fmt_ms(total)));
            }
            return;
        }
        if let Some(p) = self.party.holder {
            let left = self.party.deadline - dom::now();
            if left <= 0.0 {
                self.party.scores[p] -= 1;
                self.party.holder = None;
                self.selected.clear();
                self.refresh_marks();
                self.render_bars();
                self.say(&format!("{} {MIDDOT}TIME UP {MIDDOT}-1", PLAYER_NAMES[p]));
            } else {
                let bar = &self.els.pbars[p];
                dom::set_var(bar, "--frac", &format!("{:.3}", left / BUZZ_MS));
                if let Some(clock) = dom::children(bar).get(2) {
                    clock.set_text_content(Some(&format!("{:.1}", left / 1000.0)));
                }
            }
        }
    }

    // ---------- rendering ----------

    fn render_all(&mut self) {
        self.render_table();
        self.render_stats();
        self.render_bars();
    }

    fn render_table(&mut self) {
        let names = self.theme.color_names();
        let mut html = String::with_capacity(16 * 700);
        for (i, &card) in self.game().table().iter().enumerate() {
            let fresh = if self.fresh.contains(&card) {
                " in"
            } else {
                ""
            };
            html.push_str(&format!(
                r#"<button type="button" class="card{fresh}" data-i="{i}" data-card="{}" role="gridcell" aria-label="{}" style="--d:{}ms">{}</button>"#,
                card.0,
                cards::describe(card, names),
                (i % 12) * 28,
                cards::face(card)
            ));
        }
        self.els.table.set_inner_html(&html);
        self.fresh.clear();
        self.layout();
        self.refresh_marks();
    }

    /// Three columns in portrait, three rows in landscape; the stylesheet sizes the cells.
    fn layout(&self) {
        let n = self.game().table().len().max(12);
        let per = n.div_ceil(3);
        let (cols, rows) = if dom::landscape() { (per, 3) } else { (3, per) };
        dom::set_var(&self.els.table, "--cols", &cols.to_string());
        dom::set_var(&self.els.table, "--rows", &rows.to_string());
    }

    fn refresh_marks(&self) {
        for (i, el) in dom::children(&self.els.table).iter().enumerate() {
            dom::set_class(el, "sel", self.selected.contains(&i));
            dom::set_class(el, "hint", self.hinted.contains(&i));
            let _ = el.set_attribute(
                "aria-pressed",
                if self.selected.contains(&i) {
                    "true"
                } else {
                    "false"
                },
            );
        }
        dom::set_class(&self.els.table, "armed", self.party.holder.is_some());
        let calling = self.mode == Mode::Party && self.party.holder.is_some();
        self.els.deal.set_disabled(calling);
        self.els.shuffle.set_disabled(calling);
        let sel = match self.party.holder {
            Some(p) if self.mode == Mode::Party => format!("var(--p{p})"),
            _ => "var(--sel-default)".to_string(),
        };
        dom::set_var(&self.els.table, "--sel", &sel);
    }

    fn render_stats(&mut self) {
        let e = &self.els;
        match self.mode {
            Mode::Speed => {
                e.stat_a_label.set_text_content(Some("TIME"));
                e.stat_b_label.set_text_content(Some("DONE"));
                e.stat_c_label.set_text_content(Some("BEST"));
                e.stat_a
                    .set_text_content(Some(&fmt_tenths(self.speed.clock())));
                e.stat_b.set_text_content(Some(&format!(
                    "{}/{}",
                    self.speed.pos.min(speed::ROUND_LEN),
                    speed::ROUND_LEN
                )));
                let best = self.speed_stats.best_ms;
                e.stat_c.set_text_content(Some(&if best > 0 {
                    fmt_tenths(best)
                } else {
                    "-".to_string()
                }));
            }
            Mode::Online => {
                e.stat_a_label.set_text_content(Some("YOU"));
                e.stat_b_label.set_text_content(Some("THEM"));
                e.stat_c_label.set_text_content(Some("DECK"));
                let me = self.online.me;
                e.stat_a
                    .set_text_content(Some(&self.online.scores[me].to_string()));
                e.stat_b
                    .set_text_content(Some(&self.online.scores[1 - me].to_string()));
                e.stat_c
                    .set_text_content(Some(&self.online.game.remaining().to_string()));
            }
            Mode::Party => {
                e.stat_a_label.set_text_content(Some("SETS"));
                e.stat_b_label.set_text_content(Some("DECK"));
                e.stat_c_label.set_text_content(Some("PLAYERS"));
                e.stat_a
                    .set_text_content(Some(&self.party.game.found().to_string()));
                e.stat_b
                    .set_text_content(Some(&self.party.game.remaining().to_string()));
                e.stat_c
                    .set_text_content(Some(&self.party.players.to_string()));
            }
            _ => {
                let t = self.timed().expect("timed game");
                e.stat_a_label.set_text_content(Some("TIME"));
                e.stat_b_label.set_text_content(Some("SETS"));
                e.stat_c_label.set_text_content(Some("DECK"));
                e.stat_a.set_text_content(Some(&fmt_ms(t.total())));
                e.stat_b.set_text_content(Some(&t.game.found().to_string()));
                e.stat_c
                    .set_text_content(Some(&t.game.remaining().to_string()));
                self.shown_time = t.total();
            }
        }
        for (i, b) in dom::children(&self.els.seg_players).iter().enumerate() {
            let _ = b.set_attribute(
                "aria-checked",
                if i + 2 == self.party.players {
                    "true"
                } else {
                    "false"
                },
            );
        }
    }

    /// Player bars: the first on this side, the second across, then alternating.
    fn arrange_bars(&self) {
        let n = self.party.players;
        for (p, bar) in self.els.pbars.iter().enumerate() {
            let side = if p >= n {
                &self.els.party_pool
            } else if n == 2 {
                if p == 0 {
                    &self.els.party_bottom
                } else {
                    &self.els.party_top
                }
            } else if p < 2 {
                &self.els.party_bottom
            } else {
                &self.els.party_top
            };
            let _ = side.append_child(bar);
        }
        self.render_bars();
    }

    fn render_bars(&self) {
        for (p, bar) in self.els.pbars.iter().enumerate() {
            let holds = self.party.holder == Some(p);
            dom::set_class(bar, "holding", holds);
            dom::set_class(bar, "waiting", self.party.holder.is_some() && !holds);
            let kids = dom::children(bar);
            if let Some(score) = kids.get(1) {
                score.set_text_content(Some(&self.party.scores[p].to_string()));
            }
            if let Some(clock) = kids.get(2) {
                if !holds {
                    clock.set_text_content(Some(""));
                    dom::set_var(bar, "--frac", "0");
                }
            }
            let _ = bar.set_attribute(
                "aria-label",
                &format!("{}, score {}", PLAYER_NAMES[p], self.party.scores[p]),
            );
        }
    }

    fn say(&self, line: &str) {
        self.els.status_line.set_text_content(Some(line));
        self.announce(line);
    }

    fn announce(&self, msg: &str) {
        self.els.status.set_text_content(Some(msg));
    }

    // ---------- overlay ----------

    fn show_overlay(
        &mut self,
        kind: Overlay,
        kicker: &str,
        title: &str,
        sub_html: &str,
        primary: &str,
        secondary: Option<&str>,
    ) {
        self.overlay = kind;
        self.els.kicker.set_text_content(Some(kicker));
        self.els.title.set_text_content(Some(title));
        self.els.sub.set_inner_html(sub_html);
        self.els.primary.set_text_content(Some(primary));
        match secondary {
            Some(s) => {
                self.els.secondary.set_text_content(Some(s));
                dom::set_hidden(&self.els.secondary, false);
            }
            None => dom::set_hidden(&self.els.secondary, true),
        }
        dom::set_hidden(&self.els.overlay, false);
        self.sync_clock();
    }

    fn close_overlay(&mut self) {
        self.overlay = Overlay::None;
        dom::set_hidden(&self.els.overlay, true);
        self.sync_clock();
    }

    fn rules(&mut self) {
        if self.overlay == Overlay::Rules {
            self.close_overlay();
            return;
        }
        let html = concat!(
            "<ul class=\"rules\">",
            "<li>Every card has a <b>number</b>, a <b>shape</b>, a <b>colour</b> and a <b>shading</b>.</li>",
            "<li>Three cards are a <b>set</b> when each of those four is <b>all the same</b> or <b>all different</b> across the three.</li>",
            "<li><b>Solo</b> and <b>Daily</b>: clear the deck against the clock. Wrong pick +10s, hint +15s, dealing while a set is out +15s.</li>",
            "<li><b>Party</b>: tap your bar to call a set, then pick three within seven seconds. Right +1, wrong or late &minus;1.</li>",
            "<li>The shuffle button rearranges the table to give you a fresh look. It is free.</li>",
            "<li>The profile button flips the table over to show which kinds of set you take and which you pass over, across all your Solo and Daily games.</li>",
            "<li><b>Speed</b>: one triple at a time. Swipe right if it is a set, left if not. Twenty triples against the clock; a wrong swipe adds two seconds. The profile shows your reaction times.</li>",
            "<li>Mode and theme are under <b>OPTIONS</b> at the top.</li>",
            "<li>Keys: letters pick cards, <b>H</b> hint, <b>D</b> deal, <b>N</b> new, <b>T</b> theme, <b>P</b> profile, <b>O</b> options, <b>1</b> to <b>4</b> call, arrow keys answer in Speed.</li>",
            "</ul>"
        );
        self.show_overlay(
            Overlay::Rules,
            "WALLACE CORP. FIELD MANUAL",
            "HOW TO PLAY",
            html,
            "CLOSE",
            None,
        );
    }

    fn show_done(&mut self) {
        match self.mode {
            Mode::Speed => {
                let sp = &self.speed;
                let mut rts = sp.rts.clone();
                rts.sort_unstable();
                let p50 = speed::percentile(&rts, 0.5)
                    .map(fmt_tenths)
                    .unwrap_or_else(|| "-".into());
                let best = self.speed_stats.best_ms;
                let kicker = if sp.total <= best {
                    "NEW BEST"
                } else {
                    "STACK CLEARED"
                };
                let sub = format!(
                    "{}/{} CORRECT{MIDDOT}MEDIAN REACTION {p50}{MIDDOT}PENALTIES {}<br>BEST {}",
                    sp.correct,
                    speed::ROUND_LEN,
                    fmt_tenths(sp.penalty),
                    fmt_tenths(best)
                );
                let title = fmt_tenths(sp.total);
                self.show_overlay(
                    Overlay::Done,
                    kicker,
                    &title,
                    &sub,
                    "AGAIN",
                    Some("PROFILE"),
                );
            }
            Mode::Online => {
                let me = self.online.me;
                let (mine, theirs) = (self.online.scores[me], self.online.scores[1 - me]);
                let title = match mine.cmp(&theirs) {
                    std::cmp::Ordering::Greater => "YOU WIN",
                    std::cmp::Ordering::Less => "THEY WIN",
                    std::cmp::Ordering::Equal => "DRAW",
                };
                let sub = format!("YOU {mine}{MIDDOT}THEM {theirs}");
                let primary = if me == 0 { "NEW GAME" } else { "OK" };
                self.show_overlay(Overlay::Done, "DECK CLEARED", title, &sub, primary, None);
            }
            Mode::Party => {
                let n = self.party.players;
                let top = (0..n).map(|p| self.party.scores[p]).max().unwrap_or(0);
                let leaders: Vec<&str> = (0..n)
                    .filter(|&p| self.party.scores[p] == top)
                    .map(|p| PLAYER_NAMES[p])
                    .collect();
                let title = if leaders.len() == 1 {
                    format!("{} WINS", leaders[0])
                } else {
                    "DRAW".to_string()
                };
                let sub = (0..n)
                    .map(|p| format!("{} {}", PLAYER_NAMES[p], self.party.scores[p]))
                    .collect::<Vec<_>>()
                    .join(MIDDOT);
                self.show_overlay(
                    Overlay::Done,
                    "DECK CLEARED",
                    &title,
                    &sub,
                    "NEW GAME",
                    None,
                );
            }
            Mode::Solo => {
                let t = self.timed().expect("timed game");
                let total = t.total();
                let mut sub = format!("{}{MIDDOT}{} SETS", fmt_ms(total), t.game.found());
                if t.penalty > 0 {
                    sub.push_str(&format!("{MIDDOT}{} PENALTY", fmt_ms(t.penalty)));
                }
                if self.solo_best > 0 {
                    sub.push_str(&format!("<br>BEST {}", fmt_ms(self.solo_best)));
                }
                let kicker = if total <= self.solo_best {
                    "NEW BEST"
                } else {
                    "DECK CLEARED"
                };
                let line = WIN_LINES[(total as usize / 1000) % WIN_LINES.len()];
                self.show_overlay(Overlay::Done, kicker, line, &sub, "NEW DEAL", None);
            }
            Mode::Daily => {
                let t = self.timed().expect("timed game");
                let total = t.total();
                let today = t.date.clone().unwrap_or_default();
                let best = self.history.get(&today).copied().unwrap_or(total);
                let mut sub = format!("{}{MIDDOT}{} SETS", fmt_ms(total), t.game.found());
                if t.penalty > 0 {
                    sub.push_str(&format!("{MIDDOT}{} PENALTY", fmt_ms(t.penalty)));
                }
                sub.push_str(&format!(
                    "<br>BEST TODAY {}{MIDDOT}STREAK {}",
                    fmt_ms(best),
                    self.streak(&today)
                ));
                let kicker = format!("DAILY {today}");
                self.show_overlay(
                    Overlay::Done,
                    &kicker,
                    "DECK CLEARED",
                    &sub,
                    "RESTART",
                    Some("SHARE"),
                );
            }
        }
    }

    /// Consecutive finished days ending on the given day.
    fn streak(&self, today: &str) -> u32 {
        let mut day = daily::day_number(today);
        if day == i64::MIN {
            return 0;
        }
        let days: HashSet<i64> = self.history.keys().map(|k| daily::day_number(k)).collect();
        let mut n = 0;
        while days.contains(&day) {
            n += 1;
            day -= 1;
        }
        n
    }

    // ---------- input ----------

    fn on_action(&mut self, act: &str, btn: &Element) {
        match act {
            "mode" => {
                let m = btn.get_attribute("data-mode").unwrap_or_default();
                self.set_mode(Mode::from_key(&m), false);
            }
            "theme" => {
                let t = btn.get_attribute("data-theme").unwrap_or_default();
                self.set_theme(Theme::from_key(&t), false);
            }
            "host" => self.online_host(),
            "join" => self.online_join(),
            "leave" => {
                self.online_leave();
                self.lobby_reset();
                self.say(&self.idle_line());
            }
            "share-room" => {
                let code = self
                    .online
                    .net
                    .as_ref()
                    .map(|n| n.code.clone())
                    .unwrap_or_default();
                let text = format!(
                    "JOIN MY TRIPOS GAME{MIDDOT}ROOM {code}\n{}",
                    net::join_link(&code)
                );
                self.share_plain(text, self.els.lobby_share.clone());
            }
            "mic" => self.toggle_mic(),
            "speed-yes" => self.speed_answer(true),
            "speed-no" => self.speed_answer(false),
            "players" => {
                let n = btn
                    .get_attribute("data-n")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(2);
                self.set_players(n);
            }
            "hint" => self.hint(),
            "deal" => self.deal(),
            "shuffle" => self.shuffle(),
            "stats" => self.toggle_stats(),
            "stats-reset" => self.reset_stats(),
            "options" => self.toggle_options(),
            "options-close" => self.close_options(),
            "new" => self.new_game(),
            "rules" => self.rules(),
            "buzz" => {
                let p = btn
                    .get_attribute("data-p")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(0);
                self.buzz(p);
            }
            "overlay-primary" => match self.overlay {
                Overlay::Rules => self.close_overlay(),
                Overlay::Done => self.new_game(),
                Overlay::SpeedStart => self.speed_go(),
                Overlay::None => {}
            },
            "overlay-secondary" if self.mode == Mode::Daily => self.share(),
            "overlay-secondary" if self.mode == Mode::Speed => self.open_stats(),
            _ => {}
        }
    }

    fn on_key(&mut self, e: &KeyboardEvent) {
        if e.ctrl_key() || e.meta_key() || e.alt_key() {
            return;
        }
        let typing = e
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .map(|el| el.tag_name() == "INPUT")
            .unwrap_or(false);
        if typing {
            if e.key() == "Enter" {
                e.prevent_default();
                self.online_join();
            }
            return;
        }
        let key = e.key();
        let k = key.to_ascii_lowercase();
        match k.as_str() {
            "escape" => {
                if self.options_open {
                    self.close_options();
                } else if self.stats_open {
                    self.close_stats();
                } else if self.overlay != Overlay::None {
                    if self.overlay == Overlay::Rules {
                        self.close_overlay();
                    }
                } else if !self.locked {
                    self.selected.clear();
                    self.refresh_marks();
                }
            }
            "enter" | " " => {
                if self.overlay != Overlay::None {
                    e.prevent_default();
                    self.on_action("overlay-primary", &self.els.primary.clone());
                }
            }
            "arrowright" => self.speed_answer(true),
            "arrowleft" => self.speed_answer(false),
            "t" => self.set_theme(self.theme.next(), false),
            "p" => self.toggle_stats(),
            "o" => self.toggle_options(),
            "n" => self.new_game(),
            "h" => self.hint(),
            "d" => self.deal(),
            "?" | "r" => self.rules(),
            "1" | "2" | "3" | "4" => {
                let p = k.parse::<usize>().unwrap_or(1) - 1;
                if self.mode == Mode::Party && p < self.party.players {
                    self.buzz(p);
                }
            }
            _ => {
                if k.len() == 1 {
                    if let Some(i) = KEY_ROW.find(k.as_str()) {
                        self.on_card(i);
                    }
                }
            }
        }
    }

    fn on_card(&mut self, i: usize) {
        if self.locked
            || self.overlay != Overlay::None
            || self.stats_open
            || self.options_open
            || self.is_over()
        {
            return;
        }
        if i >= self.game().table().len() {
            return;
        }
        if self.mode == Mode::Party && self.party.holder.is_none() {
            self.say("TAP YOUR BAR FIRST");
            return;
        }
        if self.mode == Mode::Online && self.online.pending.is_some() {
            return;
        }
        if let Some(pos) = self.selected.iter().position(|&s| s == i) {
            self.selected.remove(pos);
        } else {
            self.selected.push(i);
        }
        self.refresh_marks();
        if self.selected.len() == 3 {
            self.resolve();
        }
    }

    fn resolve(&mut self) {
        let idx = [self.selected[0], self.selected[1], self.selected[2]];
        let table = self.game().table();
        let good = tripos_engine::is_set(table[idx[0]], table[idx[1]], table[idx[2]]);
        let picked = [table[idx[0]], table[idx[1]], table[idx[2]]];
        let kids = dom::children(&self.els.table);
        for &i in &idx {
            if let Some(el) = kids.get(i) {
                dom::set_class(el, if good { "good" } else { "bad" }, true);
            }
        }
        self.locked = true;
        if good {
            if self.mode == Mode::Online {
                let cards = picked;
                if self.online.me == 0 {
                    self.later(GOOD_MS, Box::new(move |a| a.online_host_claim(cards)));
                } else {
                    // The host decides; stay locked until it answers.
                    self.online.pending = Some(cards);
                    self.net_send(&Wire::Claim {
                        cards: cards.map(|c| c.0),
                    });
                    self.say("CLAIMING");
                }
                return;
            }
            self.later(GOOD_MS, Box::new(move |a| a.finish_set(idx)));
        } else {
            let line = match self.mode {
                Mode::Online => {
                    let me = self.online.me;
                    self.online.scores[me] -= 1;
                    self.net_send(&Wire::Wrong { by: me as u8 });
                    self.render_stats();
                    format!("NOT A SET{MIDDOT}-1")
                }
                Mode::Party => {
                    let p = self.party.holder.unwrap_or(0);
                    self.party.scores[p] -= 1;
                    self.party.holder = None;
                    self.render_bars();
                    format!("{}{MIDDOT}NOT A SET{MIDDOT}-1", PLAYER_NAMES[p])
                }
                _ => {
                    if let Some(t) = self.timed_mut() {
                        t.penalize(WRONG_MS);
                    }
                    self.stats.wrong += 1;
                    self.save_stats();
                    format!("NOT A SET{MIDDOT}{}", fmt_secs(WRONG_MS))
                }
            };
            self.say(&line);
            self.later(BAD_MS, Box::new(|a| a.finish_bad()));
        }
    }

    fn finish_bad(&mut self) {
        self.selected.clear();
        self.locked = false;
        for el in dom::children(&self.els.table) {
            dom::set_class(&el, "bad", false);
        }
        self.refresh_marks();
        self.save();
    }

    fn finish_set(&mut self, idx: [usize; 3]) {
        let before: HashSet<Card> = self.game().table().iter().copied().collect();
        if self.mode.is_timed() {
            // Profile the claim against every set that was out at that moment.
            let (sets, claimed) = {
                let table = self.game().table();
                let sets: Vec<[Card; 3]> = self
                    .game()
                    .sets()
                    .iter()
                    .map(|s| [table[s[0]], table[s[1]], table[s[2]]])
                    .collect();
                (sets, [table[idx[0]], table[idx[1]], table[idx[2]]])
            };
            let ms = self.timed().map(|t| t.search_ms()).unwrap_or(0);
            self.stats.record_claim(&sets, claimed, ms);
            self.save_stats();
        }
        let result = self.game_mut().claim(idx);
        if let Some(t) = self.timed_mut() {
            t.mark_table();
        }
        self.selected.clear();
        self.hinted.clear();
        self.locked = false;
        if result != Claim::Set {
            self.render_table();
            return;
        }
        self.fresh = self
            .game()
            .table()
            .iter()
            .copied()
            .filter(|c| !before.contains(c))
            .collect();
        let line = match self.mode {
            Mode::Party => {
                let p = self.party.holder.take().unwrap_or(0);
                self.party.scores[p] += 1;
                format!("{}{MIDDOT}SET{MIDDOT}+1", PLAYER_NAMES[p])
            }
            _ => format!("SET{MIDDOT}{} LEFT IN DECK", self.game().remaining()),
        };
        self.render_all();
        self.say(&line);
        self.save();
        self.check_over();
    }

    fn check_over(&mut self) {
        if !self.game().is_over() {
            return;
        }
        match self.mode {
            Mode::Speed => return,
            Mode::Online => {
                if !self.online.started {
                    return;
                }
                self.online.over = true;
            }
            Mode::Party => {
                self.party.over = true;
                self.party.holder = None;
                self.render_bars();
            }
            Mode::Solo => {
                let t = self.solo.as_mut().expect("solo");
                t.pause();
                t.over = true;
                let total = t.total();
                if self.solo_best == 0 || total < self.solo_best {
                    self.solo_best = total;
                    dom::local_set(SOLO_BEST_KEY, &total.to_string());
                }
            }
            Mode::Daily => {
                let t = self.daily.as_mut().expect("daily");
                t.pause();
                t.over = true;
                let total = t.total();
                let key = t.date.clone().unwrap_or_default();
                let best = self.history.entry(key).or_insert(total);
                if total < *best {
                    *best = total;
                }
                dom::local_set(DAILY_HIST_KEY, &to_json(&self.history));
            }
        }
        self.save();
        self.show_done();
    }

    fn hint(&mut self) {
        if !self.mode.is_timed()
            || self.locked
            || self.overlay != Overlay::None
            || self.stats_open
            || self.options_open
            || self.is_over()
        {
            return;
        }
        let sets = self.game().sets();
        let Some(set) = sets.first().copied() else {
            self.say("NO SET ON THE TABLE: DEAL");
            return;
        };
        let shown = self.hinted.len();
        if shown >= 2 {
            self.say("TWO SHOWN: FIND THE THIRD");
            return;
        }
        self.hinted = set[..shown + 1].to_vec();
        self.selected.clear();
        if let Some(t) = self.timed_mut() {
            t.hints += 1;
            t.penalize(HINT_MS);
        }
        self.stats.hints += 1;
        self.save_stats();
        self.refresh_marks();
        self.say(&format!("HINT{MIDDOT}{}", fmt_secs(HINT_MS)));
        self.save();
    }

    /// Rearranges the table for a fresh look. Selection and hints follow their cards.
    fn shuffle(&mut self) {
        if self.locked
            || self.overlay != Overlay::None
            || self.stats_open
            || self.options_open
            || self.is_over()
        {
            return;
        }
        if self.mode == Mode::Party && self.party.holder.is_some() {
            return;
        }
        let table = self.game().table().to_vec();
        let selected: Vec<Card> = self.selected.iter().map(|&i| table[i]).collect();
        let hinted: Vec<Card> = self.hinted.iter().map(|&i| table[i]).collect();
        self.game_mut()
            .shuffle_table(&mut Rng::new(dom::random_seed()));
        let table = self.game().table().to_vec();
        let at = |c: &Card| table.iter().position(|t| t == c);
        self.selected = selected.iter().filter_map(at).collect();
        self.hinted = hinted.iter().filter_map(at).collect();
        self.fresh = table.iter().copied().collect();
        self.render_table();
        self.say("SHUFFLED");
        self.save();
    }

    fn deal(&mut self) {
        if self.locked
            || self.overlay != Overlay::None
            || self.stats_open
            || self.options_open
            || self.is_over()
        {
            return;
        }
        if self.mode == Mode::Party && self.party.holder.is_some() {
            return;
        }
        if self.mode == Mode::Online && self.online.me == 1 {
            // Guests ask; the host checks the table and answers.
            self.net_send(&Wire::Deal);
            return;
        }
        if self.game().has_set() {
            let line = match self.mode {
                Mode::Party | Mode::Online => "A SET IS OUT THERE".to_string(),
                _ => {
                    if let Some(t) = self.timed_mut() {
                        t.penalize(BAD_DEAL_MS);
                    }
                    // Calling no set while sets are out: every one of them was missed.
                    let sets: Vec<[Card; 3]> = {
                        let table = self.game().table();
                        self.game()
                            .sets()
                            .iter()
                            .map(|s| [table[s[0]], table[s[1]], table[s[2]]])
                            .collect()
                    };
                    self.stats.record_passed(&sets);
                    self.stats.bad_deals += 1;
                    self.save_stats();
                    format!("A SET IS OUT THERE{MIDDOT}{}", fmt_secs(BAD_DEAL_MS))
                }
            };
            self.say(&line);
            self.save();
            return;
        }
        if self.mode == Mode::Online {
            if self.online_apply_deal() {
                self.net_send(&Wire::Dealt);
            }
            return;
        }
        let before: HashSet<Card> = self.game().table().iter().copied().collect();
        if self.game_mut().deal_three() {
            if let Some(t) = self.timed_mut() {
                t.mark_table();
            }
            self.fresh = self
                .game()
                .table()
                .iter()
                .copied()
                .filter(|c| !before.contains(c))
                .collect();
            self.selected.clear();
            self.hinted.clear();
            self.render_all();
            self.say("NO SET: THREE MORE CARDS");
            self.save();
        } else {
            self.check_over();
        }
    }

    fn buzz(&mut self, p: usize) {
        if self.mode != Mode::Party
            || self.locked
            || self.overlay != Overlay::None
            || self.party.over
            || p >= self.party.players
            || self.party.holder.is_some()
        {
            return;
        }
        self.party.holder = Some(p);
        self.party.deadline = dom::now() + BUZZ_MS;
        self.selected.clear();
        self.refresh_marks();
        self.render_bars();
        self.say(&format!("{}: PICK THREE CARDS", PLAYER_NAMES[p]));
    }

    fn set_players(&mut self, n: usize) {
        let n = n.clamp(2, 4);
        if n == self.party.players {
            return;
        }
        self.party.players = n;
        self.party.scores = [0; 4];
        self.party.holder = None;
        self.party.over = false;
        self.party.game = Game::new(&mut Rng::new(dom::random_seed()));
        self.selected.clear();
        self.arrange_bars();
        self.close_overlay();
        self.render_all();
        self.say(&format!("{n} PLAYERS{MIDDOT}NEW GAME"));
        self.save();
    }

    // ---------- play profile ----------

    fn toggle_options(&mut self) {
        if self.options_open {
            self.close_options();
        } else {
            self.open_options();
        }
    }

    fn open_options(&mut self) {
        if self.options_open {
            return;
        }
        self.options_open = true;
        dom::set_class(&self.els.options, "open", true);
        dom::set_class(&self.els.options_scrim, "open", true);
        let _ = self.els.options.set_attribute("aria-hidden", "false");
        let _ = self.els.options_btn.set_attribute("aria-expanded", "true");
        self.announce("Options shown.");
        self.sync_clock();
    }

    fn close_options(&mut self) {
        if !self.options_open {
            return;
        }
        self.options_open = false;
        dom::set_class(&self.els.options, "open", false);
        dom::set_class(&self.els.options_scrim, "open", false);
        let _ = self.els.options.set_attribute("aria-hidden", "true");
        let _ = self.els.options_btn.set_attribute("aria-expanded", "false");
        self.announce("Options hidden.");
        self.sync_clock();
    }

    /// The footer button names the current mode and theme.
    fn options_summary(&self) {
        let text = format!(
            "{}{MIDDOT}{}",
            self.mode.key().to_uppercase(),
            self.theme.key().to_uppercase()
        );
        self.els.options_summary.set_text_content(Some(&text));
    }

    fn toggle_stats(&mut self) {
        if self.stats_open {
            self.close_stats();
        } else {
            self.open_stats();
        }
    }

    /// Flips the table over to the profile. The clock pauses while it shows.
    fn open_stats(&mut self) {
        if self.locked || self.stats_open {
            return;
        }
        self.stats_open = true;
        self.reset_armed = false;
        self.render_profile();
        dom::set_class(&self.els.table_wrap, "flipped", true);
        let _ = self.els.stats_btn.set_attribute("aria-pressed", "true");
        let _ = self.els.table.set_attribute("aria-hidden", "true");
        let _ = self.els.stats_panel.set_attribute("aria-hidden", "false");
        // A finished-game card would cover the profile; it comes back on close.
        dom::set_hidden(&self.els.overlay, true);
        self.announce("Play profile shown.");
        self.sync_clock();
    }

    fn close_stats(&mut self) {
        if !self.stats_open {
            return;
        }
        self.stats_open = false;
        self.reset_armed = false;
        dom::set_class(&self.els.table_wrap, "flipped", false);
        let _ = self.els.stats_btn.set_attribute("aria-pressed", "false");
        let _ = self.els.table.set_attribute("aria-hidden", "false");
        let _ = self.els.stats_panel.set_attribute("aria-hidden", "true");
        dom::set_hidden(&self.els.overlay, self.overlay == Overlay::None);
        self.announce("Table shown.");
        self.sync_clock();
    }

    /// First tap arms, second tap clears the lifetime profile.
    fn reset_stats(&mut self) {
        if !self.reset_armed {
            self.reset_armed = true;
            self.render_profile();
            return;
        }
        self.stats = Stats::default();
        self.reset_armed = false;
        self.save_stats();
        self.render_profile();
        self.announce("Profile reset.");
    }

    fn cat_label(&self, key: &str) -> String {
        let names = self.theme.color_names();
        let (k, v) = key.split_at(1);
        let label = match (k, v) {
            ("d", "1") => "ONLY 1 DIFFERS",
            ("d", "2") => "2 DIFFER",
            ("d", "3") => "3 DIFFER",
            ("d", _) => "ALL 4 DIFFER",
            ("n", "0") => "ALL SINGLES",
            ("n", "1") => "ALL PAIRS",
            ("n", "2") => "ALL TRIPLES",
            ("n", _) => "MIXED COUNTS",
            ("s", "0") => "ALL DIAMONDS",
            ("s", "1") => "ALL PILLS",
            ("s", "2") => "ALL SQUIGGLES",
            ("s", _) => "MIXED SHAPES",
            ("c", "0") => return format!("ALL {}", names[0].to_uppercase()),
            ("c", "1") => return format!("ALL {}", names[1].to_uppercase()),
            ("c", "2") => return format!("ALL {}", names[2].to_uppercase()),
            ("c", _) => "MIXED COLOURS",
            ("h", "0") => "ALL SOLID",
            ("h", "1") => "ALL STRIPED",
            ("h", "2") => "ALL OPEN",
            _ => "MIXED SHADING",
        };
        label.to_string()
    }

    /// Builds the report on the back of the table.
    fn render_profile(&mut self) {
        const SECTIONS: [(&str, [&str; 4]); 5] = [
            ("HOW MANY ATTRIBUTES DIFFER", ["d1", "d2", "d3", "d4"]),
            ("COLOUR", ["c0", "c1", "c2", "cx"]),
            ("SHAPE", ["s0", "s1", "s2", "sx"]),
            ("COUNT", ["n0", "n1", "n2", "nx"]),
            ("SHADING", ["h0", "h1", "h2", "hx"]),
        ];
        let st = &self.stats;
        let overall = st.overall_rate().unwrap_or(0.0);
        let pct = |r: f64| (r * 100.0).round() as u32;
        let plural =
            |n: u32, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        let secs = |ms: u32| format!("{:.1}S", ms as f64 / 1000.0);

        let biases = st.biases(MIN_SEEN);
        let leans: Vec<&tripos_engine::Bias> = biases
            .iter()
            .filter(|b| b.bias >= MIN_BIAS)
            .take(2)
            .collect();
        let misses: Vec<&tripos_engine::Bias> = biases
            .iter()
            .rev()
            .filter(|b| b.bias <= -MIN_BIAS)
            .take(2)
            .collect();
        let timed: Vec<(&String, u32)> = st
            .cats
            .iter()
            .filter(|(_, t)| t.found >= MIN_FOUND)
            .filter_map(|(k, t)| t.avg_ms().map(|ms| (k, ms)))
            .collect();
        let fastest = timed.iter().min_by_key(|(_, ms)| *ms).copied();
        let slowest = timed.iter().max_by_key(|(_, ms)| *ms).copied();

        let mut h = String::with_capacity(6000);
        h.push_str(r#"<p class="st-kicker">WALLACE CORP.&nbsp;&middot;&nbsp;PLAYER PROFILE</p>"#);
        h.push_str(r#"<h2 class="st-title">BASELINE REPORT</h2>"#);
        h.push_str(r#"<h3 class="st-h">TABLE PLAY&nbsp;&middot;&nbsp;SOLO AND DAILY</h3>"#);
        if st.found == 0 {
            h.push_str(r#"<p class="st-sum">NO SETS ON RECORD YET</p>"#);
        } else {
            h.push_str(&format!(
                r#"<p class="st-sum">{} FOUND{MIDDOT}{} PASSED OVER{MIDDOT}TAKE RATE {}%{MIDDOT}AVG FIND {}</p>"#,
                plural(st.found, "SET", "SETS"),
                st.seen.saturating_sub(st.found),
                pct(overall),
                st.avg_ms().map(secs).unwrap_or_else(|| "-".into())
            ));
            h.push_str(&format!(
                r#"<p class="st-sum st-dim">{}{MIDDOT}{}{MIDDOT}{}</p>"#,
                plural(st.wrong, "WRONG PICK", "WRONG PICKS"),
                plural(st.hints, "HINT", "HINTS"),
                plural(st.bad_deals, "EARLY DEAL", "EARLY DEALS")
            ));
        }
        if st.found < 5 {
            h.push_str(r#"<p class="st-note st-warn">NOT MUCH DATA YET. THE REPORT SHARPENS AFTER A FEW DECKS OF SOLO OR DAILY.</p>"#);
        }

        let describe = |b: &tripos_engine::Bias| {
            format!(
                "<b>{}</b> {}% <span class=\"st-dim\">({}/{})</span>",
                self.cat_label(&b.key),
                pct(b.rate),
                b.tally.found,
                b.tally.seen
            )
        };
        let join = |v: &[&tripos_engine::Bias]| {
            if v.is_empty() {
                "NO CLEAR LEAN YET".to_string()
            } else {
                v.iter()
                    .map(|b| describe(b))
                    .collect::<Vec<_>>()
                    .join(MIDDOT)
            }
        };
        h.push_str(r#"<div class="st-verdicts">"#);
        h.push_str(&format!(
            r#"<p class="st-v"><span class="st-vl">YOU LEAN TOWARD</span>{}</p>"#,
            join(&leans)
        ));
        h.push_str(&format!(
            r#"<p class="st-v"><span class="st-vl">YOU TEND TO MISS</span>{}</p>"#,
            join(&misses)
        ));
        let speed = match (fastest, slowest) {
            (Some((fk, fm)), Some((sk, sm))) if fk != sk => format!(
                "<b>{}</b> {}{MIDDOT}SLOWEST <b>{}</b> {}",
                self.cat_label(fk),
                secs(fm),
                self.cat_label(sk),
                secs(sm)
            ),
            _ => "NOT ENOUGH TIMED FINDS YET".to_string(),
        };
        h.push_str(&format!(
            r#"<p class="st-v"><span class="st-vl">FASTEST FINDS</span>{speed}</p>"#
        ));
        h.push_str("</div>");

        h.push_str(r#"<div class="st-grid">"#);
        for (title, keys) in SECTIONS {
            h.push_str(&format!(r#"<section class="st-sec"><h3>{title}</h3>"#));
            for key in keys {
                let t = st.cats.get(key).copied().unwrap_or_default();
                let cls = if leans.iter().any(|b| b.key == key) {
                    " lean"
                } else if misses.iter().any(|b| b.key == key) {
                    " miss"
                } else {
                    ""
                };
                let (w, num) = match t.rate() {
                    Some(r) => (pct(r), format!("{}/{}", t.found, t.seen)),
                    None => (0, "-".to_string()),
                };
                h.push_str(&format!(
                    r#"<div class="st-row{cls}"><span class="st-label">{}</span><span class="st-bar" style="--w:{w}%;--m:{}%"><i></i><b></b></span><span class="st-num">{num}</span></div>"#,
                    self.cat_label(key),
                    pct(overall)
                ));
            }
            h.push_str("</section>");
        }
        h.push_str("</div>");
        h.push_str(r#"<p class="st-note">BAR: HOW OFTEN YOU TAKE THAT KIND OF SET WHEN IT IS ON THE TABLE. LINE: YOUR OVERALL RATE. A SET IS PASSED OVER WHEN IT WAS OUT AND YOU TOOK A DIFFERENT ONE.</p>"#);
        h.push_str(&self.profile_speed_html(&SECTIONS));
        h.push_str(&format!(
            r#"<div class="st-actions"><button class="btn btn-primary" type="button" data-act="stats">BACK TO THE TABLE</button><button class="btn" type="button" data-act="stats-reset">{}</button></div>"#,
            if self.reset_armed { "SURE? TAP AGAIN" } else { "RESET PROFILE" }
        ));
        self.els.stats_body.set_inner_html(&h);
    }

    // ---------- speed drill ----------

    fn save_speed(&self) {
        dom::local_set(
            SPEED_KEY,
            &to_json(&SpeedSave::from_stats(&self.speed_stats)),
        );
    }

    fn speed_new_round(&mut self) {
        self.cancel_pending();
        self.locked = false;
        self.drag = None;
        let mut rng = Rng::new(dom::random_seed());
        self.speed = Speed::idle();
        self.speed.trials = speed::deal_round(&mut rng);
        self.render_speed_card();
        self.render_stats();
        let best = self.speed_stats.best_ms;
        let sub = format!(
            "SWIPE RIGHT IF THE THREE ARE A SET, LEFT IF THEY ARE NOT. {} TRIPLES. A WRONG SWIPE ADDS {}.{}",
            speed::ROUND_LEN,
            fmt_tenths(speed::WRONG_MS),
            if best > 0 { format!("<br>BEST {}", fmt_tenths(best)) } else { String::new() }
        );
        self.show_overlay(
            Overlay::SpeedStart,
            "SPEED DRILL",
            "SET OR NOT",
            &sub,
            "GO",
            None,
        );
        self.say(&self.idle_line());
    }

    fn speed_go(&mut self) {
        if self.mode != Mode::Speed || self.speed.started {
            return;
        }
        self.speed.started = true;
        let now = dom::now();
        self.speed.start_at = now;
        self.speed.shown_at = now;
        self.close_overlay();
        self.say("GO");
    }

    fn speed_live(&self) -> bool {
        self.mode == Mode::Speed
            && self.speed.started
            && !self.speed.over
            && self.overlay == Overlay::None
            && !self.stats_open
            && !self.options_open
    }

    fn render_speed_card(&self) {
        let mut html = String::new();
        if let Some(t) = self.speed.trials.get(self.speed.pos) {
            for c in t.cards {
                html.push_str(&format!(
                    r#"<div class="speed-face" data-card="{}">{}</div>"#,
                    c.0,
                    cards::face(c)
                ));
            }
        }
        self.els.speed_card.set_inner_html(&html);
        let _ = self.els.speed_card.set_attribute("style", "");
        dom::set_class(&self.els.speed_card, "lean-yes", false);
        dom::set_class(&self.els.speed_card, "lean-no", false);
    }

    /// Answers the current triple: right (true) for a set, left for not.
    fn speed_answer(&mut self, yes: bool) {
        if !self.speed_live() {
            return;
        }
        let Some(trial) = self.speed.trials.get(self.speed.pos).copied() else {
            return;
        };
        let now = dom::now();
        let rt = (now - self.speed.shown_at).max(0.0) as u32;
        let ok = yes == trial.is_set;
        self.speed_stats.record(&trial, rt, ok);
        if ok {
            self.speed.correct += 1;
            self.speed.rts.push(rt);
        } else {
            self.speed.penalty += speed::WRONG_MS;
        }
        self.fly_card(yes, ok);
        self.speed.pos += 1;
        self.drag = None;
        if self.speed.pos >= speed::ROUND_LEN {
            self.speed.over = true;
            self.speed.total =
                ((now - self.speed.start_at).max(0.0) as u32).saturating_add(self.speed.penalty);
            self.speed_stats.finish_round(self.speed.total);
            self.save_speed();
            self.render_speed_card();
            self.render_stats();
            self.show_done();
            return;
        }
        self.speed.shown_at = dom::now();
        self.render_speed_card();
        self.render_stats();
        self.save_speed();
        let line = match (ok, trial.is_set) {
            (true, true) => "SET. RIGHT.",
            (true, false) => "NOT A SET. RIGHT.",
            (false, _) => "WRONG. +2S",
        };
        self.say(line);
    }

    /// The answered card flies off as a ghost so the next one is live at once.
    fn fly_card(&self, yes: bool, ok: bool) {
        let stack = &self.els.speed_stack;
        for old in dom::children(stack) {
            if old.class_list().contains("speed-ghost") {
                old.remove();
            }
        }
        if let Ok(ghost) = self.els.speed_card.clone_node_with_deep(true) {
            if let Ok(ghost) = ghost.dyn_into::<HtmlElement>() {
                ghost.set_id("");
                let _ = ghost.set_attribute("aria-hidden", "true");
                let _ = ghost.set_attribute("style", "");
                ghost.set_class_name(&format!(
                    "speed-card speed-ghost {} {}",
                    if yes { "fly-yes" } else { "fly-no" },
                    if ok { "good" } else { "bad" }
                ));
                let _ = stack.append_child(&ghost);
            }
        }
    }

    fn swipe_start(&mut self, e: &web_sys::PointerEvent) {
        if !self.speed_live() {
            return;
        }
        let card = &self.els.speed_card;
        let _ = card.set_pointer_capture(e.pointer_id());
        dom::set_class(card, "dragging", true);
        self.drag = Some((e.pointer_id(), e.client_x() as f64, 0.0));
    }

    fn swipe_move(&mut self, e: &web_sys::PointerEvent) {
        let Some((id, x0, _)) = self.drag else { return };
        if e.pointer_id() != id {
            return;
        }
        let dx = e.client_x() as f64 - x0;
        self.drag = Some((id, x0, dx));
        let card = &self.els.speed_card;
        dom::set_var(card, "--dx", &format!("{dx:.0}px"));
        dom::set_var(card, "--rot", &format!("{:.1}deg", dx / 18.0));
        dom::set_class(card, "lean-yes", dx > SWIPE_PX / 2.0);
        dom::set_class(card, "lean-no", dx < -SWIPE_PX / 2.0);
    }

    fn swipe_end(&mut self, e: &web_sys::PointerEvent) {
        let Some((id, _, dx)) = self.drag else { return };
        if e.pointer_id() != id {
            return;
        }
        self.drag = None;
        let card = &self.els.speed_card;
        dom::set_class(card, "dragging", false);
        if dx.abs() >= SWIPE_PX {
            self.speed_answer(dx > 0.0);
        } else {
            let _ = card.set_attribute("style", "");
            dom::set_class(card, "lean-yes", false);
            dom::set_class(card, "lean-no", false);
        }
    }

    /// The speed-drill block of the profile: reaction times and accuracy by kind.
    fn profile_speed_html(&self, sections: &[(&str, [&str; 4])]) -> String {
        let st = &self.speed_stats;
        let all = st.overall();
        if all.n == 0 {
            return String::new();
        }
        let pct = |r: f64| (r * 100.0).round() as u32;
        let mut h = String::with_capacity(3000);
        h.push_str(r#"<section class="st-speed"><h3 class="st-h">SPEED DRILL</h3>"#);
        h.push_str(&format!(
            r#"<p class="st-sum">{} {}{MIDDOT}{} {}{MIDDOT}ACCURACY {}%{MIDDOT}MEDIAN REACTION {}{MIDDOT}P90 {}{MIDDOT}BEST ROUND {}</p>"#,
            st.rounds,
            if st.rounds == 1 { "ROUND" } else { "ROUNDS" },
            all.n,
            if all.n == 1 { "SWIPE" } else { "SWIPES" },
            pct(all.accuracy().unwrap_or(0.0)),
            st.rt_percentile(0.5).map(fmt_tenths).unwrap_or_else(|| "-".into()),
            st.rt_percentile(0.9).map(fmt_tenths).unwrap_or_else(|| "-".into()),
            if st.best_ms > 0 { fmt_tenths(st.best_ms) } else { "-".into() }
        ));
        let sets = st.sets();
        let near = st.near_misses();
        let far = st.far_misses();
        let miss_rate = |r: &speed::Reading| r.accuracy().map(|a| pct(1.0 - a));
        h.push_str(r#"<div class="st-verdicts">"#);
        h.push_str(&format!(
            r#"<p class="st-v"><span class="st-vl">SETS YOU MISS</span>{}</p>"#,
            match miss_rate(&sets) {
                Some(m) => format!(
                    "<b>{m}%</b> OF SETS SWIPED AWAY <span class=\"st-dim\">({}/{})</span>",
                    sets.n - sets.correct,
                    sets.n
                ),
                None => "NO SETS SEEN YET".to_string(),
            }
        ));
        h.push_str(&format!(
            r#"<p class="st-v"><span class="st-vl">FALSE ALARMS</span>{}</p>"#,
            match (miss_rate(&near), miss_rate(&far)) {
                (Some(a), Some(b)) =>
                    format!("<b>{a}%</b> ON NEAR MISSES{MIDDOT}<b>{b}%</b> ON CLEAR NON-SETS"),
                (Some(a), None) => format!("<b>{a}%</b> ON NEAR MISSES"),
                (None, Some(b)) => format!("<b>{b}%</b> ON CLEAR NON-SETS"),
                _ => "NO NON-SETS SEEN YET".to_string(),
            }
        ));
        // Fastest and slowest kinds of set to recognise, among kinds seen enough.
        let readings: Vec<(&str, speed::Reading)> = sections
            .iter()
            .flat_map(|(_, keys)| keys.iter().copied())
            .map(|k| (k, st.for_key(k)))
            .filter(|(_, r)| r.correct >= MIN_FOUND)
            .collect();
        let fastest = readings
            .iter()
            .filter_map(|(k, r)| r.p50.map(|p| (*k, p)))
            .min_by_key(|x| x.1);
        let slowest = readings
            .iter()
            .filter_map(|(k, r)| r.p50.map(|p| (*k, p)))
            .max_by_key(|x| x.1);
        h.push_str(&format!(
            r#"<p class="st-v"><span class="st-vl">QUICKEST TO SPOT</span>{}</p>"#,
            match (fastest, slowest) {
                (Some((fk, fp)), Some((sk, sp))) if fk != sk => format!(
                    "<b>{}</b> {}{MIDDOT}SLOWEST <b>{}</b> {}",
                    self.cat_label(fk),
                    fmt_tenths(fp),
                    self.cat_label(sk),
                    fmt_tenths(sp)
                ),
                _ => "NOT ENOUGH SETS SEEN YET".to_string(),
            }
        ));
        h.push_str("</div>");
        let max_p50 = readings
            .iter()
            .filter_map(|(_, r)| r.p50)
            .max()
            .unwrap_or(1)
            .max(1);
        h.push_str(r#"<div class="st-grid">"#);
        for (title, keys) in sections {
            h.push_str(&format!(r#"<section class="st-sec"><h3>{title}</h3>"#));
            for key in keys {
                let r = st.for_key(key);
                let (w, num) = match (r.p50, r.accuracy()) {
                    (Some(p), Some(a)) => (
                        (p as f64 / max_p50 as f64 * 100.0).round() as u32,
                        format!("{}{MIDDOT}{}%", fmt_tenths(p), pct(a)),
                    ),
                    (None, Some(a)) => (0, format!("-{MIDDOT}{}%", pct(a))),
                    _ => (0, "-".to_string()),
                };
                let cls = if fastest.map(|f| f.0 == *key).unwrap_or(false) {
                    " lean"
                } else if slowest.map(|s| s.0 == *key).unwrap_or(false) {
                    " miss"
                } else {
                    ""
                };
                h.push_str(&format!(
                    r#"<div class="st-row{cls}"><span class="st-label">{}</span><span class="st-bar" style="--w:{w}%"><i></i></span><span class="st-num st-wide">{num}</span></div>"#,
                    self.cat_label(key)
                ));
            }
            h.push_str("</section>");
        }
        h.push_str("</div>");
        h.push_str(r#"<p class="st-note">BAR: MEDIAN TIME TO SPOT THAT KIND OF SET IN THE DRILL, LONGER IS SLOWER. PERCENT: HOW OFTEN YOU SWIPED RIGHT ON IT.</p>"#);
        h.push_str("</section>");
        h
    }

    // ---------- deferred work ----------

    fn later(&mut self, ms: i32, f: Later) {
        self.cancel_pending();
        self.pending = Some(f);
        let cb = self.later_cb.as_ref().expect("later closure");
        self.pending_timer = Some(dom::set_timeout(cb, ms));
    }

    fn cancel_pending(&mut self) {
        if let Some(id) = self.pending_timer.take() {
            dom::clear_timeout(id);
        }
        self.pending = None;
    }

    fn run_pending(&mut self) {
        self.pending_timer = None;
        if let Some(f) = self.pending.take() {
            f(self);
        }
    }

    // ---------- share ----------

    fn share_text(&self) -> String {
        let t = self.daily.as_ref().expect("daily");
        format!(
            "TRIPOS DAILY {}{MIDDOT}{} SETS{MIDDOT}{}{MIDDOT}STREAK {}\n{}",
            t.date.clone().unwrap_or_default(),
            t.game.found(),
            fmt_ms(t.total()),
            self.streak(t.date.as_deref().unwrap_or("")),
            dom::page_url()
        )
    }

    fn share(&mut self) {
        let text = self.share_text();
        self.share_plain(text, self.els.secondary.clone());
    }

    /// Shares text with the system sheet, or copies it, and reports on the button.
    fn share_plain(&self, text: String, btn: HtmlButtonElement) {
        let nav = dom::window().navigator();
        let weak = self.me.clone();
        // Feature detection through Reflect keeps this independent of web-sys bindings.
        if let Ok(share) = js_sys::Reflect::get(&nav, &"share".into()) {
            if let Some(share) = share.dyn_ref::<js_sys::Function>() {
                let data = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&data, &"text".into(), &text.clone().into());
                if let Ok(p) = share.call1(&nav, &data) {
                    if let Ok(p) = p.dyn_into::<js_sys::Promise>() {
                        let done = btn.clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            if JsFuture::from(p).await.is_ok() {
                                with_app(&weak, |_| done.set_text_content(Some("SHARED")));
                            }
                        });
                        return;
                    }
                }
            }
        }
        if let Ok(clip) = js_sys::Reflect::get(&nav, &"clipboard".into()) {
            if !clip.is_undefined() && !clip.is_null() {
                if let Ok(write) = js_sys::Reflect::get(&clip, &"writeText".into()) {
                    if let Some(write) = write.dyn_ref::<js_sys::Function>() {
                        if let Ok(p) = write.call1(&clip, &text.into()) {
                            if let Ok(p) = p.dyn_into::<js_sys::Promise>() {
                                let done = btn.clone();
                                wasm_bindgen_futures::spawn_local(async move {
                                    let label = if JsFuture::from(p).await.is_ok() {
                                        "COPIED"
                                    } else {
                                        "COPY FAILED"
                                    };
                                    with_app(&weak, |_| done.set_text_content(Some(label)));
                                });
                                return;
                            }
                        }
                    }
                }
            }
        }
        btn.set_text_content(Some("COPY FAILED"));
    }

    // ---------- online ----------

    fn lobby_status(&self, s: &str) {
        self.els.lobby_status.set_text_content(Some(s));
    }

    /// The lobby's resting state: host or join, no room yet.
    fn lobby_reset(&mut self) {
        dom::set_hidden(&self.els.lobby, false);
        dom::set_hidden(&self.els.lobby_start, false);
        dom::set_hidden(&self.els.lobby_live, true);
        dom::set_hidden(&self.els.lobby_room, true);
        self.els.lobby_title.set_text_content(Some("LINK UP"));
        self.els.lobby_share.set_text_content(Some("SHARE LINK"));
        self.lobby_status("HOST A ROOM, OR JOIN ONE WITH A CODE");
    }

    fn net_handler(&self) -> net::Handler {
        let weak = self.me.clone();
        Rc::new(move |ev: NetEvent| {
            with_app(&weak, |a| a.net_event(ev));
        })
    }

    fn online_connect(&mut self, role: Role, code: String) {
        self.online_leave();
        self.online.me = if role == Role::Host { 0 } else { 1 };
        let handler = self.net_handler();
        match Net::start(
            role,
            code.clone(),
            net::signal_from_url(),
            self.els.remote_audio.clone(),
            handler,
        ) {
            Ok(n) => {
                self.online.net = Some(n);
                dom::set_hidden(&self.els.lobby_start, true);
                dom::set_hidden(&self.els.lobby_live, false);
                dom::set_hidden(&self.els.lobby_room, false);
                dom::set_hidden(&self.els.lobby_share, role != Role::Host);
                self.els.lobby_room.set_text_content(Some(&code));
                self.els
                    .lobby_title
                    .set_text_content(Some(if role == Role::Host {
                        "YOUR ROOM"
                    } else {
                        "JOINING"
                    }));
                self.lobby_status(if role == Role::Host {
                    "STARTING THE ROOM"
                } else {
                    "LOOKING FOR THE HOST"
                });
            }
            Err(_) => self.lobby_status("THIS BROWSER CANNOT MAKE PEER CALLS"),
        }
    }

    fn online_host(&mut self) {
        if self.mode != Mode::Online {
            return;
        }
        let code = net::random_code();
        self.online_connect(Role::Host, code);
    }

    fn online_join(&mut self) {
        if self.mode != Mode::Online || self.online.net.is_some() {
            return;
        }
        let code = net::clean_code(&self.els.lobby_code.value());
        if code.len() != net::CODE_LEN {
            self.lobby_status("ENTER THE 5-CHARACTER ROOM CODE");
            return;
        }
        self.els.lobby_code.set_value(&code);
        self.online_connect(Role::Guest, code);
    }

    /// Drops the call and the game; the lobby comes back on the next render.
    fn online_leave(&mut self) {
        if let Some(n) = self.online.net.take() {
            n.close();
            self.dead_nets.push(n);
        }
        let me = self.online.me;
        self.online = Online::idle();
        self.online.me = me;
        self.cancel_pending();
        self.locked = false;
        self.selected.clear();
        self.render_mic();
    }

    fn net_send(&self, msg: &Wire) {
        if let (Some(n), Ok(text)) = (self.online.net.as_ref(), serde_json::to_string(msg)) {
            let _ = n.send(&text);
        }
    }

    fn net_event(&mut self, ev: NetEvent) {
        if self.mode != Mode::Online {
            return;
        }
        match ev {
            NetEvent::Status(s) => self.lobby_status(&s),
            NetEvent::Mic(on) => {
                self.online.mic = Some(on);
                self.render_mic();
            }
            NetEvent::Connected => {
                if self.online.me == 0 {
                    let seed = dom::random_seed();
                    self.online_start(seed);
                    self.net_send(&Wire::Start { seed });
                } else {
                    self.lobby_status("CONNECTED. WAITING FOR THE DEAL");
                }
            }
            NetEvent::Data(text) => {
                if let Ok(msg) = serde_json::from_str::<Wire>(&text) {
                    self.on_wire(msg);
                }
            }
            NetEvent::Closed => {
                let was_playing = self.online.started;
                self.online_leave();
                self.lobby_reset();
                self.lobby_status(if was_playing {
                    "CONNECTION LOST"
                } else {
                    "THE CALL DID NOT CONNECT"
                });
                self.close_overlay();
                self.render_all();
                self.say(&self.idle_line());
            }
        }
    }

    /// Both sides deal the same deck from the host's seed.
    fn online_start(&mut self, seed: u64) {
        self.cancel_pending();
        self.locked = false;
        self.selected.clear();
        self.hinted.clear();
        self.online.game = Game::new(&mut Rng::new(seed));
        self.online.scores = [0; 2];
        self.online.started = true;
        self.online.over = false;
        self.online.pending = None;
        self.fresh = self.online.game.table().iter().copied().collect();
        dom::set_hidden(&self.els.lobby, true);
        self.close_overlay();
        self.render_all();
        self.say(&self.idle_line());
    }

    fn on_wire(&mut self, msg: Wire) {
        let host = self.online.me == 0;
        match msg {
            Wire::Start { seed } if !host => self.online_start(seed),
            Wire::Claim { cards } if host => {
                let cards = cards.map(Card);
                if self.indices_of(cards).is_some() {
                    self.online_apply_claim(1, cards);
                    self.net_send(&Wire::Claimed {
                        by: 1,
                        cards: cards.map(|c| c.0),
                    });
                } else {
                    self.net_send(&Wire::Lost);
                }
            }
            Wire::Claimed { by, cards } if !host => {
                self.online_apply_claim(by as usize, cards.map(Card));
            }
            Wire::Lost if !host => {
                self.online.pending = None;
                self.finish_bad();
                self.say("TOO SLOW: THE HOST TOOK IT");
            }
            Wire::Wrong { by } => {
                let by = (by as usize).min(1);
                if by != self.online.me {
                    self.online.scores[by] -= 1;
                    self.render_stats();
                    self.say(&format!("THEM{MIDDOT}NOT A SET{MIDDOT}-1"));
                }
            }
            Wire::Deal if host => {
                if self.online.game.has_set() {
                    self.net_send(&Wire::NoDeal);
                } else if self.online_apply_deal() {
                    self.net_send(&Wire::Dealt);
                }
            }
            Wire::Dealt if !host => {
                self.online_apply_deal();
            }
            Wire::NoDeal if !host => self.say("A SET IS OUT THERE"),
            _ => {}
        }
    }

    /// Where three cards sit on this side's table, if all three are still out.
    fn indices_of(&self, cards: [Card; 3]) -> Option<[usize; 3]> {
        let table = self.game().table();
        let at = |c: Card| table.iter().position(|&t| t == c);
        Some([at(cards[0])?, at(cards[1])?, at(cards[2])?])
    }

    fn online_host_claim(&mut self, cards: [Card; 3]) {
        if self.indices_of(cards).is_some() {
            self.online_apply_claim(0, cards);
            self.net_send(&Wire::Claimed {
                by: 0,
                cards: cards.map(|c| c.0),
            });
        } else {
            self.finish_bad();
        }
    }

    fn online_apply_claim(&mut self, by: usize, cards: [Card; 3]) {
        let Some(idx) = self.indices_of(cards) else {
            return;
        };
        let before: HashSet<Card> = self.game().table().iter().copied().collect();
        if self.online.game.claim(idx) != Claim::Set {
            return;
        }
        self.online.scores[by.min(1)] += 1;
        self.online.pending = None;
        self.selected.clear();
        self.locked = false;
        self.fresh = self
            .game()
            .table()
            .iter()
            .copied()
            .filter(|c| !before.contains(c))
            .collect();
        self.render_all();
        let who = if by == self.online.me { "YOU" } else { "THEM" };
        self.say(&format!("{who}{MIDDOT}SET{MIDDOT}+1"));
        self.check_over();
    }

    fn online_apply_deal(&mut self) -> bool {
        let before: HashSet<Card> = self.game().table().iter().copied().collect();
        if !self.online.game.deal_three() {
            self.check_over();
            return false;
        }
        self.fresh = self
            .game()
            .table()
            .iter()
            .copied()
            .filter(|c| !before.contains(c))
            .collect();
        self.selected.clear();
        self.render_all();
        self.say("NO SET: THREE MORE CARDS");
        true
    }

    fn toggle_mic(&mut self) {
        let Some(n) = self.online.net.clone() else {
            return;
        };
        let on = !self.online.mic.unwrap_or(false);
        if n.has_mic() {
            n.set_mic(on);
            self.online.mic = Some(on);
            self.announce(if on {
                "Microphone on."
            } else {
                "Microphone muted."
            });
        }
        self.render_mic();
    }

    fn render_mic(&self) {
        let label = match (self.online.net.as_ref(), self.online.mic) {
            (None, _) => "MIC",
            (Some(n), _) if !n.has_mic() => "NO MIC",
            (Some(_), Some(true)) => "MIC ON",
            (Some(_), _) => "MUTED",
        };
        self.els.mic.set_text_content(Some(label));
        let _ = self.els.mic.set_attribute(
            "aria-pressed",
            if self.online.mic == Some(true) {
                "true"
            } else {
                "false"
            },
        );
        self.els.mic.set_disabled(
            self.online
                .net
                .as_ref()
                .map(|n| !n.has_mic())
                .unwrap_or(true),
        );
    }
}
