//! One mirroring session with a real iPhone: tunnel, display stream, HID input, RTCP feedback.
//! Protocol details follow the idevice crate's `displayservice` support (MIT) and
//! pymobiledevice3's notes on what the phone expects.

use super::{audio, config_message, frame_message, audio_message, hevc, Cmd, Input, Phase, Screen};
use crate::device::idev;
use idevice::{
    core_device::{
        build_keyframe_request, build_liveness, build_screen_audio_offer, build_screen_video_offer, build_start_audio_parameters,
        build_start_video_parameters, is_rtcp, ButtonState, CallInfoBlob, CoreDeviceServiceClient, DisplayServiceClient, IndigoHidClient,
        ReportBlock, RtpPacket, UniversalHidServiceClient, TOUCHSCREEN_STATE_CONTACT, TOUCHSCREEN_STATE_RELEASE,
    },
    core_device_proxy::CoreDeviceProxy,
    dvt::{remote_server::RemoteServerClient, screenshot::ScreenshotClient},
    lockdown::LockdownClient,
    provider::{IdeviceProvider, RsdProvider},
    rsd::RsdHandshake,
    IdeviceService, ReadWrite, RsdService,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

/// What Device Hub announces as its own features; the phone's own mask breaks negotiation.
const CLIENT_FEATURES: u64 = 140;
/// Error code CoreDevice returns when Camera or a recording app holds the camera or microphone.
const MEDIA_IN_USE: &str = "9022";

type Hid = UniversalHidServiceClient<Box<dyn ReadWrite>>;
type Indigo = IndigoHidClient<Box<dyn ReadWrite>>;

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
    drop(ld);
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
    // Which way the screen is turned; without it the picture stays upright and the Rotate button helps.
    let mut info = match hs.services.get("com.apple.coredevice.deviceinfo").map(|s| s.port) {
        Some(port) => match adapter.connect_to_service_port(port).await {
            Ok(stream) => CoreDeviceServiceClient::new(stream).await.ok(),
            Err(_) => None,
        },
        None => None,
    };
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
    let input_task = tokio::task::spawn_local(async move {
        let (mut hid, mut indigo) = (hid, indigo);
        while let Some((i, r)) = input_rx.recv().await {
            let _ = r.send(apply(i, hid.as_mut(), indigo.as_mut()).await);
        }
    });

    let started = Instant::now();
    let mut depack = hevc::Depacketizer::new();
    let (mut v_seq, mut a_seq) = (SeqCounter::default(), SeqCounter::default());
    let mut last_keyframe_request = Instant::now() - Duration::from_secs(5);
    let mut fir_seq = 0u8;
    // `want_key`: someone needs a keyframe (a viewer, or lost packets); `refresh`: a still screen
    // gets one now and then for new viewers. Only the first kind counts towards a restart.
    let (mut want_key, mut refresh, mut key_requests) = (true, false, 0u32);
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    let mut orient_tick = tokio::time::interval(Duration::from_millis(500));
    let (mut last_video, mut last_dgram) = (Instant::now(), Instant::now());
    let mut rotation = 0u16;

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
            _ = orient_tick.tick(), if info.is_some() => {
                let answer = match info.as_mut() {
                    Some(c) => tokio::time::timeout(Duration::from_millis(800), c.invoke("com.apple.coredevice.feature.getdisplayinfo", None))
                        .await
                        .ok()
                        .and_then(Result::ok),
                    None => None,
                };
                match answer {
                    Some(v) => {
                        let r = display_rotation(&v);
                        if r != rotation {
                            rotation = r;
                            m.set(session, |s| s.rotation = r);
                        }
                    }
                    None => info = None, // not supported by this iPhone: stop asking
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
            }
        }
    };
    input_task.abort();
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

async fn apply(i: Input, hid: Option<&mut Hid>, indigo: Option<&mut Indigo>) -> Result<(), String> {
    const NO_CONTROL: &str = "The iPhone did not accept touch input in this session. Stop and start mirroring again.";
    let r = match i {
        Input::Touch { phase, x, y } => {
            let state = if phase == Phase::Up { TOUCHSCREEN_STATE_RELEASE } else { TOUCHSCREEN_STATE_CONTACT };
            hid.ok_or(NO_CONTROL)?.send_touchscreen(state, x, y, None).await
        }
        Input::Tap { x, y } => hid.ok_or(NO_CONTROL)?.tap(x, y).await,
        // ~60 samples a second, so the phone sees a drag with a speed rather than a tap.
        Input::Swipe { x1, y1, x2, y2, ms } => hid.ok_or(NO_CONTROL)?.drag(x1, y1, x2, y2, (ms / 16).max(2) as u32, 16).await,
        Input::Button(_, page, usage, hold) => press(indigo.ok_or(NO_CONTROL)?, page, usage, hold).await,
        Input::Key { usage, down } => {
            indigo.ok_or(NO_CONTROL)?.send_keyboard(usage, if down { ButtonState::Down } else { ButtonState::Up }).await
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
