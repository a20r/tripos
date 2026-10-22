//! Peer-to-peer transport for the online mode: a WebRTC data channel carries
//! the moves and an audio track carries voice. The two browsers find each
//! other through a room code on a tiny pub/sub channel: ntfy.sh in the wild,
//! or a BroadcastChannel when both tabs are in one browser (used by the tests).
//!
//! The host publishes an offer, the guest answers, ICE candidates trickle
//! both ways, and every signalling message carries the sender's id so each
//! side can ignore its own echoes.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{
    BroadcastChannel, HtmlAudioElement, MediaStream, MediaStreamConstraints, MediaStreamTrack,
    MessageEvent, Request, RequestInit, RtcConfiguration, RtcDataChannel, RtcDataChannelEvent,
    RtcIceCandidateInit, RtcIceServer, RtcPeerConnection, RtcPeerConnectionIceEvent,
    RtcPeerConnectionState, RtcRtpTransceiverDirection, RtcRtpTransceiverInit, RtcSdpType,
    RtcSessionDescriptionInit, RtcTrackEvent, WebSocket,
};

use crate::dom;

/// Public pub/sub broker used for signalling. Any ntfy server works here.
pub const NTFY: &str = "https://ntfy.sh";
const TOPIC_PREFIX: &str = "tripos-";
/// ntfy caps a message at 4 KiB; longer signals are split into parts.
const PART_LEN: usize = 3000;
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
pub const CODE_LEN: usize = 5;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Host,
    Guest,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Signal {
    Ntfy,
    Local,
}

/// What the transport reports back to the game.
#[derive(Debug)]
pub enum NetEvent {
    Status(String),
    Connected,
    Data(String),
    Closed,
    Mic(bool),
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    from: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    sdp: String,
    #[serde(default)]
    candidate: String,
    #[serde(default)]
    mid: Option<String>,
    #[serde(default)]
    mline: Option<u16>,
    // Long signals split into parts.
    #[serde(default)]
    id: String,
    #[serde(default)]
    i: usize,
    #[serde(default)]
    n: usize,
    #[serde(default)]
    data: String,
}

impl Envelope {
    fn new(from: &str, kind: &str) -> Envelope {
        Envelope {
            from: from.to_string(),
            kind: kind.to_string(),
            sdp: String::new(),
            candidate: String::new(),
            mid: None,
            mline: None,
            id: String::new(),
            i: 0,
            n: 0,
            data: String::new(),
        }
    }
}

pub type Handler = Rc<dyn Fn(NetEvent)>;

pub struct Net {
    pub role: Role,
    pub code: String,
    id: String,
    signal: Signal,
    pc: RtcPeerConnection,
    dc: RefCell<Option<RtcDataChannel>>,
    ws: RefCell<Option<WebSocket>>,
    bc: RefCell<Option<BroadcastChannel>>,
    local: RefCell<Option<MediaStream>>,
    audio: HtmlAudioElement,
    remote_set: Cell<bool>,
    /// The microphone has been asked for, granted or not.
    media_done: Cell<bool>,
    /// A description exchange is in flight, so a repeated offer or answer is ignored.
    negotiating: Cell<bool>,
    pending_ice: RefCell<Vec<RtcIceCandidateInit>>,
    parts: RefCell<HashMap<String, Vec<Option<String>>>>,
    closed: Cell<bool>,
    handler: Handler,
    // Event closures live as long as the transport.
    keep: RefCell<Vec<Box<dyn std::any::Any>>>,
}

pub fn random_code() -> String {
    (0..CODE_LEN)
        .map(|_| {
            let i = (js_sys::Math::random() * CODE_ALPHABET.len() as f64) as usize;
            CODE_ALPHABET[i.min(CODE_ALPHABET.len() - 1)] as char
        })
        .collect()
}

/// Normalises typed room codes: upper case, letters and digits only.
pub fn clean_code(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .take(CODE_LEN)
        .collect()
}

fn random_id() -> String {
    format!("{:08x}", (js_sys::Math::random() * 4294967296.0) as u32)
}

impl Net {
    /// Opens the signalling channel and starts the WebRTC handshake. The host
    /// publishes an offer at once; the guest waits for one.
    pub fn start(
        role: Role,
        code: String,
        signal: Signal,
        audio: HtmlAudioElement,
        handler: Handler,
    ) -> Result<Rc<Net>, JsValue> {
        let config = RtcConfiguration::new();
        let servers = js_sys::Array::new();
        for url in [
            "stun:stun.l.google.com:19302",
            "stun:stun1.l.google.com:19302",
        ] {
            let s = RtcIceServer::new();
            s.set_urls(&JsValue::from_str(url));
            servers.push(&s);
        }
        config.set_ice_servers(&servers);
        let pc = RtcPeerConnection::new_with_configuration(&config)?;
        let net = Rc::new(Net {
            role,
            code,
            id: random_id(),
            signal,
            pc,
            dc: RefCell::new(None),
            ws: RefCell::new(None),
            bc: RefCell::new(None),
            local: RefCell::new(None),
            audio,
            remote_set: Cell::new(false),
            media_done: Cell::new(false),
            negotiating: Cell::new(false),
            pending_ice: RefCell::new(Vec::new()),
            parts: RefCell::new(HashMap::new()),
            closed: Cell::new(false),
            handler,
            keep: RefCell::new(Vec::new()),
        });
        net.wire_pc();
        net.open_signal()?;
        // The host gets its microphone before offering; a guest gets it when
        // the offer arrives, so the answer carries the voice track.
        if role == Role::Host {
            let me = Rc::downgrade(&net);
            spawn_local(async move {
                if let Some(net) = me.upgrade() {
                    net.setup_media().await;
                    net.make_offer().await;
                }
            });
        }
        Ok(net)
    }

    fn emit(&self, ev: NetEvent) {
        (self.handler)(ev);
    }

    fn status(&self, s: &str) {
        self.emit(NetEvent::Status(s.to_string()));
    }

    fn keep<T: 'static>(&self, c: T) {
        self.keep.borrow_mut().push(Box::new(c));
    }

    fn topic(&self) -> String {
        format!("{TOPIC_PREFIX}{}", self.code.to_lowercase())
    }

    // ---------- peer connection ----------

    fn wire_pc(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        let on_ice = Closure::wrap(Box::new(move |e: RtcPeerConnectionIceEvent| {
            let Some(net) = weak.upgrade() else { return };
            if let Some(c) = e.candidate() {
                let mut env = Envelope::new(&net.id, "ice");
                env.candidate = c.candidate();
                env.mid = c.sdp_mid();
                env.mline = c.sdp_m_line_index();
                net.publish(&env);
            }
        }) as Box<dyn FnMut(RtcPeerConnectionIceEvent)>);
        self.pc
            .set_onicecandidate(Some(on_ice.as_ref().unchecked_ref()));
        self.keep(on_ice);

        let weak = Rc::downgrade(self);
        let on_state = Closure::wrap(Box::new(move || {
            let Some(net) = weak.upgrade() else { return };
            match net.pc.connection_state() {
                RtcPeerConnectionState::Connected => net.status("CONNECTED"),
                RtcPeerConnectionState::Disconnected
                | RtcPeerConnectionState::Failed
                | RtcPeerConnectionState::Closed => {
                    net.on_closed();
                }
                _ => {}
            }
        }) as Box<dyn FnMut()>);
        self.pc
            .set_onconnectionstatechange(Some(on_state.as_ref().unchecked_ref()));
        self.keep(on_state);

        let weak = Rc::downgrade(self);
        let on_track = Closure::wrap(Box::new(move |e: RtcTrackEvent| {
            let Some(net) = weak.upgrade() else { return };
            let streams = e.streams();
            if let Some(stream) = streams.get(0).dyn_ref::<MediaStream>() {
                net.audio.set_src_object(Some(stream));
                let _ = net.audio.play();
            }
        }) as Box<dyn FnMut(RtcTrackEvent)>);
        self.pc.set_ontrack(Some(on_track.as_ref().unchecked_ref()));
        self.keep(on_track);

        if self.role == Role::Guest {
            let weak = Rc::downgrade(self);
            let on_dc = Closure::wrap(Box::new(move |e: RtcDataChannelEvent| {
                if let Some(net) = weak.upgrade() {
                    net.adopt_channel(e.channel());
                }
            }) as Box<dyn FnMut(RtcDataChannelEvent)>);
            self.pc
                .set_ondatachannel(Some(on_dc.as_ref().unchecked_ref()));
            self.keep(on_dc);
        }
    }

    fn adopt_channel(self: &Rc<Self>, dc: RtcDataChannel) {
        let weak = Rc::downgrade(self);
        let on_open = Closure::wrap(Box::new(move || {
            if let Some(net) = weak.upgrade() {
                net.emit(NetEvent::Connected);
            }
        }) as Box<dyn FnMut()>);
        dc.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        self.keep(on_open);

        let weak = Rc::downgrade(self);
        let on_msg = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let (Some(net), Some(text)) = (weak.upgrade(), e.data().as_string()) {
                net.emit(NetEvent::Data(text));
            }
        }) as Box<dyn FnMut(MessageEvent)>);
        dc.set_onmessage(Some(on_msg.as_ref().unchecked_ref()));
        self.keep(on_msg);

        let weak = Rc::downgrade(self);
        let on_close = Closure::wrap(Box::new(move || {
            if let Some(net) = weak.upgrade() {
                net.on_closed();
            }
        }) as Box<dyn FnMut()>);
        dc.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        self.keep(on_close);

        *self.dc.borrow_mut() = Some(dc);
    }

    /// Asks for the microphone and adds it to the call. Without one the call
    /// still receives the other side's voice.
    async fn setup_media(self: &Rc<Self>) {
        if self.media_done.replace(true) {
            return;
        }
        let stream = match dom::window().navigator().media_devices() {
            Ok(md) => {
                let c = MediaStreamConstraints::new();
                c.set_audio(&JsValue::TRUE);
                match md.get_user_media_with_constraints(&c) {
                    Ok(p) => JsFuture::from(p)
                        .await
                        .ok()
                        .and_then(|v| v.dyn_into::<MediaStream>().ok()),
                    Err(_) => None,
                }
            }
            Err(_) => None,
        };
        match stream {
            Some(stream) => {
                let tracks = stream.get_audio_tracks();
                for i in 0..tracks.length() {
                    if let Some(track) = tracks.get(i).dyn_ref::<MediaStreamTrack>() {
                        let _ = self.pc.add_track(track, &stream, &js_sys::Array::new());
                    }
                }
                *self.local.borrow_mut() = Some(stream);
                self.emit(NetEvent::Mic(true));
            }
            None => {
                if self.role == Role::Host {
                    let init = RtcRtpTransceiverInit::new();
                    init.set_direction(RtcRtpTransceiverDirection::Recvonly);
                    let _ = self.pc.add_transceiver_with_str_and_init("audio", &init);
                }
                self.emit(NetEvent::Mic(false));
            }
        }
    }

    async fn make_offer(self: &Rc<Self>) {
        let dc = self.pc.create_data_channel("tripos");
        self.adopt_channel(dc);
        let Ok(offer) = JsFuture::from(self.pc.create_offer()).await else {
            self.status("COULD NOT CREATE AN OFFER");
            return;
        };
        let offer: RtcSessionDescriptionInit = offer.unchecked_into();
        if JsFuture::from(self.pc.set_local_description(&offer))
            .await
            .is_err()
        {
            self.status("COULD NOT START THE CALL");
            return;
        }
        let sdp = js_sys::Reflect::get(&offer, &"sdp".into())
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        let mut env = Envelope::new(&self.id, "offer");
        env.sdp = sdp;
        self.publish(&env);
        self.status("WAITING FOR A FRIEND TO JOIN");
    }

    async fn take_offer(self: &Rc<Self>, sdp: String) {
        if self.remote_set.get() || self.negotiating.replace(true) {
            return;
        }
        self.setup_media().await;
        let desc = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
        desc.set_sdp(&sdp);
        if JsFuture::from(self.pc.set_remote_description(&desc))
            .await
            .is_err()
        {
            self.status("BAD OFFER FROM THE HOST");
            return;
        }
        self.remote_set.set(true);
        self.flush_ice().await;
        let Ok(answer) = JsFuture::from(self.pc.create_answer()).await else {
            self.status("COULD NOT ANSWER");
            return;
        };
        let answer: RtcSessionDescriptionInit = answer.unchecked_into();
        if JsFuture::from(self.pc.set_local_description(&answer))
            .await
            .is_err()
        {
            self.status("COULD NOT ANSWER");
            return;
        }
        let sdp = js_sys::Reflect::get(&answer, &"sdp".into())
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        let mut env = Envelope::new(&self.id, "answer");
        env.sdp = sdp;
        self.publish(&env);
        self.status("CONNECTING");
    }

    async fn take_answer(self: &Rc<Self>, sdp: String) {
        if self.remote_set.get() || self.negotiating.replace(true) {
            return;
        }
        let desc = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
        desc.set_sdp(&sdp);
        if JsFuture::from(self.pc.set_remote_description(&desc))
            .await
            .is_err()
        {
            self.status("BAD ANSWER FROM THE GUEST");
            return;
        }
        self.remote_set.set(true);
        self.flush_ice().await;
        self.status("CONNECTING");
    }

    async fn flush_ice(self: &Rc<Self>) {
        let pending: Vec<RtcIceCandidateInit> = self.pending_ice.borrow_mut().drain(..).collect();
        for c in pending {
            let _ = JsFuture::from(
                self.pc
                    .add_ice_candidate_with_opt_rtc_ice_candidate_init(Some(&c)),
            )
            .await;
        }
    }

    fn take_ice(self: &Rc<Self>, env: &Envelope) {
        let init = RtcIceCandidateInit::new(&env.candidate);
        init.set_sdp_mid(env.mid.as_deref());
        init.set_sdp_m_line_index(env.mline);
        if self.remote_set.get() {
            let _ = self
                .pc
                .add_ice_candidate_with_opt_rtc_ice_candidate_init(Some(&init));
        } else {
            self.pending_ice.borrow_mut().push(init);
        }
    }

    // ---------- signalling ----------

    fn open_signal(self: &Rc<Self>) -> Result<(), JsValue> {
        match self.signal {
            Signal::Local => {
                let bc = BroadcastChannel::new(&self.topic())?;
                let weak = Rc::downgrade(self);
                let on_msg = Closure::wrap(Box::new(move |e: MessageEvent| {
                    if let (Some(net), Some(text)) = (weak.upgrade(), e.data().as_string()) {
                        net.on_signal(&text);
                    }
                }) as Box<dyn FnMut(MessageEvent)>);
                bc.set_onmessage(Some(on_msg.as_ref().unchecked_ref()));
                self.keep(on_msg);
                *self.bc.borrow_mut() = Some(bc);
            }
            Signal::Ntfy => {
                let base = NTFY.replacen("https://", "wss://", 1);
                // since= replays the host's offer for a guest who arrives later.
                let url = format!("{base}/{}/ws?since=30m", self.topic());
                let ws = WebSocket::new(&url)?;
                let weak = Rc::downgrade(self);
                let on_msg = Closure::wrap(Box::new(move |e: MessageEvent| {
                    let (Some(net), Some(text)) = (weak.upgrade(), e.data().as_string()) else {
                        return;
                    };
                    // ntfy wraps each message: {"event":"message","message":"..."}
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                        if v.get("event").and_then(|e| e.as_str()) == Some("message") {
                            if let Some(m) = v.get("message").and_then(|m| m.as_str()) {
                                net.on_signal(m);
                            }
                        }
                    }
                }) as Box<dyn FnMut(MessageEvent)>);
                ws.set_onmessage(Some(on_msg.as_ref().unchecked_ref()));
                self.keep(on_msg);
                let weak = Rc::downgrade(self);
                let on_err = Closure::wrap(Box::new(move || {
                    if let Some(net) = weak.upgrade() {
                        if !net.remote_set.get() {
                            net.status("CANNOT REACH THE SIGNAL SERVER");
                        }
                    }
                }) as Box<dyn FnMut()>);
                ws.set_onerror(Some(on_err.as_ref().unchecked_ref()));
                self.keep(on_err);
                *self.ws.borrow_mut() = Some(ws);
            }
        }
        // A guest announces itself so a host that already published can repeat its offer.
        if self.role == Role::Guest {
            self.publish(&Envelope::new(&self.id, "hello"));
        }
        Ok(())
    }

    /// Repeats the current offer, candidates included, for a guest who arrived late.
    fn republish_offer(self: &Rc<Self>) {
        if self.remote_set.get() {
            return;
        }
        if let Some(desc) = self.pc.local_description() {
            let mut env = Envelope::new(&self.id, "offer");
            env.sdp = desc.sdp();
            self.publish(&env);
        }
    }

    fn publish(self: &Rc<Self>, env: &Envelope) {
        let text = serde_json::to_string(env).unwrap_or_default();
        if text.len() <= PART_LEN {
            self.publish_raw(text);
            return;
        }
        let id = random_id();
        let chunks: Vec<String> = text
            .as_bytes()
            .chunks(PART_LEN)
            .map(|c| String::from_utf8_lossy(c).into_owned())
            .collect();
        let n = chunks.len();
        for (i, data) in chunks.into_iter().enumerate() {
            let mut part = Envelope::new(&self.id, "part");
            part.id = id.clone();
            part.i = i;
            part.n = n;
            part.data = data;
            self.publish_raw(serde_json::to_string(&part).unwrap_or_default());
        }
    }

    fn publish_raw(self: &Rc<Self>, text: String) {
        match self.signal {
            Signal::Local => {
                if let Some(bc) = self.bc.borrow().as_ref() {
                    let _ = bc.post_message(&JsValue::from_str(&text));
                }
            }
            Signal::Ntfy => {
                let url = format!("{NTFY}/{}", self.topic());
                spawn_local(async move {
                    let init = RequestInit::new();
                    init.set_method("POST");
                    init.set_body(&JsValue::from_str(&text));
                    if let Ok(req) = Request::new_with_str_and_init(&url, &init) {
                        let _ = JsFuture::from(dom::window().fetch_with_request(&req)).await;
                    }
                });
            }
        }
    }

    fn on_signal(self: &Rc<Self>, text: &str) {
        let Ok(env) = serde_json::from_str::<Envelope>(text) else {
            return;
        };
        if env.from == self.id || self.closed.get() {
            return;
        }
        match env.kind.as_str() {
            "part" => {
                let whole = {
                    let mut parts = self.parts.borrow_mut();
                    let slots = parts
                        .entry(env.id.clone())
                        .or_insert_with(|| vec![None; env.n.max(1)]);
                    if env.i < slots.len() {
                        slots[env.i] = Some(env.data.clone());
                    }
                    if slots.iter().all(|s| s.is_some()) {
                        let joined: String = slots
                            .iter()
                            .map(|s| s.clone().unwrap_or_default())
                            .collect();
                        parts.remove(&env.id);
                        Some(joined)
                    } else {
                        None
                    }
                };
                if let Some(whole) = whole {
                    self.on_signal(&whole);
                }
            }
            "hello" if self.role == Role::Host => self.republish_offer(),
            "offer" if self.role == Role::Guest => {
                let me = self.clone();
                spawn_local(async move { me.take_offer(env.sdp).await });
            }
            "answer" if self.role == Role::Host => {
                let me = self.clone();
                spawn_local(async move { me.take_answer(env.sdp).await });
            }
            "ice" => self.take_ice(&env),
            _ => {}
        }
    }

    // ---------- game traffic ----------

    pub fn send(&self, text: &str) -> bool {
        match self.dc.borrow().as_ref() {
            Some(dc) if dc.ready_state() == web_sys::RtcDataChannelState::Open => {
                dc.send_with_str(text).is_ok()
            }
            _ => false,
        }
    }

    pub fn has_mic(&self) -> bool {
        self.local.borrow().is_some()
    }

    /// Mutes or unmutes the local microphone; returns the new state.
    pub fn set_mic(&self, on: bool) -> bool {
        let Some(stream) = self.local.borrow().clone() else {
            return false;
        };
        let tracks = stream.get_audio_tracks();
        for i in 0..tracks.length() {
            if let Some(t) = tracks.get(i).dyn_ref::<MediaStreamTrack>() {
                t.set_enabled(on);
            }
        }
        on
    }

    fn on_closed(&self) {
        if self.closed.replace(true) {
            return;
        }
        self.emit(NetEvent::Closed);
    }

    /// Unhooks every handler before closing, so nothing fires into a dropped closure.
    pub fn close(&self) {
        self.closed.set(true);
        self.pc.set_onicecandidate(None);
        self.pc.set_onconnectionstatechange(None);
        self.pc.set_ontrack(None);
        self.pc.set_ondatachannel(None);
        if let Some(dc) = self.dc.borrow_mut().take() {
            dc.set_onopen(None);
            dc.set_onmessage(None);
            dc.set_onclose(None);
            dc.close();
        }
        self.pc.close();
        if let Some(ws) = self.ws.borrow_mut().take() {
            ws.set_onmessage(None);
            ws.set_onerror(None);
            let _ = ws.close();
        }
        if let Some(bc) = self.bc.borrow_mut().take() {
            bc.set_onmessage(None);
            bc.close();
        }
        if let Some(stream) = self.local.borrow_mut().take() {
            let tracks = stream.get_tracks();
            for i in 0..tracks.length() {
                if let Some(t) = tracks.get(i).dyn_ref::<MediaStreamTrack>() {
                    t.stop();
                }
            }
        }
        self.audio.set_src_object(None);
    }
}

impl Drop for Net {
    fn drop(&mut self) {
        self.close();
    }
}

/// The room link for sharing: this page with ?join=CODE.
pub fn join_link(code: &str) -> String {
    format!("{}?join={code}", dom::page_url())
}

/// A code passed in the page URL, if any.
pub fn code_from_url() -> Option<String> {
    let search = dom::window().location().search().ok()?;
    let params = web_sys::UrlSearchParams::new_with_str(&search).ok()?;
    params
        .get("join")
        .map(|c| clean_code(&c))
        .filter(|c| c.len() == CODE_LEN)
}

/// `?signal=local` switches signalling to a BroadcastChannel for tests.
pub fn signal_from_url() -> Signal {
    let local = dom::window()
        .location()
        .search()
        .ok()
        .and_then(|s| web_sys::UrlSearchParams::new_with_str(&s).ok())
        .and_then(|p| p.get("signal"))
        .map(|v| v == "local")
        .unwrap_or(false);
    if local {
        Signal::Local
    } else {
        Signal::Ntfy
    }
}
