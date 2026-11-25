//! One mirroring session with a real iPhone: tunnel, display stream, HID input, RTCP feedback.
//! Protocol details follow the idevice crate's `displayservice` support (MIT) and
//! pymobiledevice3's notes on what the phone expects.

use super::{audio, config_message, frame_message, audio_message, hevc, Cmd, Input, Phase, Screen};
use crate::device::idev;
use idevice::{
    core_device::{
        build_keyframe_request, build_liveness, build_screen_audio_offer, build_screen_video_offer, build_start_audio_parameters,
        build_start_video_parameters, is_rtcp, ButtonState, CallInfoBlob, CoreDeviceServiceClient, DisplayServiceClient, IndigoHidClient,
        ReportBlock, RtpPacket, TouchscreenContact, UniversalHidServiceClient, TOUCHSCREEN_STATE_CONTACT, TOUCHSCREEN_STATE_RELEASE,
    },
    core_device_proxy::CoreDeviceProxy,
    diagnostics_relay::DiagnosticsRelayClient,
    dvt::{
        device_info::DeviceInfoClient,
        message::AuxValue,
        remote_server::RemoteServerClient,
        screenshot::ScreenshotClient,
        sysmontap::{SysmontapClient, SysmontapConfig},
    },
    lockdown::LockdownClient,
    provider::{IdeviceProvider, RsdProvider, UsbmuxdProvider},
    rsd::RsdHandshake,
    IdeviceError, IdeviceService, ReadWrite, RsdService,
};
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

/// What Device Hub announces as its own features; the phone's own mask breaks negotiation.
const CLIENT_FEATURES: u64 = 140;
/// Error code CoreDevice returns when Camera or a recording app holds the camera or microphone.
const MEDIA_IN_USE: &str = "9022";

type Hid = UniversalHidServiceClient<Box<dyn ReadWrite>>;
type Indigo = IndigoHidClient<Box<dyn ReadWrite>>;
type Rs = RemoteServerClient<Box<dyn ReadWrite>>;

/// Unanswered keyframe requests (while the picture is broken) before the stream is restarted;
/// iOS 27's encoder sometimes stops answering them, and a fresh stream always opens with one.
const MAX_KEY_REQUESTS: u32 = 3;
/// No datagram at all (the phone's RTCP reports come every second) means the stream died.
const SILENCE: Duration = Duration::from_secs(6);
/// Automatic restarts in a row (less than a minute apart) before giving up.
const MAX_RESTARTS: u32 = 3;

/// How a stream ended without an error.
enum Ended {
    Stopped,
    /// Start it again: `auto` when the stream froze, false when the user asked (Refresh).
    Restart { auto: bool },
}

pub(super) async fn run(
    m: Arc<Screen>,
    session: u64,
    udid: Option<String>,
    audio_on: bool,
    rx: mpsc::UnboundedReceiver<Cmd>,
    ready: oneshot::Sender<Result<(), String>>,
) {
    let mut ready = Some(ready);
    let mut rx = rx;
    let (mut restarts, mut last_restart) = (0, Instant::now());
    let result = loop {
        match stream(&m, session, udid.clone(), audio_on, &mut rx, &mut ready).await {
            Ok(Ended::Restart { auto }) => {
                if auto {
                    restarts = if last_restart.elapsed() < Duration::from_secs(60) { restarts + 1 } else { 1 };
                    last_restart = Instant::now();
                    if restarts > MAX_RESTARTS {
                        break Err("The iPhone stopped sending its screen. Unlock it, then start mirroring again.".to_string());
                    }
                }
                m.reset_perf(session);
                m.set(session, |s| s.note = Some("Reconnecting to the iPhone's screen…".into()));
            }
            Ok(Ended::Stopped) => break Ok(()),
            Err(e) => break Err(e),
        }
    };
    if let Some(r) = ready.take() {
        let _ = r.send(Err(result.clone().err().unwrap_or_else(|| "Screen mirroring stopped.".into())));
    }
    m.ended(session, result.err());
}

/// The `streamConfig` values of a start answer: the phone's RTP port and both SSRCs.
fn stream_config(answer: &plist::Value) -> (u16, u32, u32) {
    let cfg = answer.as_dictionary().and_then(|d| d.get("connection")).and_then(|c| c.as_dictionary()).and_then(|c| c.get("streamConfig"));
    let num = |k: &str| cfg.and_then(|c| c.as_dictionary()).and_then(|c| c.get(k)).and_then(|v| v.as_unsigned_integer()).unwrap_or(0);
    // Named from the phone's side: LocalSSRC is the phone's, RemoteSSRC is ours.
    (num("SourcePort") as u16, num("RemoteSSRC") as u32, num("LocalSSRC") as u32)
}

fn start_error(what: &str, e: idevice::IdeviceError) -> String {
    let text = format!("{e:?}");
    if text.contains(MEDIA_IN_USE) {
        return "The iPhone won't share its screen while an app uses the camera or microphone (for example Camera or Voice Memos). Close that app and try again.".into();
    }
    format!("Could not start the {what} stream: {}", idev::friendly(&e))
}

/// Extended highest RTP sequence number (RFC 3550): wrap count in the high 16 bits.
#[derive(Default)]
struct SeqCounter(Option<u32>);

impl SeqCounter {
    fn add(&mut self, seq: u16) {
        self.0 = Some(match self.0 {
            None => seq as u32,
            // Only newer packets move it forward; a late one from before a wrap is ignored.
            Some(cur) => match seq.wrapping_sub(cur as u16) as i16 {
                d if d > 0 => cur.wrapping_add(d as u32),
                _ => cur,
            },
        });
    }
}

async fn stream(
    m: &Arc<Screen>,
    session: u64,
    udid: Option<String>,
    audio_on: bool,
    rx: &mut mpsc::UnboundedReceiver<Cmd>,
    ready: &mut Option<oneshot::Sender<Result<(), String>>>,
) -> Result<Ended, String> {
    let fr = |e: idevice::IdeviceError| idev::friendly(&e);
    let (provider, _) = idev::find_device(udid.as_deref(), crate::device::Via::Auto).await?;
    idev::ensure_paired(&provider).await?;
    let pairing = provider.get_pairing_file().await.map_err(fr)?;
    let mut ld = LockdownClient::connect(&provider).await.map_err(fr)?;
    ld.start_session(&pairing).await.map_err(fr)?;
    let version = ld.get_value(Some("ProductVersion"), None).await.ok().and_then(|v| v.as_string().map(String::from)).unwrap_or_default();
    let chip = ld.get_value(Some("UniqueChipID"), None).await.ok().and_then(|v| v.as_unsigned_integer());
    let name = ld.get_value(Some("DeviceName"), None).await.ok().and_then(|v| v.as_string().map(String::from));
    drop(ld);
    let ios = Some(version.clone()).filter(|v| !v.is_empty());
    m.set(session, |s| (s.device_name, s.ios) = (name, ios));
    if idev::major(&version) < 27 {
        return Err(format!("Screen mirroring needs iOS 27 or newer. This iPhone has iOS {version}."));
    }
    // A failed mount is reported only if the display service then turns out to be missing.
    let ddi = idev::ensure_ddi(&provider, &version, chip).await.err();

    let proxy = CoreDeviceProxy::connect(&provider).await.map_err(fr)?;
    let rsd_port = proxy.tunnel_info().server_rsd_port;
    let mut adapter = proxy.create_software_tunnel().map_err(fr)?.to_async_handle();
    let rsd = adapter.connect(rsd_port).await.map_err(|e| format!("Could not open the developer tunnel: {e}"))?;
    let mut hs = RsdHandshake::new(rsd).await.map_err(fr)?;
    let mut display = DisplayServiceClient::connect_rsd(&mut adapter, &mut hs).await.map_err(|e| match &ddi {
        Some(d) => format!("The iPhone's screen service is not available ({}). {d}", fr(e)),
        None => format!("The iPhone's screen service is not available: {}. Check that Developer Mode is on.", fr(e)),
    })?;

    let audio_udp = adapter.bind_udp(0).await.map_err(|e| e.to_string())?;
    let video_udp = adapter.bind_udp(0).await.map_err(|e| e.to_string())?;
    let (host, peer) = (adapter.host_ip().to_string(), adapter.peer_ip().to_string());
    // Values from a captured Device Hub offer that the phone accepts.
    let call_info = CallInfoBlob {
        call_id: 0,
        client_version: 1,
        device_type: "Mac17,7".into(),
        framework_version: "2205.3.1".into(),
        os_version: "25F71".into(),
        device_name: None,
        audio_device_uid: None,
    };
    let call_id = || uuid::Uuid::from_bytes(rand::random()).hyphenated().to_string().to_uppercase();
    let session_id = uuid::Uuid::from_bytes(rand::random());
    // The audio stream goes first: it sets up the screen-sharing session the video joins.
    let offer = build_screen_audio_offer(&call_id(), &call_info).map_err(fr)?;
    let a_params = build_start_audio_parameters(&host, audio_udp.local_port(), &peer, 50000, offer, CLIENT_FEATURES, session_id);
    let (a_port, a_ours, a_phone) = stream_config(&display.start_media_stream(a_params).await.map_err(|e| start_error("sound", e))?);
    let our_ssrc: u32 = rand::random();
    let offer = build_screen_video_offer(&call_id(), &call_info, our_ssrc).map_err(fr)?;
    // shortcut: display 1 is the built-in screen; foldables with two panels would need the active one.
    let v_params = build_start_video_parameters(&host, video_udp.local_port(), &peer, 50001, offer, CLIENT_FEATURES, 1, session_id);
    let (v_port, v_ours, v_phone) = stream_config(&display.start_media_stream(v_params).await.map_err(|e| start_error("screen", e))?);
    let v_ours = if v_ours != 0 { v_ours } else { our_ssrc };

    // Touch and buttons are refused until the phone has matched them to the new stream.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let hid: Option<Hid> = UniversalHidServiceClient::connect_rsd(&mut adapter, &mut hs).await.ok();
    let indigo: Option<Indigo> = IndigoHidClient::connect_rsd(&mut adapter, &mut hs).await.ok();
    // Which way the screen is turned, asked on its own task so a slow answer never holds up the
    // picture or input; without it the picture stays upright and the Rotate button helps.
    let info = match hs.services.get("com.apple.coredevice.deviceinfo").map(|s| s.port) {
        Some(port) => match adapter.connect_to_service_port(port).await {
            Ok(stream) => CoreDeviceServiceClient::new(stream).await.ok(),
            Err(_) => None,
        },
        None => None,
    };
    let mut tasks = Vec::new();
    if let Some(info) = info {
        tasks.push(tokio::task::spawn_local(follow_rotation(m.clone(), session, info)));
    }
    let mut decoder = if audio_on { audio::Decoder::new() } else { None };
    let mut note = Vec::new();
    if hid.is_none() || indigo.is_none() {
        note.push("The iPhone did not accept touch input, so it can only be watched.");
    }
    if audio_on && decoder.is_none() {
        note.push("This build of the app cannot play the iPhone's sound.");
    }
    m.set(session, |s| {
        s.state = "on";
        s.audio = decoder.is_some();
        s.note = (!note.is_empty()).then(|| note.join(" "));
    });
    if let Some(r) = ready.take() {
        let _ = r.send(Ok(()));
    }
    // Input runs on its own task, in order: a held button (Siri: 1 s), a swipe or typing must
    // not stop the picture from being received meanwhile.
    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<(Input, oneshot::Sender<Result<(), String>>)>();
    let mi = m.clone();
    tasks.push(tokio::task::spawn_local(async move {
        let (mut hid, mut indigo) = (hid, indigo);
        while let Some((i, r)) = input_rx.recv().await {
            let rotation = mi.status().rotation;
            let _ = r.send(apply(i, hid.as_mut(), indigo.as_mut(), rotation).await);
        }
    }));

    let started = Instant::now();
    let mut depack = hevc::Depacketizer::new();
    let (mut v_seq, mut a_seq) = (SeqCounter::default(), SeqCounter::default());
    let mut last_keyframe_request = Instant::now() - Duration::from_secs(5);
    let mut fir_seq = 0u8;
    // `want_key`: someone needs a keyframe (a viewer, or lost packets); `refresh`: a still screen
    // gets one now and then for new viewers. Only the first kind counts towards a restart.
    let (mut want_key, mut refresh, mut key_requests) = (true, false, 0u32);
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    let (mut last_video, mut last_dgram) = (Instant::now(), Instant::now());

    let result = loop {
        let broken = want_key || depack.waiting_for_key;
        if (broken || refresh) && last_keyframe_request.elapsed() > Duration::from_secs(1) && v_phone != 0 {
            if broken {
                if key_requests >= MAX_KEY_REQUESTS {
                    break Ok(Ended::Restart { auto: true });
                }
                key_requests += 1;
            }
            let block = ReportBlock { source_ssrc: v_phone, highest_seq: v_seq.0.unwrap_or(0), ..Default::default() };
            let _ = video_udp.send_to(v_port, build_keyframe_request(v_ours, "", v_phone, &[block], fir_seq)).await;
            fir_seq = fir_seq.wrapping_add(1);
            last_keyframe_request = Instant::now();
            (want_key, refresh) = (false, false);
        }
        if last_dgram.elapsed() > SILENCE {
            break Ok(Ended::Restart { auto: true });
        }
        tokio::select! {
            dg = video_udp.recv() => {
                let Ok(dg) = dg else { break Err("The connection to the iPhone was lost.".to_string()) };
                last_dgram = Instant::now();
                if is_rtcp(&dg.data) { continue; }
                let Some(p) = RtpPacket::parse(&dg.data) else { continue };
                last_video = Instant::now();
                v_seq.add(p.sequence_number);
                let frames = depack.push(p.sequence_number, p.timestamp, p.marker, p.payload);
                for au in frames {
                    if au.key {
                        key_requests = 0;
                    }
                    if depack.config_changed {
                        if let Some(c) = depack.decoder_config() {
                            m.set(session, |s| { s.width = Some(c.width); s.height = Some(c.height); });
                            m.send_config(session, config_message(&c));
                        }
                        depack.config_changed = false;
                    }
                    m.send(frame_message(&au, started.elapsed().as_micros() as u64));
                    let _ = video_udp.send_to(v_port, idevice::core_device::build_frame_ack(v_ours, au.rtp_ts)).await;
                }
            }
            dg = audio_udp.recv() => {
                let Ok(dg) = dg else { break Err("The connection to the iPhone was lost.".to_string()) };
                if is_rtcp(&dg.data) { continue; }
                let Some(p) = RtpPacket::parse(&dg.data) else { continue };
                a_seq.add(p.sequence_number);
                if let Some(pcm) = decoder.as_mut().and_then(|d| d.decode(p.payload)) {
                    m.send(audio_message(&pcm, started.elapsed().as_micros() as u64));
                }
            }
            _ = tick.tick() => {
                // Receiver reports keep both streams alive; without them the phone stops after ~20 s.
                if v_phone != 0 {
                    let b = ReportBlock { source_ssrc: v_phone, highest_seq: v_seq.0.unwrap_or(0), ..Default::default() };
                    let _ = video_udp.send_to(v_port, build_liveness(v_ours, "", &[b])).await;
                }
                if a_phone != 0 && a_port != 0 {
                    let b = ReportBlock { source_ssrc: a_phone, highest_seq: a_seq.0.unwrap_or(0), ..Default::default() };
                    let _ = audio_udp.send_to(a_port, build_liveness(a_ours, "", &[b])).await;
                }
                if last_video.elapsed() > Duration::from_secs(4) {
                    refresh = true; // a still screen sends nothing; ask again so new viewers get a picture
                    last_video = Instant::now();
                }
            }
            cmd = rx.recv() => match cmd {
                None => break Ok(Ended::Stopped),
                Some(Cmd::Input(Input::KeyFrame, r)) => { want_key = true; let _ = r.send(Ok(())); }
                Some(Cmd::Input(Input::Refresh, r)) => { let _ = r.send(Ok(())); break Ok(Ended::Restart { auto: false }); }
                Some(Cmd::Input(i, r)) => {
                    let _ = input_tx.send((i, r));
                }
                Some(Cmd::Screenshot(r)) => {
                    let _ = r.send(screenshot(&mut adapter, &mut hs).await);
                }
                Some(Cmd::StartPerf) => tasks.extend(start_perf(m, session, &mut adapter, &mut hs, &provider).await),
            }
        }
    };
    tasks.iter().for_each(|t| t.abort());
    let _ = display.stop_media_stream().await;
    result
}

async fn press(indigo: &mut Indigo, page: u64, usage: u64, hold: u64) -> Result<(), idevice::IdeviceError> {
    indigo.send_button(page, usage, ButtonState::Down).await?;
    tokio::time::sleep(Duration::from_millis(hold)).await;
    indigo.send_button(page, usage, ButtonState::Up).await
}

async fn key(indigo: &mut Indigo, usage: u64, shift: bool) -> Result<(), idevice::IdeviceError> {
    const SHIFT: u64 = 0xE1;
    if shift {
        indigo.send_keyboard(SHIFT, ButtonState::Down).await?;
    }
    indigo.send_keyboard(usage, ButtonState::Down).await?;
    indigo.send_keyboard(usage, ButtonState::Up).await?;
    if shift {
        indigo.send_keyboard(SHIFT, ButtonState::Up).await?;
    }
    // Keystrokes sent back to back get merged.
    tokio::time::sleep(Duration::from_millis(12)).await;
    Ok(())
}

/// A point on the upright picture (fractions) -> the phone's native touch coordinates.
fn upright_to_native(u: f64, v: f64, rotation: u16) -> (u16, u16) {
    let (x, y) = match rotation {
        90 => (v, 1.0 - u),
        180 => (1.0 - u, 1.0 - v),
        270 => (1.0 - v, u),
        _ => (u, v),
    };
    ((x.clamp(0.0, 1.0) * 65535.0).round() as u16, (y.clamp(0.0, 1.0) * 65535.0).round() as u16)
}

async fn gesture(hid: &mut Hid, name: &str, rotation: u16) -> Result<(), IdeviceError> {
    let Some(&(_, (u1, v1), (u2, v2), ms, hold)) = super::GESTURES.iter().find(|g| g.0 == name) else { return Ok(()) };
    let steps = (ms / 16).max(2);
    let at = |f: f64| upright_to_native(u1 + (u2 - u1) * f, v1 + (v2 - v1) * f, rotation);
    for n in 0..=steps {
        let (x, y) = at(n as f64 / steps as f64);
        hid.send_touchscreen(TOUCHSCREEN_STATE_CONTACT, x, y, None).await?;
        tokio::time::sleep(Duration::from_millis(16)).await;
    }
    tokio::time::sleep(Duration::from_millis(hold)).await;
    let (x, y) = at(1.0);
    hid.send_touchscreen(TOUCHSCREEN_STATE_RELEASE, x, y, None).await
}

async fn apply(i: Input, hid: Option<&mut Hid>, indigo: Option<&mut Indigo>, rotation: u16) -> Result<(), String> {
    const NO_CONTROL: &str = "The iPhone did not accept touch input in this session. Stop and start mirroring again.";
    const COMMAND: u64 = 0xE3;
    let r = match i {
        Input::Touch { phase, x, y, second: None } => {
            let state = if phase == Phase::Up { TOUCHSCREEN_STATE_RELEASE } else { TOUCHSCREEN_STATE_CONTACT };
            hid.ok_or(NO_CONTROL)?.send_touchscreen(state, x, y, None).await
        }
        // Two fingers (pinch): both in every report, with identities that stay the same.
        Input::Touch { phase, x, y, second: Some((x2, y2)) } => {
            let touching = phase != Phase::Up;
            let contacts = [TouchscreenContact { identity: 2, touching, x, y }, TouchscreenContact { identity: 3, touching, x: x2, y: y2 }];
            hid.ok_or(NO_CONTROL)?.send_multitouch(&contacts, None).await
        }
        Input::LongPress { x, y, ms } => {
            let hid = hid.ok_or(NO_CONTROL)?;
            hid.send_touchscreen(TOUCHSCREEN_STATE_CONTACT, x, y, None).await.map_err(|e| idev::friendly(&e))?;
            tokio::time::sleep(Duration::from_millis(ms)).await;
            hid.send_touchscreen(TOUCHSCREEN_STATE_RELEASE, x, y, None).await
        }
        Input::Gesture(name) => gesture(hid.ok_or(NO_CONTROL)?, name, rotation).await,
        Input::Tap { x, y } => hid.ok_or(NO_CONTROL)?.tap(x, y).await,
        // ~60 samples a second, so the phone sees a drag with a speed rather than a tap.
        Input::Swipe { x1, y1, x2, y2, ms } => hid.ok_or(NO_CONTROL)?.drag(x1, y1, x2, y2, (ms / 16).max(2) as u32, 16).await,
        Input::Button(_, page, usage, hold) => press(indigo.ok_or(NO_CONTROL)?, page, usage, hold).await,
        Input::Key { usage, down: Some(down), command: false } => {
            indigo.ok_or(NO_CONTROL)?.send_keyboard(usage, if down { ButtonState::Down } else { ButtonState::Up }).await
        }
        Input::Key { usage, down, command } => {
            let indigo = indigo.ok_or(NO_CONTROL)?;
            let keys: &[u64] = if command { &[COMMAND, usage] } else { &[usage] };
            let mut r = Ok(());
            if down != Some(false) {
                for &k in keys {
                    r = r.and(indigo.send_keyboard(k, ButtonState::Down).await);
                }
            }
            if down != Some(true) {
                for &k in keys.iter().rev() {
                    r = r.and(indigo.send_keyboard(k, ButtonState::Up).await);
                }
            }
            r
        }
        Input::Text(text) => {
            let indigo = indigo.ok_or(NO_CONTROL)?;
            for c in text.chars() {
                if let Some((usage, shift)) = super::char_usage(c) {
                    key(indigo, usage, shift).await.map_err(|e| idev::friendly(&e))?;
                }
            }
            Ok(())
        }
        Input::KeyFrame | Input::Refresh => Ok(()),
    };
    r.map_err(|e| idev::friendly(&e))
}

/// Asks which way the screen is turned twice a second, until the phone stops answering.
async fn follow_rotation(m: Arc<Screen>, session: u64, mut info: CoreDeviceServiceClient<Box<dyn ReadWrite>>) {
    let mut rotation = 0;
    loop {
        let answer = tokio::time::timeout(Duration::from_secs(3), info.invoke("com.apple.coredevice.feature.getdisplayinfo", None)).await;
        let Ok(Ok(v)) = answer else { return };
        let r = display_rotation(&v);
        if r != rotation {
            rotation = r;
            m.set(session, |s| s.rotation = r);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Starts measuring CPU and memory, GPU, and the battery, each on its own connection and task
/// (a part the phone refuses stays null).
async fn start_perf(
    m: &Arc<Screen>,
    session: u64,
    adapter: &mut idevice::tcp::handle::AdapterHandle,
    hs: &mut RsdHandshake,
    provider: &UsbmuxdProvider,
) -> Vec<tokio::task::JoinHandle<()>> {
    let mut tasks = Vec::new();
    if let Ok(rs) = RemoteServerClient::connect_rsd(adapter, hs).await {
        let m = m.clone();
        tasks.push(tokio::task::spawn_local(async move { let _ = cpu_memory(&m, session, rs).await; }));
    }
    if let Ok(rs) = RemoteServerClient::connect_rsd(adapter, hs).await {
        let m = m.clone();
        tasks.push(tokio::task::spawn_local(async move { let _ = gpu(&m, session, rs).await; }));
    }
    if let Ok(d) = DiagnosticsRelayClient::connect(provider).await {
        tasks.push(tokio::task::spawn_local(battery(m.clone(), session, d)));
    }
    tasks
}

fn num(v: Option<&plist::Value>) -> Option<f64> {
    match v? {
        plist::Value::Integer(i) => i.as_signed().map(|i| i as f64).or_else(|| i.as_unsigned().map(|u| u as f64)),
        plist::Value::Real(f) => Some(*f),
        plist::Value::Boolean(b) => Some(*b as u8 as f64),
        _ => None,
    }
}

/// Memory in use and in total (bytes) from sysmontap's system figures (page counts, 16 KB pages).
fn memory(sys: &std::collections::HashMap<String, plist::Value>) -> (Option<f64>, Option<f64>) {
    const PAGE: f64 = 16384.0;
    let pages = |k: &str| num(sys.get(k));
    let used = match (pages("vmActiveCount"), pages("vmWireCount")) {
        (Some(a), Some(w)) => Some((a + w + pages("vmCompressorPageCount").unwrap_or(0.0)) * PAGE),
        _ => None,
    };
    let total = num(sys.get("physMemSize")).or_else(|| {
        let parts = ["vmFreeCount", "vmActiveCount", "vmInactiveCount", "vmWireCount", "vmCompressorPageCount", "vmSpeculativeCount"];
        let found: Vec<f64> = parts.iter().filter_map(|k| pages(k)).collect();
        (found.len() >= 4).then(|| found.iter().sum::<f64>() * PAGE)
    });
    (used, total)
}

async fn cpu_memory(m: &Arc<Screen>, session: u64, mut rs: Rs) -> Result<(), IdeviceError> {
    rs.read_message(0).await?;
    let (cores, attrs) = {
        let mut info = DeviceInfoClient::new(&mut rs).await?;
        let hw = info.hardware_information().await.ok();
        (hw.and_then(|h| num(h.get("numberOfCpus"))), info.sysmon_system_attributes().await?)
    };
    m.update_perf(session, |p| p["cores"] = json!(cores));
    let mut tap = SysmontapClient::new(&mut rs).await?;
    tap.set_config(&SysmontapConfig { interval_ms: 1000, process_attributes: Vec::new(), system_attributes: attrs.clone() }).await?;
    tap.start().await?;
    loop {
        let s = tap.next_sample().await?;
        // CPU_TotalLoad adds up all cores (200 = two cores busy).
        let load = s.system_cpu_usage.as_ref().and_then(|d| num(d.get("CPU_TotalLoad")));
        let cpu = load.map(|l| (l / cores.unwrap_or(1.0).max(1.0)).clamp(0.0, 100.0));
        let sys = s.system.map(|v| attrs.iter().cloned().zip(v).collect()).unwrap_or_default();
        let (used, total) = memory(&sys);
        m.update_perf(session, |p| {
            p["cpu"] = json!(cpu);
            p["memory_used"] = json!(used);
            p["memory_total"] = json!(total);
        });
    }
}

async fn gpu(m: &Arc<Screen>, session: u64, mut rs: Rs) -> Result<(), IdeviceError> {
    rs.read_message(0).await?;
    let mut ch = rs.make_channel("com.apple.instruments.server.services.graphics.opengl").await?;
    ch.call_method(Some(plist::Value::String("startSamplingAtTimeInterval:".into())), Some(vec![AuxValue::Double(0.0)]), true).await?;
    ch.read_message().await?;
    loop {
        let Some(d) = ch.read_message().await?.data.and_then(|v| v.into_dictionary()) else { continue };
        let (gpu, fps) = (num(d.get("Device Utilization %")), num(d.get("CoreAnimationFramesPerSecond")));
        if gpu.is_some() || fps.is_some() {
            m.update_perf(session, |p| {
                p["gpu"] = json!(gpu);
                p["fps"] = json!(fps);
            });
        }
    }
}

/// Battery level, charging and temperature every 5 s (newer iOS keeps the temperature on the battery pack).
async fn battery(m: Arc<Screen>, session: u64, mut d: DiagnosticsRelayClient) {
    loop {
        let Ok(Some(info)) = d.ioregistry(None, None, Some("IOPMPowerSource")).await else { return };
        let mut temp = num(info.get("Temperature"));
        if temp.is_none() {
            if let Ok(Some(pack)) = d.ioregistry(None, None, Some("AppleSmartBatteryPack")).await {
                temp = num(pack.get("BatteryData").and_then(|b| b.as_dictionary()).and_then(|b| b.get("Temperature")));
            }
        }
        m.update_perf(session, |p| {
            p["battery_temp"] = json!(temp.map(|t| t / 100.0));
            p["battery"] = json!(num(info.get("CurrentCapacity")));
            p["charging"] = json!(info.get("IsCharging").and_then(|v| v.as_boolean()));
        });
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

/// Clockwise degrees (0, 90, 180, 270) that turn the streamed picture upright, from a
/// `getdisplayinfo` answer: the built-in display in use, else the primary one.
fn display_rotation(info: &plist::Value) -> u16 {
    let displays = info.as_dictionary().and_then(|d| d.get("displays")).and_then(|v| v.as_array()).map(Vec::as_slice).unwrap_or(&[]);
    let get = |d: &plist::Value, k: &str| d.as_dictionary().and_then(|d| d.get(k)).cloned();
    let flag = |d: &plist::Value, k: &str| get(d, k).and_then(|v| v.as_boolean()).unwrap_or(false);
    let built_in: Vec<&plist::Value> = displays.iter().filter(|d| !flag(d, "external")).collect();
    let chosen = built_in
        .iter()
        .find(|d| flag(d, "active"))
        .or_else(|| built_in.iter().find(|d| get(d, "backlightState").and_then(|v| v.as_string().map(String::from)).is_some_and(|b| b != "off")))
        .copied()
        .or_else(|| displays.iter().find(|d| flag(d, "primary")))
        .or_else(|| displays.first());
    match chosen.and_then(|d| get(d, "currentOrientation")).and_then(|v| v.as_string().map(String::from)).as_deref() {
        Some("rot90") => 90,
        Some("rot180") => 180,
        Some("rot270") => 270,
        _ => 0,
    }
}

async fn screenshot(adapter: &mut idevice::tcp::handle::AdapterHandle, hs: &mut RsdHandshake) -> Result<Vec<u8>, String> {
    let fr = |e: idevice::IdeviceError| idev::friendly(&e);
    let mut rs = RemoteServerClient::connect_rsd(adapter, hs).await.map_err(fr)?;
    rs.read_message(0).await.map_err(fr)?;
    let mut shot = ScreenshotClient::new(&mut rs).await.map_err(fr)?;
    shot.take_screenshot().await.map_err(fr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_numbers_extend_across_wraps() {
        let mut s = SeqCounter::default();
        for seq in [65534, 65535, 0, 1, 65535, 2] {
            s.add(seq);
        }
        assert_eq!(s.0, Some(0x1_0002));
    }

    #[test]
    fn upright_points_map_to_the_native_screen() {
        assert_eq!(upright_to_native(0.25, 0.0, 0), (16384, 0));
        assert_eq!(upright_to_native(0.0, 0.25, 90), (16384, 65535));
        assert_eq!(upright_to_native(1.0, 1.0, 180), (0, 0));
        assert_eq!(upright_to_native(0.0, 0.25, 270), (49151, 0));
    }

    #[test]
    fn memory_from_page_counts() {
        let sys: std::collections::HashMap<String, plist::Value> = [("vmActiveCount", 10u64), ("vmWireCount", 5), ("vmFreeCount", 5), ("vmInactiveCount", 4)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), plist::Value::Integer(v.into())))
            .collect();
        assert_eq!(memory(&sys), (Some(15.0 * 16384.0), Some(24.0 * 16384.0)));
        assert_eq!(memory(&Default::default()), (None, None));
    }

    #[test]
    fn reads_the_screen_rotation() {
        let d = |pairs: &[(&str, plist::Value)]| plist::Value::Dictionary(pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect());
        let s = |v: &str| plist::Value::String(v.into());
        let info = d(&[(
            "displays",
            plist::Value::Array(vec![
                d(&[("external", true.into()), ("currentOrientation", s("rot180"))]),
                d(&[("primary", true.into()), ("currentOrientation", s("rot90"))]),
                d(&[("active", true.into()), ("currentOrientation", s("rot270"))]),
            ]),
        )]);
        assert_eq!(display_rotation(&info), 270);
        let single = d(&[("displays", plist::Value::Array(vec![d(&[("currentOrientation", s("rot90"))])]))]);
        assert_eq!(display_rotation(&single), 90);
        assert_eq!(display_rotation(&plist::Value::Boolean(true)), 0);
    }

    #[test]
    fn reads_the_stream_config() {
        let answer: plist::Value = plist::Value::Dictionary(
            [("connection".to_string(), plist::Value::Dictionary(
                [("streamConfig".to_string(), plist::Value::Dictionary(
                    [("SourcePort", 50001u64), ("LocalSSRC", 7), ("RemoteSSRC", 9)]
                        .into_iter()
                        .map(|(k, v)| (k.to_string(), plist::Value::Integer(v.into())))
                        .collect(),
                ))]
                .into_iter()
                .collect(),
            ))]
            .into_iter()
            .collect(),
        );
        assert_eq!(stream_config(&answer), (50001, 9, 7));
        assert_eq!(stream_config(&plist::Value::Boolean(true)), (0, 0, 0));
    }
}
