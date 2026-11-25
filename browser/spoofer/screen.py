"""Screen mirroring: the iPhone's screen and sound in the app, and mouse/keyboard control.

Same behaviour, messages and WebSocket format as the standalone app (standalone/src/screen).
iOS 27+ streams its display through ``com.apple.coredevice.displayservice`` as HEVC over RTP
and its sound as AAC-ELD; touches, buttons and keys go back through the CoreDevice HID
services, which only accept input while a stream runs. pymobiledevice3 provides the services.

WebSocket messages to clients (``/api/screen/ws``):

* ``[1][width u16][height u16][codec length u8][codec][hvcC]``: decoder setup;
* ``[2][key u8][timestamp µs u64][frame as 4-byte length-prefixed NAL units]``;
* ``[3][timestamp µs u64][PCM s16le, 48 kHz, stereo]``;
* text: the status JSON whenever it changes.
"""

from __future__ import annotations

import asyncio
import contextlib
import json
import logging
import random
import struct
import sys
import time
import uuid
from contextlib import AsyncExitStack
from typing import Any, Optional

log = logging.getLogger(__name__)

# name -> (HID usage page, usage, hold in seconds). Siri needs a long press.
BUTTONS = {
    "home": (0x0C, 0x40, 0.05),
    "lock": (0x0C, 0x30, 0.05),
    "volume-up": (0x0C, 0xE9, 0.05),
    "volume-down": (0x0C, 0xEA, 0.05),
    "mute": (0x0C, 0xE2, 0.05),
    "siri": (0x0C, 0xCF, 1.0),
}


class ScreenError(Exception):
    """Shown to the user as is (HTTP 409)."""


def _coord(v: dict, k: str) -> int:
    f = v.get(k)
    if isinstance(f, bool) or not isinstance(f, (int, float)):
        raise ValueError(f"'{k}' must be a number from 0 to 1")
    if not 0 <= f <= 1:
        raise ValueError(f"'{k}' must be from 0 to 1 (a fraction of the screen)")
    return round(f * 65535)


def parse_input(v: Any) -> tuple:
    """Parse one input message (coordinates are fractions of the screen, top left 0,0).

    ``{"t":"touch","phase":"down|move|up","x":..,"y":..}``, ``{"t":"tap","x":..,"y":..}``,
    ``{"t":"swipe","x1":..,"y1":..,"x2":..,"y2":..,"ms":300}``, ``{"t":"button","name":"home"}``,
    ``{"t":"key","usage":40,"down":true}``, ``{"t":"text","text":"hi"}``, ``{"t":"keyframe"}``.
    Raises ValueError with a message for the user.
    """
    if not isinstance(v, dict):
        raise ValueError("'t' must be touch, tap, swipe, button, key, text or keyframe")
    t = v.get("t")
    if t == "touch":
        if v.get("phase") not in ("down", "move", "up"):
            raise ValueError("'phase' must be down, move or up")
        return ("touch", v["phase"], _coord(v, "x"), _coord(v, "y"))
    if t == "tap":
        return ("tap", _coord(v, "x"), _coord(v, "y"))
    if t == "swipe":
        ms = v.get("ms", 300)
        ms = min(max(int(ms), 50), 5000) if isinstance(ms, int) and not isinstance(ms, bool) else 300
        return ("swipe", _coord(v, "x1"), _coord(v, "y1"), _coord(v, "x2"), _coord(v, "y2"), ms)
    if t == "button":
        name = v.get("name", "")
        if name not in BUTTONS:
            raise ValueError(f"Unknown button '{name}'. Use one of: home, lock, volume-up, volume-down, mute, siri")
        return ("button", name)
    if t == "key":
        usage = v.get("usage")
        if isinstance(usage, bool) or not isinstance(usage, int) or not 4 <= usage <= 0xE7:
            raise ValueError("'usage' must be a HID keyboard usage (4–231)")
        if not isinstance(v.get("down"), bool):
            raise ValueError("'down' must be true or false")
        return ("key", usage, v["down"])
    if t == "text":
        text = v.get("text")
        if not isinstance(text, str):
            raise ValueError("'text' must be a string")
        if len(text) > 2000:
            raise ValueError("Type at most 2000 characters at a time")
        return ("text", text)
    if t == "keyframe":
        return ("keyframe",)
    raise ValueError("'t' must be touch, tap, swipe, button, key, text or keyframe")


_SHIFTED_DIGITS = "!@#$%^&*()"
_PUNCT = {"-": "_", "=": "+", "[": "{", "]": "}", "\\": "|", ";": ":", "'": '"', "`": "~", ",": "<", ".": ">", "/": "?"}
_PUNCT_USAGE = dict(zip(_PUNCT, (0x2D, 0x2E, 0x2F, 0x30, 0x31, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38)))


def char_usage(c: str) -> Optional[tuple[int, bool]]:
    """HID keyboard usage and whether Shift is needed, for one character (US layout)."""
    if "a" <= c <= "z":
        return 0x04 + ord(c) - ord("a"), False
    if "A" <= c <= "Z":
        return 0x04 + ord(c) - ord("A"), True
    if "1" <= c <= "9":
        return 0x1E + ord(c) - ord("1"), False
    fixed = {"0": (0x27, False), "\n": (0x28, False), "\t": (0x2B, False), " ": (0x2C, False)}
    if c in fixed:
        return fixed[c]
    if c and c in _SHIFTED_DIGITS:
        return 0x1E + _SHIFTED_DIGITS.index(c), True
    for plain, shifted in _PUNCT.items():
        if c in (plain, shifted):
            return _PUNCT_USAGE[plain], c == shifted
    return None


# ---------------------------------------------------------------- HEVC
NAL_VPS, NAL_SPS, NAL_PPS, NAL_AUD, NAL_AP, NAL_FU = 32, 33, 34, 35, 48, 49
# Apple's display service ends every coded slice with this footer; it is not HEVC data.
APPLE_SLICE_FOOTER = bytes.fromhex("04f00ac0000003000004ec0ab003")


def nal_type(nal: bytes) -> int:
    return (nal[0] >> 1) & 0x3F if nal else 0xFF


def _strip_footer(nal: bytes) -> bytes:
    if nal_type(nal) <= 31 and nal.endswith(APPLE_SLICE_FOOTER):
        return nal[: -len(APPLE_SLICE_FOOTER)]
    return nal


class Depacketizer:
    """RTP/HEVC (RFC 7798) to whole frames: ``push`` returns ``[(rtp_ts, key, [nal, ...])]``."""

    def __init__(self) -> None:
        self.expect_seq: Optional[int] = None
        self.fu: Optional[bytearray] = None
        self.cur: list[bytes] = []
        self.cur_ts: Optional[int] = None
        self.cur_key = False
        self.waiting_for_key = True  # after a loss, frames are held back until a keyframe
        self.vps = self.sps = self.pps = None
        self.config_changed = False
        self.lost_packets = 0

    def push(self, seq: int, ts: int, marker: bool, payload: bytes) -> list:
        done: list = []
        if self.expect_seq is not None:
            gap = (seq - self.expect_seq) & 0xFFFF
            if gap >= 0x8000:
                return done  # a late duplicate
            if gap:
                self.lost_packets += gap
                self.fu, self.cur, self.cur_key, self.cur_ts = None, [], False, None
                self.waiting_for_key = True
        self.expect_seq = (seq + 1) & 0xFFFF
        if self.cur_ts is not None and self.cur_ts != ts:
            self._finish(done)
        self.cur_ts = ts
        if len(payload) < 2:
            return done
        t = nal_type(payload)
        if t == NAL_AP:
            i = 2
            while i + 2 <= len(payload):
                n = int.from_bytes(payload[i : i + 2], "big")
                i += 2
                if n == 0 or i + n > len(payload):
                    break
                self._add(payload[i : i + n])
                i += n
        elif t == NAL_FU:
            if len(payload) < 3:
                return done
            h = payload[2]
            if h & 0x80:
                self.fu = bytearray([(payload[0] & 0x81) | ((h & 0x3F) << 1), payload[1]]) + payload[3:]
            elif self.fu is not None:
                self.fu += payload[3:]
            if h & 0x40 and self.fu is not None:
                self._add(_strip_footer(bytes(self.fu)))
                self.fu = None
        else:
            self._add(_strip_footer(bytes(payload)))
        if marker:
            self._finish(done)
        return done

    def _add(self, nal: bytes) -> None:
        t = nal_type(nal)
        names = {NAL_VPS: "vps", NAL_SPS: "sps", NAL_PPS: "pps"}
        if t in names:
            if getattr(self, names[t]) != nal:
                setattr(self, names[t], nal)
                self.config_changed = True
            return
        if t == NAL_AUD:
            return
        if 16 <= t <= 21:
            self.cur_key = True
        self.cur.append(nal)

    def _finish(self, done: list) -> None:
        nals, key, ts = self.cur, self.cur_key, self.cur_ts or 0
        self.cur, self.cur_key, self.cur_ts, self.fu = [], False, None, None
        if not nals:
            return
        if self.waiting_for_key:
            if not key or None in (self.vps, self.sps, self.pps):
                return
            self.waiting_for_key = False
        done.append((ts, key, nals))

    def decoder_config(self) -> Optional[dict]:
        if None in (self.vps, self.sps, self.pps):
            return None
        info = parse_sps(self.sps)
        if info is None:
            return None
        return {"codec": codec_string(info), "hvcc": hvcc(info, self.vps, self.sps, self.pps), "width": info["width"], "height": info["height"]}


def rbsp(nal: bytes) -> bytes:
    """Remove emulation-prevention bytes (00 00 03 -> 00 00) after the 2-byte NAL header."""
    out, zeros = bytearray(), 0
    for b in nal[2:]:
        if zeros >= 2 and b == 3:
            zeros = 0
            continue
        zeros = zeros + 1 if b == 0 else 0
        out.append(b)
    return bytes(out)


class _Bits:
    def __init__(self, d: bytes) -> None:
        self.d, self.pos = d, 0

    def bits(self, n: int) -> int:
        v = 0
        for _ in range(n):
            byte = self.d[self.pos // 8]  # IndexError on a short SPS: caught by parse_sps
            v = (v << 1) | ((byte >> (7 - self.pos % 8)) & 1)
            self.pos += 1
        return v

    def ue(self) -> int:
        zeros = 0
        while self.bits(1) == 0:
            zeros += 1
            if zeros > 31:
                raise ValueError("bad exp-Golomb code")
        return (1 << zeros) - 1 + self.bits(zeros)


def parse_sps(sps: bytes) -> Optional[dict]:
    """What the SPS says: the 12-byte general profile_tier_level, size, chroma and bit depths."""
    if nal_type(sps) != NAL_SPS:
        return None
    d = rbsp(sps)
    b = _Bits(d)
    try:
        b.bits(4)
        sub = b.bits(3)
        nested = b.bits(1)
        ptl = d[b.pos // 8 : b.pos // 8 + 12]
        if len(ptl) != 12:
            return None
        b.pos += 96
        flags = [(b.bits(1), b.bits(1)) for _ in range(sub)]
        if sub > 0:
            for _ in range(sub, 8):
                b.bits(2)
        for profile, level in flags:
            b.pos += 88 * profile + 8 * level
        b.ue()
        chroma = b.ue()
        if chroma == 3:
            b.bits(1)
        width, height = b.ue(), b.ue()
        if b.bits(1):
            left, right, top, bottom = b.ue(), b.ue(), b.ue(), b.ue()
            sx, sy = {1: (2, 2), 2: (2, 1)}.get(chroma, (1, 1))
            width, height = max(0, width - sx * (left + right)), max(0, height - sy * (top + bottom))
        depth_luma, depth_chroma = b.ue() + 8, b.ue() + 8
    except (IndexError, ValueError):
        return None
    return {"ptl": bytes(ptl), "max_sub_layers": sub + 1, "nested": bool(nested), "chroma": chroma,
            "depth_luma": depth_luma, "depth_chroma": depth_chroma, "width": width, "height": height}


def codec_string(info: dict) -> str:
    """WebCodecs / RFC 6381 codec string (ISO/IEC 14496-15 Annex E), e.g. ``hev1.1.6.L180.90``."""
    p = info["ptl"]
    compat = int("{:032b}".format(int.from_bytes(p[1:5], "big"))[::-1], 2)
    s = f"hev1.{['', 'A', 'B', 'C'][p[0] >> 6]}{p[0] & 0x1F}.{compat:X}.{'H' if p[0] & 0x20 else 'L'}{p[11]}"
    constraints = p[5:11].rstrip(b"\0")
    return s + "".join(f".{c:X}" for c in constraints)


def hvcc(info: dict, vps: bytes, sps: bytes, pps: bytes) -> bytes:
    """HEVCDecoderConfigurationRecord (``hvcC``), the WebCodecs ``description`` for HEVC."""
    r = bytearray([1]) + info["ptl"]
    r += bytes([0xF0, 0x00, 0xFC, 0xFC | (info["chroma"] & 3), 0xF8 | ((info["depth_luma"] - 8) & 7),
                0xF8 | ((info["depth_chroma"] - 8) & 7), 0, 0])
    r.append(((info["max_sub_layers"] & 7) << 3) | (int(info["nested"]) << 2) | 3)
    r.append(3)
    for t, nal in ((NAL_VPS, vps), (NAL_SPS, sps), (NAL_PPS, pps)):
        r += bytes([0x80 | t]) + (1).to_bytes(2, "big") + len(nal).to_bytes(2, "big") + nal
    return bytes(r)


def config_message(c: dict) -> bytes:
    codec = c["codec"].encode()
    return bytes([1]) + struct.pack(">HHB", min(c["width"], 65535), min(c["height"], 65535), len(codec)) + codec + c["hvcc"]


def frame_message(key: bool, nals: list, ts_us: int) -> bytes:
    return bytes([2, int(key)]) + struct.pack(">Q", ts_us) + b"".join(len(n).to_bytes(4, "big") + n for n in nals)


def audio_message(pcm: bytes, ts_us: int) -> bytes:
    return bytes([3]) + struct.pack(">Q", ts_us) + pcm


# ---------------------------------------------------------------- RTCP (RFC 3550 / 4585)
def _rr_sdes(ours: int, phone: int, highest_seq: int) -> bytes:
    """Receiver report plus an empty CNAME, as Xcode sends them; keeps a stream alive."""
    rr = struct.pack(">BBHII4xIIII", 0x81, 201, 7, ours, phone, highest_seq, 0, 0, 0)
    return rr + struct.pack(">BBHIBBBB", 0x81, 202, 2, ours, 1, 0, 0, 0)


def _keyframe_request(ours: int, phone: int, highest_seq: int, fir_seq: int) -> bytes:
    pli = struct.pack(">BBHII", 0x81, 206, 2, ours, phone)
    fir = struct.pack(">BBHIIIB3x", 0x84, 206, 4, ours, 0, phone, fir_seq & 0xFF)
    return _rr_sdes(ours, phone, highest_seq) + pli + fir


def _frame_ack(ours: int, rtp_ts: int) -> bytes:
    return struct.pack(">BBHIII", 0x80, 204, 3, ours, 5, rtp_ts)


def _rtp(data: bytes) -> Optional[tuple[int, int, bool, bytes]]:
    """(seq, timestamp, marker, payload) of an RTP packet, or None for RTCP or junk."""
    if len(data) < 12 or data[0] >> 6 != 2 or 200 <= data[1] <= 206:
        return None
    off = 12 + 4 * (data[0] & 0x0F)
    if data[0] & 0x10:
        if off + 4 > len(data):
            return None
        off += 4 + 4 * int.from_bytes(data[off + 2 : off + 4], "big")
    end = len(data) - (data[-1] if data[0] & 0x20 else 0)
    if off > end:
        return None
    seq, ts = struct.unpack(">HI", data[2:8])
    return seq, ts, bool(data[1] & 0x80), data[off:end]


class _SeqCounter:
    """Extended highest RTP sequence number; a late packet from before a wrap is ignored."""

    def __init__(self) -> None:
        self.value: Optional[int] = None

    def add(self, seq: int) -> None:
        if self.value is None:
            self.value = seq
            return
        d = (seq - self.value) & 0xFFFF
        if 0 < d < 0x8000:
            self.value = (self.value + d) & 0xFFFFFFFF


def _stream_config(answer: Any) -> tuple[int, int, int]:
    """(phone RTP port, our SSRC, phone SSRC); the answer names them from the phone's side."""
    try:
        cfg = answer["connection"]["streamConfig"]
    except (KeyError, TypeError):
        return 0, 0, 0
    return int(cfg.get("SourcePort", 0)), int(cfg.get("RemoteSSRC", 0)), int(cfg.get("LocalSSRC", 0))


def _status(**kw) -> dict:
    s = {"state": "off", "error": None, "simulated": False, "width": None, "height": None, "audio": False, "note": None}
    s.update(kw)
    return s


class _Client:
    def __init__(self) -> None:
        self.queue: asyncio.Queue = asyncio.Queue(maxsize=512)
        self.need_key = True


class Screen:
    def __init__(self) -> None:
        self.status = _status()
        self.session = 0
        self.config: Optional[bytes] = None
        self.clients: set[_Client] = set()
        self.sim_inputs: list[tuple] = []
        self._live: Optional["_Live"] = None
        self._task: Optional[asyncio.Task] = None

    # ---- clients
    def subscribe(self) -> _Client:
        c = _Client()
        self.clients.add(c)
        return c

    def unsubscribe(self, c: _Client) -> None:
        self.clients.discard(c)

    def _broadcast(self, msg: Any) -> None:
        for c in list(self.clients):
            if isinstance(msg, (bytes, bytearray)) and msg[0] == 2:
                if c.need_key and not msg[1]:
                    continue
                c.need_key = False
            try:
                c.queue.put_nowait(msg)
            except asyncio.QueueFull:  # a slow viewer: start it again from the next keyframe
                while not c.queue.empty():
                    c.queue.get_nowait()
                c.need_key = True
                if self._live:
                    self._live.want_key = True

    def _set(self, session: int, **kw) -> None:
        if session != self.session:
            return
        self.status.update(kw)
        self._broadcast(json.dumps({"type": "status", "status": self.status}))

    def _ended(self, session: int, error: Optional[str]) -> None:
        if session != self.session:
            return
        self._live, self.config = None, None
        self.status = _status(state="error" if error else "off", error=error)
        self._broadcast(json.dumps({"type": "status", "status": self.status}))

    # ---- control
    async def start(self, simulated: bool, udid: Optional[str], audio: bool) -> dict:
        await self._cancel()
        self.session += 1
        session = self.session
        self.config, self.sim_inputs = None, []
        self.status = _status(state="starting", simulated=simulated)
        self._set(session)
        if simulated:
            self._live = None
            self._set(session, state="on", width=1170, height=2532,
                      note="Simulator: there is no real screen; input is accepted and ignored.")
            return dict(self.status)
        ready: asyncio.Future = asyncio.get_running_loop().create_future()
        self._task = asyncio.create_task(self._run(session, udid, audio, ready))
        try:
            await asyncio.wait_for(asyncio.shield(ready), 60)
        except asyncio.TimeoutError:
            await self.stop()
            raise ScreenError("Timed out starting screen mirroring. Unlock the iPhone and try again.") from None
        return dict(self.status)

    async def _cancel(self) -> None:
        task, self._task = self._task, None
        if task and not task.done():
            task.cancel()
            with contextlib.suppress(asyncio.CancelledError, Exception):
                await task

    async def stop(self) -> dict:
        session = self.session
        await self._cancel()
        self._ended(session, None)
        return dict(self.status)

    def _require(self) -> Optional["_Live"]:
        if self.status["state"] != "on":
            raise ScreenError("Screen mirroring is not running. Start it first.")
        return self._live

    async def input(self, i: tuple) -> None:
        live = self._require()
        if live is None:  # simulator
            if len(self.sim_inputs) < 1000:
                self.sim_inputs.append(i)
            return
        await live.apply(i)

    async def screenshot(self) -> bytes:
        live = self._require()
        if live is None:
            raise ScreenError("The simulator has no screen to capture.")
        return await live.screenshot()

    async def _run(self, session: int, udid: Optional[str], audio: bool, ready: asyncio.Future) -> None:
        error = None
        try:
            async with AsyncExitStack() as stack:
                live = _Live(self, session)
                await live.open(stack, udid, audio)
                self._live = live
                ready.set_result(None)
                await live.loop()
        except asyncio.CancelledError:
            raise
        except ScreenError as e:
            error = str(e)
        except Exception as e:  # noqa: BLE001 - shown to the user
            log.exception("screen mirroring failed")
            from .device.pmd3 import _friendly

            error = _friendly(e)
        finally:
            if not ready.done():
                ready.set_exception(ScreenError(error or "Screen mirroring stopped."))
            self._ended(session, error)


MEDIA_IN_USE_CODE = 9022


class _Live:
    """One mirroring session with a real iPhone."""

    def __init__(self, m: Screen, session: int) -> None:
        self.m, self.session = m, session
        self.want_key = True
        self.hid = self.indigo = None
        self.held: set[int] = set()
        self.kb_id: Optional[int] = None

    async def open(self, stack: AsyncExitStack, udid: Optional[str], audio: bool) -> None:
        from pymobiledevice3.lockdown import create_using_usbmux
        from pymobiledevice3.remote.core_device.display_service import DisplayService
        from pymobiledevice3.remote.core_device.hid_service import IndigoHIDService, UniversalHIDServiceService
        from pymobiledevice3.remote.core_device.screen_stream import open_media_receiver

        from .device.pmd3 import Pmd3Driver, _major

        lockdown = await create_using_usbmux(serial=udid, autopair=True)
        stack.push_async_callback(lockdown.close)
        version = lockdown.product_version
        if _major(version) < 27:
            raise ScreenError(f"Screen mirroring needs iOS 27 or newer. This iPhone has iOS {version}.")
        driver = Pmd3Driver()
        self.rsd, _how = await driver._open_rsd(stack, udid or lockdown.udid, lockdown.udid)
        mount_note = await driver._try_mount(self.rsd)
        try:
            self.display = await stack.enter_async_context(DisplayService(self.rsd))
        except Exception as e:  # noqa: BLE001
            raise ScreenError(f"The iPhone's screen service is not available ({e}). {mount_note or 'Check that Developer Mode is on.'}") from e
        self.video, receiver_ip = open_media_receiver(self.display, (8 << 20, 4 << 20))
        stack.callback(self.video.close)
        self.audio_rx, _ = open_media_receiver(self.display, (1 << 20,))
        stack.callback(self.audio_rx.close)
        self.sender_ip = self.rsd.service.address[0]
        sid = uuid.uuid4()
        try:
            # The audio stream goes first: it sets up the screen-sharing session the video joins.
            a = await self.display.start_audio_stream(receiver_ip, self.audio_rx.port, self.sender_ip, client_session_id=sid)
            # shortcut: display 1 is the built-in screen; foldables with two panels would need the active one.
            v = await self.display.start_video_stream(receiver_ip, self.video.port, self.sender_ip, display_id=1, client_session_id=sid)
        except Exception as e:
            if getattr(e, "code", None) == MEDIA_IN_USE_CODE:
                raise ScreenError("The iPhone won't share its screen while an app uses the camera or microphone "
                                  "(for example Camera or Voice Memos). Close that app and try again.") from e
            raise
        stack.push_async_callback(self._stop_streams)
        self.a_port, self.a_ours, self.a_phone = _stream_config(a)
        self.v_port, self.v_ours, self.v_phone = _stream_config(v)
        self.v_ours = self.v_ours or random.getrandbits(32)

        await asyncio.sleep(0.3)  # the phone matches HID input to the new stream first
        notes = []
        try:
            self.hid = await stack.enter_async_context(UniversalHIDServiceService(self.rsd))
            self.indigo = await stack.enter_async_context(IndigoHIDService(self.rsd))
        except Exception:  # noqa: BLE001
            log.exception("HID services unavailable")
            self.hid = self.indigo = None
            notes.append("The iPhone did not accept touch input, so it can only be watched.")
        self.decoder = None
        if audio:
            if sys.platform == "darwin":
                from pymobiledevice3.remote.core_device.aac_eld import AAC_ELD_ASC_48K_STEREO_480, AACELDDecoder

                self.decoder = AACELDDecoder(AAC_ELD_ASC_48K_STEREO_480)
            else:
                notes.append("The browser edition can only play the iPhone's sound on a Mac; the standalone app plays it on Windows.")
        self.m._set(self.session, state="on", audio=self.decoder is not None, note=" ".join(notes) or None)

    async def _stop_streams(self) -> None:
        with contextlib.suppress(Exception):
            await asyncio.wait_for(self.display.stop_media_stream(), 3)

    async def loop(self) -> None:
        self.started = time.monotonic()
        self.v_seq, self.a_seq = _SeqCounter(), _SeqCounter()
        tasks = [asyncio.create_task(c) for c in (self._video(), self._audio(), self._feedback())]
        try:
            done, _ = await asyncio.wait(tasks, return_when=asyncio.FIRST_COMPLETED)
            for t in done:
                t.result()
        finally:
            for t in tasks:
                t.cancel()
        raise ScreenError("The connection to the iPhone was lost.")

    def _ts(self) -> int:
        return int((time.monotonic() - self.started) * 1e6)

    async def _video(self) -> None:
        depack = Depacketizer()
        while True:
            p = _rtp(await self.video.recv(65535))
            if p is None:
                continue
            self.last_video = time.monotonic()
            seq, ts, marker, payload = p
            self.v_seq.add(seq)
            was_waiting = depack.waiting_for_key
            frames = depack.push(seq, ts, marker, payload)
            if depack.waiting_for_key and not was_waiting:
                self.want_key = True
            for rtp_ts, key, nals in frames:
                if depack.config_changed:
                    c = depack.decoder_config()
                    if c:
                        self.m._set(self.session, width=c["width"], height=c["height"])
                        self.m.config = config_message(c)
                        self.m._broadcast(self.m.config)
                    depack.config_changed = False
                self.m._broadcast(frame_message(key, nals, self._ts()))
                await self._rtcp(self.video, self.v_port, _frame_ack(self.v_ours, rtp_ts))

    async def _audio(self) -> None:
        while True:
            p = _rtp(await self.audio_rx.recv(65535))
            if p is None:
                continue
            self.a_seq.add(p[0])
            if self.decoder is not None:
                try:
                    pcm = self.decoder.decode(p[3])
                except Exception:  # noqa: BLE001 - one bad packet
                    continue
                if pcm:
                    self.m._broadcast(audio_message(pcm, self._ts()))

    async def _rtcp(self, transport, port: int, packet: bytes) -> None:
        if port:
            with contextlib.suppress(OSError):
                await transport.sendto(packet, self.sender_ip, port)

    async def _feedback(self) -> None:
        """Receiver reports every second (the phone stops after ~20 s without them) and
        rate-limited keyframe requests."""
        fir_seq, last_report, last_request = 0, 0.0, 0.0
        self.last_video = time.monotonic()
        while True:
            await asyncio.sleep(0.1)
            now = time.monotonic()
            if now - self.last_video > 4:  # a still screen sends nothing: refresh for new viewers
                self.want_key, self.last_video = True, now
            if self.want_key and now - last_request > 0.7 and self.v_phone:
                await self._rtcp(self.video, self.v_port, _keyframe_request(self.v_ours, self.v_phone, self.v_seq.value or 0, fir_seq))
                fir_seq, last_request, self.want_key = fir_seq + 1, now, False
            if now - last_report >= 1:
                last_report = now
                if self.v_phone:
                    await self._rtcp(self.video, self.v_port, _rr_sdes(self.v_ours, self.v_phone, self.v_seq.value or 0))
                if self.a_phone:
                    await self._rtcp(self.audio_rx, self.a_port, _rr_sdes(self.a_ours, self.a_phone, self.a_seq.value or 0))

    # ---- input
    async def apply(self, i: tuple) -> None:
        from pymobiledevice3.remote.core_device.hid_service import (
            HID_BUTTON_STATE_DOWN,
            HID_BUTTON_STATE_UP,
            TOUCHSCREEN_STATE_CONTACT,
            TOUCHSCREEN_STATE_RELEASE,
        )

        kind = i[0]
        if kind == "keyframe":
            self.want_key = True
            return
        if self.hid is None or self.indigo is None:
            raise ScreenError("The iPhone did not accept touch input in this session. Stop and start mirroring again.")
        if kind == "touch":
            state = TOUCHSCREEN_STATE_RELEASE if i[1] == "up" else TOUCHSCREEN_STATE_CONTACT
            await self.hid.send_touchscreen(state, i[2], i[3])
        elif kind == "tap":
            await self.hid.send_touchscreen(TOUCHSCREEN_STATE_CONTACT, i[1], i[2])
            await asyncio.sleep(0.05)
            await self.hid.send_touchscreen(TOUCHSCREEN_STATE_RELEASE, i[1], i[2])
        elif kind == "swipe":
            x1, y1, x2, y2, ms = i[1:]
            steps = max(ms // 16, 2)  # ~60 samples a second, so the phone sees a drag with a speed
            for n in range(steps + 1):
                f = n / steps
                await self.hid.send_touchscreen(TOUCHSCREEN_STATE_CONTACT, round(x1 + (x2 - x1) * f), round(y1 + (y2 - y1) * f))
                await asyncio.sleep(0.016)
            await self.hid.send_touchscreen(TOUCHSCREEN_STATE_RELEASE, x2, y2)
        elif kind == "button":
            page, usage, hold = BUTTONS[i[1]]
            await self.indigo.send_button(page, usage, HID_BUTTON_STATE_DOWN)
            await asyncio.sleep(hold)
            await self.indigo.send_button(page, usage, HID_BUTTON_STATE_UP)
        elif kind == "key":
            (self.held.add if i[2] else self.held.discard)(i[1])
            await self._keys(self.held)
        elif kind == "text":
            for ch in i[1]:
                u = char_usage(ch)
                if u:
                    await self._keys({u[0], 0xE1} if u[1] else {u[0]})
                    await self._keys(set())
                    await asyncio.sleep(0.012)  # keystrokes sent back to back get merged

    async def _keys(self, held: set[int]) -> None:
        if self.kb_id is None:
            self.kb_id = await self.hid.create_keyboard_service()
        await self.hid.send_keyboard(self.kb_id, sorted(held))

    async def screenshot(self) -> bytes:
        from pymobiledevice3.services.dvt.instruments.dvt_provider import DvtProvider
        from pymobiledevice3.services.dvt.instruments.screenshot import Screenshot

        async with DvtProvider(self.rsd) as dvt, Screenshot(dvt) as shot:
            return await shot.get_screenshot()
