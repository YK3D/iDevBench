"""Screen mirroring: input parsing, HEVC framing and the simulated session (same cases as the standalone)."""

import json
import struct

import pytest
from fastapi.testclient import TestClient

from spoofer import screen as mi


class _Writer:
    def __init__(self):
        self.bits = []

    def put(self, v, n):
        self.bits += [(v >> i) & 1 for i in reversed(range(n))]

    def ue(self, v):
        x = v + 1
        n = x.bit_length()
        self.put(0, n - 1)
        self.put(x, n)

    def finish(self):
        self.bits += [0] * (-len(self.bits) % 8)
        return bytes(int("".join(map(str, self.bits[i : i + 8])), 2) for i in range(0, len(self.bits), 8))


def sample_sps():
    """A 1170x2532 Main-profile SPS (level 6.0), with emulation prevention as an encoder adds it."""
    w = _Writer()
    w.put(0, 4); w.put(0, 3); w.put(1, 1)
    w.put(0, 2); w.put(0, 1); w.put(1, 5); w.put(0x60000000, 32); w.put(0x90, 8); w.put(0, 40); w.put(180, 8)
    w.ue(0); w.ue(1); w.ue(1184); w.ue(2544)
    w.put(1, 1); w.ue(0); w.ue(7); w.ue(0); w.ue(6)
    w.ue(0); w.ue(0); w.put(1, 1)
    nal, zeros = bytearray([mi.NAL_SPS << 1, 1]), 0
    for b in w.finish():
        if zeros >= 2 and b <= 3:
            nal.append(3)
            zeros = 0
        zeros = zeros + 1 if b == 0 else 0
        nal.append(b)
    return bytes(nal)


def nal(t, body=b""):
    return bytes([t << 1, 1]) + body


def with_params(d, seq=0):
    for n in (nal(mi.NAL_VPS, b"\1"), sample_sps(), nal(mi.NAL_PPS, b"\2")):
        d.push(seq, 1000, False, n)
        seq += 1
    return seq


def test_parses_sps_codec_and_hvcc():
    info = mi.parse_sps(sample_sps())
    assert (info["width"], info["height"], info["chroma"]) == (1170, 2532, 1)
    assert mi.codec_string(info) == "hev1.1.6.L180.90"
    r = mi.hvcc(info, nal(32, b"\xaa"), sample_sps(), nal(34, b"\xbb"))
    assert r[0] == 1 and r[1:13] == info["ptl"] and r[16] == 0xFD and r[21] & 3 == 3 and r[22] == 3
    assert r[23] == 0x80 | 32 and r.endswith(nal(34, b"\xbb"))
    assert mi.parse_sps(sample_sps()[:8]) is None


def test_frames_fragments_and_footer():
    d = mi.Depacketizer()
    seq = with_params(d)
    out = d.push(seq, 1000, True, nal(19, b"\x09\x09") + mi.APPLE_SLICE_FOOTER)
    assert out == [(1000, True, [nal(19, b"\x09\x09")])] and d.config_changed
    assert d.decoder_config()["width"] == 1170
    body = bytes(range(30))
    fu = lambda s, e, part: bytes([mi.NAL_FU << 1, 1, (0x80 if s else 0) | (0x40 if e else 0) | 1]) + part
    assert d.push(seq + 1, 2000, False, fu(True, False, body[:10])) == []
    assert d.push(seq + 2, 2000, False, fu(False, False, body[10:20])) == []
    assert d.push(seq + 3, 2000, True, fu(False, True, body[20:])) == [(2000, False, [nal(1, body)])]
    sei, idr = nal(39, b"\5"), nal(19, b"\1\2")
    ap = bytes([mi.NAL_AP << 1, 1]) + b"".join(len(n).to_bytes(2, "big") + n for n in (sei, idr))
    assert d.push(seq + 4, 3000, True, ap) == [(3000, True, [sei, idr])]


def test_loss_holds_frames_until_keyframe():
    d = mi.Depacketizer()
    seq = with_params(d)
    assert len(d.push(seq, 1, True, nal(19, b"\1"))) == 1
    assert d.push(seq + 2, 2, True, nal(1, b"\1")) == [] and d.waiting_for_key and d.lost_packets == 1
    assert d.push(seq + 3, 3, True, nal(1, b"\1")) == []
    assert d.push(seq + 4, 4, True, nal(20, b"\1"))[0][1] is True
    assert d.push(seq + 1, 2, True, nal(1, b"\1")) == []  # late duplicate
    assert d.push(seq + 5, 10, False, nal(1, b"\1")) == []
    assert d.push(seq + 6, 11, False, nal(1, b"\2"))[0][0] == 10  # a new timestamp ends the frame


def test_inputs_and_keys():
    assert mi.parse_input({"t": "touch", "phase": "down", "x": 0.5, "y": 1}) == ("touch", "down", 32768, 65535, None)
    assert mi.parse_input({"t": "touch", "phase": "move", "x": 0, "y": 0, "x2": 1, "y2": 0.5}) == ("touch", "move", 0, 0, (65535, 32768))
    assert mi.parse_input({"t": "gesture", "name": "back"}) == ("gesture", "back")
    assert mi.parse_input({"t": "long_press", "x": 0, "y": 0}) == ("long_press", 0, 0, 800)
    assert mi.parse_input({"t": "key", "usage": 4, "command": True}) == ("key", 4, None, True)
    assert [mi.upright_to_native(0.25, 0, 0), mi.upright_to_native(0, 0.25, 90), mi.upright_to_native(1, 1, 180),
            mi.upright_to_native(0, 0.25, 270)] == [(16384, 0), (16384, 65535), (0, 0), (49151, 0)]
    assert mi.memory({"vmActiveCount": 10, "vmWireCount": 5, "vmFreeCount": 5, "vmInactiveCount": 4}) == (15 * 16384, 24 * 16384)
    assert mi.memory({}) == (None, None)
    assert mi.parse_input({"t": "swipe", "x1": 0, "y1": 0, "x2": 1, "y2": 1})[-1] == 300
    assert mi.parse_input({"t": "key", "usage": 40, "down": False}) == ("key", 40, False, False)
    assert mi.parse_input({"t": "refresh"}) == ("refresh",)
    for bad in ({"t": "tap", "x": 1.5, "y": 0}, {"t": "tap", "x": "0.2", "y": 0}, {"t": "touch", "phase": "hover", "x": 0, "y": 0},
                {"t": "button", "name": "power"}, {"t": "key", "usage": 1, "down": True}, {"t": "fly"}, [],
                {"t": "key", "usage": 4, "down": "yes"}, {"t": "touch", "phase": "down", "x": 0, "y": 0, "x2": 0.5}, {"t": "gesture", "name": "spin"}):
        with pytest.raises(ValueError):
            mi.parse_input(bad)
    assert [mi.char_usage(c) for c in "aZ0(?/"] == [(4, False), (0x1D, True), (0x27, False), (0x26, True), (0x38, True), (0x38, False)]
    assert mi.char_usage("é") is None


def test_message_layouts_match_the_standalone():
    c = {"codec": "hev1.1.6.L180.90", "hvcc": b"\1\2", "width": 1170, "height": 2532}
    assert mi.config_message(c) == bytes([1, 0x04, 0x92, 0x09, 0xE4, 16]) + b"hev1.1.6.L180.90\1\2"
    assert mi.frame_message(True, [b"\x26\1\7"], 5) == bytes([2, 1]) + struct.pack(">Q", 5) + b"\0\0\0\3\x26\1\7"
    assert mi.audio_message(b"\1\0", 2) == bytes([3]) + struct.pack(">Q", 2) + b"\1\0"


def test_rtcp_and_rtp_helpers():
    assert len(mi._rr_sdes(1, 2, 3)) == 44 and mi._rr_sdes(1, 2, 3)[1] == 201
    kf = mi._keyframe_request(1, 2, 3, 4)
    assert len(kf) == 44 + 12 + 20 and kf[45] == 206
    assert mi._frame_ack(7, 9) == struct.pack(">BBHIII", 0x80, 204, 3, 7, 5, 9)
    pkt = bytes([0x80, 0x80 | 100]) + struct.pack(">HII", 7, 99, 1) + b"payload"
    assert mi._rtp(pkt) == (7, 99, True, b"payload")
    assert mi._rtp(bytes([0x80, 201]) + bytes(10)) is None
    s = mi._SeqCounter()
    for seq in (65534, 65535, 0, 1, 65535, 2):
        s.add(seq)
    assert s.value == 0x10002
    answer = {"connection": {"streamConfig": {"SourcePort": 50001, "LocalSSRC": 7, "RemoteSSRC": 9}}}
    assert mi._stream_config(answer) == (50001, 9, 7) and mi._stream_config(None) == (0, 0, 0)


def test_reads_the_screen_rotation():
    info = {"displays": [{"external": True, "currentOrientation": "rot180"}, {"primary": True, "currentOrientation": "rot90"},
                         {"active": True, "currentOrientation": "rot270"}]}
    assert mi.display_rotation(info) == 270
    assert mi.display_rotation({"displays": [{"currentOrientation": "rot90"}]}) == 90
    assert mi.display_rotation(True) == 0


@pytest.fixture()
def client(tmp_path, monkeypatch):
    monkeypatch.setenv("SPOOFER_DATA_DIR", str(tmp_path))
    from spoofer import server
    from spoofer.store import Store

    server.app.state.s = server.AppState()
    server.app.state.s.store = Store(tmp_path / "data.json")
    with TestClient(server.app, base_url="http://127.0.0.1:8765") as c:
        yield c


def test_mirror_routes_with_the_simulator(client):
    assert client.get("/api/screen").json()["state"] == "off"
    assert client.post("/api/screen/input", json={"t": "tap", "x": 0.5, "y": 0.5}).status_code == 409
    s = client.post("/api/screen/start", json={"driver": "mock"}).json()
    assert (s["state"], s["simulated"], s["width"]) == ("on", True, 1170)
    assert client.post("/api/screen/input", json={"t": "text", "text": "hi"}).json() == {"ok": True}
    assert client.post("/api/screen/input", json={"t": "tap", "x": 2, "y": 0}).status_code == 400
    assert client.get("/api/screen/screenshot").status_code == 409
    # The test client sends "Host: testserver" for WebSockets unless told otherwise.
    with client.websocket_connect("/api/screen/ws", headers={"host": "127.0.0.1:8765"}) as ws:
        assert json.loads(ws.receive_text())["status"]["state"] == "on"
        ws.send_text(json.dumps({"t": "button", "name": "nope"}))
        assert json.loads(ws.receive_text())["type"] == "error"
    from spoofer import server

    assert ("text", "hi") in server.app.state.s.screen.sim_inputs
    assert client.post("/api/screen/stop").json()["state"] == "off"


def test_websocket_refuses_other_websites(client):
    from starlette.websockets import WebSocketDisconnect

    with pytest.raises(WebSocketDisconnect):
        with client.websocket_connect("/api/screen/ws", headers={"host": "127.0.0.1:8765", "origin": "https://evil.example"}) as ws:
            ws.receive_text()


def test_screen_captures_go_to_the_chosen_folder(client, tmp_path):
    folder = tmp_path / "captures"
    client.put("/api/store", json={"settings": {"capture_folder": str(folder)}})
    first = client.post("/api/screen/save?name=shot.png", content=b"png").json()["path"]
    second = client.post("/api/screen/save?name=shot.png", content=b"png").json()["path"]
    assert open(first, "rb").read() == b"png" and second.endswith("shot-1.png")
    for bad in ("../x.png", "x.exe", ".png", "a/b.png"):
        assert client.post("/api/screen/save", params={"name": bad}, content=b"x").status_code == 400, bad


def test_unanswered_keyframe_requests_restart_the_stream(monkeypatch):
    import asyncio
    import itertools
    import types

    clock = itertools.count(0, 0.6)  # every look at the clock is 0.6 s later
    monkeypatch.setattr(mi, "time", types.SimpleNamespace(monotonic=lambda: next(clock)))
    live = mi._Live(mi.Screen(), 1)
    live.v_phone, live.v_ours, live.v_port, live.a_phone = 2, 1, 5, 0
    live.v_seq, live.a_seq = mi._SeqCounter(), mi._SeqCounter()
    live.video = live.audio_rx = None
    requests = []

    async def rtcp(_transport, _port, packet):
        live.last_dgram = mi.time.monotonic()  # the phone keeps sending, just no keyframe
        if len(packet) > 44:
            requests.append(packet)

    live._rtcp = rtcp
    with pytest.raises(mi._Restart) as e:
        asyncio.run(asyncio.wait_for(live._feedback(), 10))
    assert e.value.auto and len(requests) == mi.MAX_KEY_REQUESTS
