//! Screen mirroring: the iPhone's screen and sound in the app, and mouse/keyboard control.
//!
//! iOS 27+ streams its display through `com.apple.coredevice.displayservice` (the service Xcode
//! uses) as HEVC over RTP, and its sound as AAC-ELD, inside the developer tunnel. Touches,
//! buttons and keys go back through the CoreDevice HID services, which only accept input while
//! a stream is running. Each session runs on its own thread with its own tunnel, like the
//! location session (the idevice futures are not `Send`).
//!
//! Clients (the Mirror tab, or other programs) read a WebSocket at `/api/screen/ws`:
//! * binary `[1][width u16][height u16][codec length u8][codec][hvcC]`: decoder setup;
//! * binary `[2][key u8][timestamp µs u64][frame as 4-byte length-prefixed NAL units]`;
//! * binary `[3][timestamp µs u64][PCM s16le, 48 kHz, stereo]`;
//! * text: the status JSON whenever it changes.
//!
//! and send input as JSON (the same body `POST /api/screen/input` takes), see [`parse_input`].

pub mod hevc;

use serde::Serialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot};

/// Hardware buttons: (name, HID usage page, usage, hold in ms). Siri needs a long press.
pub const BUTTONS: [(&str, u64, u64, u64); 6] = [
    ("home", 0x0C, 0x40, 50),
    // shortcut: lock uses the side button's usage with a longer press; unconfirmed on a real iPhone, change the usage if it still does nothing.
    ("lock", 0x0C, 0x30, 250),
    ("volume-up", 0x0C, 0xE9, 50),
    ("volume-down", 0x0C, 0xEA, 50),
    ("mute", 0x0C, 0xE2, 50),
    ("siri", 0x0C, 0xCF, 1000),
];

/// System gestures: name, start and end (fractions of the upright screen), drag time and hold in ms.
pub const GESTURES: [(&str, (f64, f64), (f64, f64), u64, u64); 5] = [
    ("home", (0.5, 0.995), (0.5, 0.55), 200, 0),
    ("app_switcher", (0.5, 0.995), (0.5, 0.62), 260, 450),
    ("control_centre", (0.92, 0.004), (0.92, 0.45), 260, 0),
    ("notification_centre", (0.3, 0.004), (0.3, 0.6), 260, 0),
    ("back", (0.004, 0.5), (0.65, 0.5), 260, 0),
];

#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// One finger (or two, for pinching): down, move or up. Coordinates are 0..65535 across the screen.
    Touch { phase: Phase, x: u16, y: u16, second: Option<(u16, u16)> },
    Tap { x: u16, y: u16 },
    Swipe { x1: u16, y1: u16, x2: u16, y2: u16, ms: u64 },
    LongPress { x: u16, y: u16, ms: u64 },
    Gesture(&'static str),
    Button(&'static str, u64, u64, u64),
    /// A HID keyboard usage (page 7): down, up, or (None) pressed and released; `command` holds ⌘.
    Key { usage: u64, down: Option<bool>, command: bool },
    Text(String),
    /// Ask the phone for a fresh full frame (a new viewer, or a decoder error).
    KeyFrame,
    /// Restart the stream (a frozen picture).
    Refresh,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Phase {
    Down,
    Move,
    Up,
}

fn coord(v: &Value, k: &str) -> Result<u16, String> {
    let f = v.get(k).and_then(Value::as_f64).ok_or(format!("'{k}' must be a number from 0 to 1"))?;
    if !(0.0..=1.0).contains(&f) {
        return Err(format!("'{k}' must be from 0 to 1 (a fraction of the screen)"));
    }
    Ok((f * 65535.0).round() as u16)
}

/// Parse one input message. Coordinates are fractions of the screen (0..1, top left is 0,0).
///
/// `{"t":"touch","phase":"down|move|up","x":0.5,"y":0.5}` (plus `"x2","y2"` for a second finger),
/// `{"t":"tap","x":..,"y":..}`, `{"t":"swipe","x1":..,"y1":..,"x2":..,"y2":..,"ms":300}`,
/// `{"t":"long_press","x":..,"y":..,"ms":800}`, `{"t":"gesture","name":"app_switcher"}`, `{"t":"button","name":"home"}`,
/// `{"t":"key","usage":40,"down":true}` (no `down`: press and release; `"command":true` holds ⌘),
/// `{"t":"text","text":"hello"}`, `{"t":"keyframe"}`, `{"t":"refresh"}`.
pub fn parse_input(v: &Value) -> Result<Input, String> {
    let t = v.get("t").and_then(Value::as_str).unwrap_or("");
    Ok(match t {
        "touch" => Input::Touch {
            phase: match v.get("phase").and_then(Value::as_str) {
                Some("down") => Phase::Down,
                Some("move") => Phase::Move,
                Some("up") => Phase::Up,
                _ => return Err("'phase' must be down, move or up".into()),
            },
            x: coord(v, "x")?,
            y: coord(v, "y")?,
            second: if v.get("x2").is_some() || v.get("y2").is_some() { Some((coord(v, "x2")?, coord(v, "y2")?)) } else { None },
        },
        "tap" => Input::Tap { x: coord(v, "x")?, y: coord(v, "y")? },
        "swipe" => Input::Swipe {
            x1: coord(v, "x1")?,
            y1: coord(v, "y1")?,
            x2: coord(v, "x2")?,
            y2: coord(v, "y2")?,
            ms: v.get("ms").and_then(Value::as_u64).unwrap_or(300).clamp(50, 5000),
        },
        "long_press" => Input::LongPress { x: coord(v, "x")?, y: coord(v, "y")?, ms: v.get("ms").and_then(Value::as_u64).unwrap_or(800).clamp(100, 5000) },
        "gesture" => {
            let name = v.get("name").and_then(Value::as_str).unwrap_or("");
            let g = GESTURES.iter().find(|g| g.0 == name).ok_or(format!(
                "Unknown gesture '{name}'. Use one of: home, app_switcher, control_centre, notification_centre, back"
            ))?;
            Input::Gesture(g.0)
        }
        "button" => {
            let name = v.get("name").and_then(Value::as_str).unwrap_or("");
            let &(n, page, usage, hold) = BUTTONS
                .iter()
                .find(|b| b.0 == name)
                .ok_or(format!("Unknown button '{name}'. Use one of: home, lock, volume-up, volume-down, mute, siri"))?;
            Input::Button(n, page, usage, hold)
        }
        "key" => Input::Key {
            usage: v.get("usage").and_then(Value::as_u64).filter(|u| (4..=0xE7).contains(u)).ok_or("'usage' must be a HID keyboard usage (4–231)")?,
            down: match v.get("down") {
                None => None,
                Some(d) => Some(d.as_bool().ok_or("'down' must be true or false")?),
            },
            command: v.get("command").and_then(Value::as_bool).unwrap_or(false),
        },
        "text" => {
            let text = v.get("text").and_then(Value::as_str).ok_or("'text' must be a string")?;
            if text.chars().count() > 2000 {
                return Err("Type at most 2000 characters at a time".into());
            }
            Input::Text(text.into())
        }
        "keyframe" => Input::KeyFrame,
        "refresh" => Input::Refresh,
        _ => return Err("'t' must be touch, tap, swipe, long_press, gesture, button, key, text, keyframe or refresh".into()),
    })
}

/// A finger moving: when moves pile up, only the newest needs sending.
pub fn is_move(i: &Input) -> bool {
    matches!(i, Input::Touch { phase: Phase::Move, .. })
}

/// HID keyboard usage and whether Shift is needed, for one character (US layout).
pub fn char_usage(c: char) -> Option<(u64, bool)> {
    const SHIFTED: &str = "!@#$%^&*()";
    Some(match c {
        'a'..='z' => (0x04 + (c as u64 - 'a' as u64), false),
        'A'..='Z' => (0x04 + (c as u64 - 'A' as u64), true),
        '1'..='9' => (0x1E + (c as u64 - '1' as u64), false),
        '0' => (0x27, false),
        '\n' => (0x28, false),
        '\t' => (0x2B, false),
        ' ' => (0x2C, false),
        _ if SHIFTED.contains(c) => (0x1E + SHIFTED.find(c)? as u64, true),
        _ => {
            const KEYS: [(char, char, u64); 11] = [
                ('-', '_', 0x2D), ('=', '+', 0x2E), ('[', '{', 0x2F), (']', '}', 0x30), ('\\', '|', 0x31),
                (';', ':', 0x33), ('\'', '"', 0x34), ('`', '~', 0x35), (',', '<', 0x36), ('.', '>', 0x37), ('/', '?', 0x38),
            ];
            let &(plain, _, u) = KEYS.iter().find(|k| k.0 == c || k.1 == c)?;
            (u, c != plain)
        }
    })
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Status {
    /// off, starting, on or error
    pub state: &'static str,
    pub error: Option<String>,
    pub simulated: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Whether the phone's sound is being sent.
    pub audio: bool,
    /// Why there is no sound, or no touch control, when there isn't.
    pub note: Option<String>,
    /// Clockwise degrees (0, 90, 180, 270) that turn the picture upright: the phone is turned.
    pub rotation: u16,
}

impl Default for Status {
    fn default() -> Self {
        Self { state: "off", error: None, simulated: false, width: None, height: None, audio: false, note: None, rotation: 0 }
    }
}

#[derive(Clone, Debug)]
pub enum Out {
    Bin(Arc<Vec<u8>>),
    Text(Arc<String>),
}

enum Cmd {
    Input(Input, oneshot::Sender<Result<(), String>>),
    Screenshot(oneshot::Sender<Result<Vec<u8>, String>>),
    /// Start measuring the phone's CPU, memory, GPU and battery (on the first request for them).
    StartPerf,
}

/// The phone's performance figures, all null until measured: `cpu` (% of all cores), `cores`,
/// `memory_used`/`memory_total` (bytes), `gpu` (%), `fps` (the phone's own frame rate),
/// `battery_temp` (°C), `battery` (%), `charging`.
pub fn empty_perf() -> serde_json::Map<String, Value> {
    ["cpu", "cores", "memory_used", "memory_total", "gpu", "fps", "battery_temp", "battery", "charging"]
        .into_iter()
        .map(|k| (k.to_string(), Value::Null))
        .collect()
}

#[derive(Default)]
struct Inner {
    status: Status,
    cmd: Option<mpsc::UnboundedSender<Cmd>>,
    /// The last decoder setup message, for viewers that join mid-stream.
    config: Option<Arc<Vec<u8>>>,
    /// Which session the fields belong to, so a stopped session can't overwrite a new one.
    session: u64,
    /// Simulator only: the inputs received, for tests and the API's dry runs.
    pub sim_inputs: Vec<Input>,
    perf: serde_json::Map<String, Value>,
    perf_started: bool,
}

pub struct Screen {
    inner: Mutex<Inner>,
    out: broadcast::Sender<Out>,
}

impl Default for Screen {
    fn default() -> Self {
        Self { inner: Mutex::default(), out: broadcast::channel(512).0 }
    }
}

impl Screen {
    pub fn status(&self) -> Status {
        self.inner.lock().unwrap().status.clone()
    }

    pub fn subscribe(&self) -> (broadcast::Receiver<Out>, Option<Arc<Vec<u8>>>, Status) {
        let rx = self.out.subscribe();
        let g = self.inner.lock().unwrap();
        (rx, g.config.clone(), g.status.clone())
    }

    fn set(&self, session: u64, f: impl FnOnce(&mut Status)) {
        let mut g = self.inner.lock().unwrap();
        if g.session != session {
            return;
        }
        f(&mut g.status);
        let text = serde_json::to_string(&json!({"type": "status", "status": g.status})).unwrap();
        let _ = self.out.send(Out::Text(Arc::new(text)));
    }

    fn send_config(&self, session: u64, msg: Vec<u8>) {
        let msg = Arc::new(msg);
        let mut g = self.inner.lock().unwrap();
        if g.session == session {
            g.config = Some(msg.clone());
            let _ = self.out.send(Out::Bin(msg));
        }
    }

    fn send(&self, msg: Vec<u8>) {
        let _ = self.out.send(Out::Bin(Arc::new(msg)));
    }

    /// Start mirroring (stopping any earlier session) and wait until the stream runs.
    pub async fn start(self: &Arc<Self>, simulated: bool, udid: Option<String>, audio: bool) -> Result<Status, String> {
        let (tx, rx) = mpsc::unbounded_channel();
        let session = {
            let mut g = self.inner.lock().unwrap();
            g.session += 1;
            g.cmd = Some(tx);
            g.config = None;
            g.sim_inputs.clear();
            (g.perf, g.perf_started) = (empty_perf(), false);
            g.status = Status { state: "starting", simulated, ..Default::default() };
            g.session
        };
        self.set(session, |_| {});
        if simulated {
            tokio::spawn(simulator(self.clone(), session, rx));
            self.set(session, |s| {
                *s = Status { state: "on", simulated: true, width: Some(1170), height: Some(2532), ..Default::default() };
                s.note = Some("Simulator: there is no real screen; input is accepted and ignored.".into());
            });
            return Ok(self.status());
        }
        let (ready_tx, ready_rx) = oneshot::channel();
        let me = self.clone();
        std::thread::Builder::new()
            .name("iphone-mirror".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread().enable_all().build();
                match rt {
                    // A LocalSet lets the session run its (non-Send) input on a task of its own.
                    Ok(rt) => tokio::task::LocalSet::new().block_on(&rt, session::run(me, session, udid, audio, rx, ready_tx)),
                    Err(e) => me.ended(session, Some(format!("Could not start the mirroring thread: {e}"))),
                }
            })
            .map_err(|e| e.to_string())?;
        match tokio::time::timeout(Duration::from_secs(60), ready_rx).await {
            Ok(Ok(Ok(()))) => Ok(self.status()),
            Ok(Ok(Err(e))) => Err(e),
            _ => {
                self.stop();
                Err("Timed out starting screen mirroring. Unlock the iPhone and try again.".into())
            }
        }
    }

    pub fn stop(&self) {
        let session = {
            let mut g = self.inner.lock().unwrap();
            g.cmd = None; // the session sees the closed channel and ends
            g.config = None;
            g.session
        };
        self.set(session, |s| *s = Status::default());
    }

    fn ended(&self, session: u64, error: Option<String>) {
        self.set(session, |s| {
            *s = Status { state: if error.is_some() { "error" } else { "off" }, error, ..Default::default() };
        });
        let mut g = self.inner.lock().unwrap();
        if g.session == session {
            g.cmd = None;
            g.config = None;
        }
    }

    fn command(&self) -> Result<mpsc::UnboundedSender<Cmd>, String> {
        self.inner.lock().unwrap().cmd.clone().ok_or_else(|| "Screen mirroring is not running. Start it first.".into())
    }

    pub async fn input(&self, input: Input) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        self.command()?.send(Cmd::Input(input, tx)).map_err(|_| "Screen mirroring stopped.".to_string())?;
        tokio::time::timeout(Duration::from_secs(30), rx)
            .await
            .map_err(|_| "The iPhone did not answer.".to_string())?
            .map_err(|_| "Screen mirroring stopped.".to_string())?
    }

    pub async fn screenshot(&self) -> Result<Vec<u8>, String> {
        let (tx, rx) = oneshot::channel();
        self.command()?.send(Cmd::Screenshot(tx)).map_err(|_| "Screen mirroring stopped.".to_string())?;
        tokio::time::timeout(Duration::from_secs(30), rx)
            .await
            .map_err(|_| "The iPhone did not answer.".to_string())?
            .map_err(|_| "Screen mirroring stopped.".to_string())?
    }

    /// The phone's latest performance figures; the first call starts measuring them.
    pub fn perf(&self) -> Result<Value, String> {
        let mut g = self.inner.lock().unwrap();
        if g.status.state != "on" {
            return Err("Screen mirroring is not running. Start it first.".into());
        }
        if g.perf.is_empty() {
            g.perf = empty_perf();
        }
        if !g.perf_started && !g.status.simulated {
            if let Some(c) = &g.cmd {
                g.perf_started = c.send(Cmd::StartPerf).is_ok();
            }
        }
        Ok(Value::Object(g.perf.clone()))
    }

    fn update_perf(&self, session: u64, f: impl FnOnce(&mut serde_json::Map<String, Value>)) {
        let mut g = self.inner.lock().unwrap();
        if g.session == session {
            f(&mut g.perf);
        }
    }

    /// A restarted stream measures again from scratch.
    fn reset_perf(&self, session: u64) {
        let mut g = self.inner.lock().unwrap();
        if g.session == session {
            (g.perf, g.perf_started) = (empty_perf(), false);
        }
    }

    #[cfg(test)]
    pub fn sim_inputs(&self) -> Vec<Input> {
        self.inner.lock().unwrap().sim_inputs.clone()
    }
}

async fn simulator(m: Arc<Screen>, session: u64, mut rx: mpsc::UnboundedReceiver<Cmd>) {
    while let Some(cmd) = rx.recv().await {
        match cmd {
            Cmd::Input(i, r) => {
                let mut g = m.inner.lock().unwrap();
                if g.session == session && g.sim_inputs.len() < 1000 {
                    g.sim_inputs.push(i);
                }
                let _ = r.send(Ok(()));
            }
            Cmd::Screenshot(r) => {
                let _ = r.send(Err("The simulator has no screen to capture.".into()));
            }
            Cmd::StartPerf => {}
        }
    }
    m.ended(session, None);
}

/// Decoder setup message for viewers.
pub fn config_message(c: &hevc::DecoderConfig) -> Vec<u8> {
    let mut m = vec![1];
    m.extend_from_slice(&(c.width.min(65535) as u16).to_be_bytes());
    m.extend_from_slice(&(c.height.min(65535) as u16).to_be_bytes());
    m.push(c.codec.len() as u8);
    m.extend_from_slice(c.codec.as_bytes());
    m.extend_from_slice(&c.hvcc);
    m
}

pub fn frame_message(au: &hevc::AccessUnit, ts_us: u64) -> Vec<u8> {
    let mut m = vec![2, au.key as u8];
    m.extend_from_slice(&ts_us.to_be_bytes());
    m.extend_from_slice(&au.length_prefixed());
    m
}

pub fn audio_message(pcm: &[i16], ts_us: u64) -> Vec<u8> {
    let mut m = Vec::with_capacity(9 + pcm.len() * 2);
    m.push(3);
    m.extend_from_slice(&ts_us.to_be_bytes());
    for s in pcm {
        m.extend_from_slice(&s.to_le_bytes());
    }
    m
}

/// AAC-ELD (48 kHz stereo, 480-sample frames, as the phone sends it) to PCM.
pub mod audio {
    /// AudioSpecificConfig: AAC-ELD, 48 kHz, stereo, frameLengthFlag = 1 (480 samples).
    #[cfg(not(all(windows, target_env = "gnu")))]
    pub const ASC: [u8; 4] = [0xF8, 0xE6, 0x50, 0x00];

    #[cfg(not(all(windows, target_env = "gnu")))]
    pub struct Decoder {
        dec: fdk_aac::dec::Decoder,
        pcm: Vec<i16>,
    }

    #[cfg(not(all(windows, target_env = "gnu")))]
    impl Decoder {
        pub fn new() -> Option<Self> {
            let mut dec = fdk_aac::dec::Decoder::new(fdk_aac::dec::Transport::Raw);
            dec.config_raw(&ASC).ok()?;
            Some(Self { dec, pcm: vec![0; 4096] })
        }
        /// Decode one access unit (one RTP payload) to interleaved stereo samples.
        pub fn decode(&mut self, au: &[u8]) -> Option<Vec<i16>> {
            self.dec.fill(au).ok()?;
            self.dec.decode_frame(&mut self.pcm).ok()?;
            Some(self.pcm[..self.dec.decoded_frame_size()].to_vec())
        }
    }

    #[cfg(all(windows, target_env = "gnu"))]
    pub struct Decoder;

    #[cfg(all(windows, target_env = "gnu"))]
    impl Decoder {
        pub fn new() -> Option<Self> {
            None
        }
        pub fn decode(&mut self, _au: &[u8]) -> Option<Vec<i16>> {
            None
        }
    }
}

mod session;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_inputs() {
        assert_eq!(
            parse_input(&json!({"t": "touch", "phase": "down", "x": 0.5, "y": 1.0})).unwrap(),
            Input::Touch { phase: Phase::Down, x: 32768, y: 65535, second: None }
        );
        assert_eq!(
            parse_input(&json!({"t": "touch", "phase": "move", "x": 0, "y": 0, "x2": 1, "y2": 0.5})).unwrap(),
            Input::Touch { phase: Phase::Move, x: 0, y: 0, second: Some((65535, 32768)) }
        );
        assert_eq!(parse_input(&json!({"t": "gesture", "name": "back"})).unwrap(), Input::Gesture("back"));
        assert_eq!(parse_input(&json!({"t": "long_press", "x": 0, "y": 0})).unwrap(), Input::LongPress { x: 0, y: 0, ms: 800 });
        assert_eq!(
            parse_input(&json!({"t": "key", "usage": 4, "command": true})).unwrap(),
            Input::Key { usage: 4, down: None, command: true }
        );
        assert_eq!(parse_input(&json!({"t": "button", "name": "siri"})).unwrap(), Input::Button("siri", 0x0C, 0xCF, 1000));
        assert_eq!(parse_input(&json!({"t": "key", "usage": 40, "down": false})).unwrap(), Input::Key { usage: 40, down: Some(false), command: false });
        assert_eq!(parse_input(&json!({"t": "refresh"})).unwrap(), Input::Refresh);
        assert!(matches!(parse_input(&json!({"t": "swipe", "x1": 0, "y1": 0, "x2": 1, "y2": 1})).unwrap(), Input::Swipe { ms: 300, .. }));
        for bad in [
            json!({"t": "tap", "x": 1.5, "y": 0}),
            json!({"t": "tap", "x": "0.2", "y": 0}),
            json!({"t": "touch", "phase": "hover", "x": 0, "y": 0}),
            json!({"t": "button", "name": "power"}),
            json!({"t": "key", "usage": 1, "down": true}),
            json!({"t": "key", "usage": 4, "down": "yes"}),
            json!({"t": "touch", "phase": "down", "x": 0, "y": 0, "x2": 0.5}),
            json!({"t": "gesture", "name": "spin"}),
            json!({"t": "fly"}),
        ] {
            assert!(parse_input(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn characters_map_to_us_keys() {
        assert_eq!(char_usage('a'), Some((0x04, false)));
        assert_eq!(char_usage('Z'), Some((0x1D, true)));
        assert_eq!(char_usage('0'), Some((0x27, false)));
        assert_eq!(char_usage('('), Some((0x26, true)));
        assert_eq!(char_usage('?'), Some((0x38, true)));
        assert_eq!(char_usage('/'), Some((0x38, false)));
        assert_eq!(char_usage('é'), None);
    }

    #[test]
    fn messages_have_the_documented_layout() {
        let c = hevc::DecoderConfig { codec: "hev1.1.6.L180.90".into(), hvcc: vec![1, 2], width: 1170, height: 2532 };
        let m = config_message(&c);
        assert_eq!(&m[..6], &[1, 0x04, 0x92, 0x09, 0xE4, 16]);
        assert_eq!(&m[6..22], b"hev1.1.6.L180.90");
        assert_eq!(&m[22..], &[1, 2]);
        let au = hevc::AccessUnit { rtp_ts: 0, key: true, nals: vec![vec![0x26, 1, 7]] };
        assert_eq!(frame_message(&au, 5), [vec![2, 1], 5u64.to_be_bytes().to_vec(), vec![0, 0, 0, 3, 0x26, 1, 7]].concat());
        assert_eq!(audio_message(&[1, -1], 2), [vec![3], 2u64.to_be_bytes().to_vec(), vec![1, 0, 0xff, 0xff]].concat());
    }

    #[tokio::test]
    async fn simulated_session_takes_input_and_stops() {
        let m = Arc::new(Screen::default());
        assert!(m.input(Input::KeyFrame).await.is_err());
        let s = m.start(true, None, true).await.unwrap();
        assert_eq!((s.state, s.simulated, s.width), ("on", true, Some(1170)));
        m.input(Input::Tap { x: 1, y: 2 }).await.unwrap();
        assert_eq!(m.sim_inputs(), vec![Input::Tap { x: 1, y: 2 }]);
        assert!(m.screenshot().await.is_err());
        m.stop();
        assert_eq!(m.status().state, "off");
        assert!(m.input(Input::KeyFrame).await.is_err());
    }

    #[cfg(not(all(windows, target_env = "gnu")))]
    #[test]
    fn audio_decoder_accepts_the_phone_setup_and_survives_garbage() {
        let mut d = audio::Decoder::new().expect("FDK accepts the 480-sample AAC-ELD setup");
        assert!(d.decode(&[0xff; 7]).is_none());
        assert!(d.decode(&[]).is_none());
    }
}
