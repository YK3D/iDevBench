# Sideport local API

While Sideport is open, other programs on the same computer can use the same HTTP API as its own window: connect to the iPhone, set its location, watch its screen and control it.

* **Address:** `http://127.0.0.1:8766` for the standalone app, `http://127.0.0.1:8765` for the browser edition. Start the app with `--port <number>` to choose another port; if the standalone app's port is taken by another program, it picks a free one at random.
* **Who can use it:** programs on this computer. Requests must use the host `127.0.0.1` or `localhost`. Requests from websites (any `Origin` header other than the app's own page) are refused with `403`, and the server is not reachable from other computers.
* **Format:** JSON in and out. Errors come back as `{"detail": "message"}` with status `400` (bad request), `409` (the device or mirroring is not ready) or `502` (an online service failed).
* **Stability:** these endpoints are what the app's own window uses. They are kept stable where possible, but check the [changelog](../CHANGELOG.md) when you update.

## Device and location

| Method and path | Body | What it does |
|---|---|---|
| `GET /api/status` | | App version, the connected device, playback state |
| `GET /api/devices?driver=pmd3` | | iPhones found over USB (`driver=mock` lists the simulator) |
| `POST /api/connect` | `{"driver": "pmd3", "udid": null}` | Connect (first iPhone when `udid` is null). Use `"mock"` to try things without a phone |
| `POST /api/disconnect` | `{}` | Disconnect |
| `POST /api/location/set` | `{"lat": 51.5007, "lon": -0.1246}` | Set the simulated location |
| `POST /api/location/clear` | `{}` | Back to the real GPS |

```bash
curl -X POST http://127.0.0.1:8766/api/connect -H "Content-Type: application/json" -d '{"driver":"pmd3"}'
```

```bash
curl -X POST http://127.0.0.1:8766/api/location/set -H "Content-Type: application/json" -d '{"lat":48.8584,"lon":2.2945}'
```

## Screen mirroring and control

Needs iOS 27 or newer. The phone only accepts touches and keys while mirroring runs, so start it first (the app's Mirror tab and other programs share one session).

| Method and path | Body | What it does |
|---|---|---|
| `GET /api/screen` | | Status: `state` (`off`, `starting`, `on`, `error`), `error`, `simulated`, `width`, `height` (screen pixels), `audio`, `note`, `rotation` (see below) |
| `POST /api/screen/start` | `{"driver": "pmd3", "udid": null, "audio": true}` | Start (all fields optional; defaults to the device the app is connected to). Returns the status once the picture is flowing |
| `POST /api/screen/stop` | `{}` | Stop |
| `POST /api/screen/input` | one input, see below | Tap, swipe, press a button or type |
| `GET /api/screen/screenshot` | | A PNG of the screen, taken by the phone |
| `GET /api/screen/ws` | WebSocket | Live picture, sound and status; send inputs on it too |
| `POST /api/screen/save?name=<file>` | the file | Saves a screenshot or recording (`.png`, `.mp4`, `.webm`) to the folder chosen in *Settings › Screen* (Downloads by default); answers `{"path": ...}` |

### Inputs

Coordinates are fractions of the screen as the phone holds it natively (portrait on an iPhone): `0,0` is the top left, `1,1` the bottom right. Multiply by `width`/`height` from the status to work in pixels.

The picture always arrives in that native orientation. When the phone is turned, the status's `rotation` says how many degrees clockwise (0, 90, 180 or 270) to turn the picture to show it upright; turn your coordinates back the same way before sending them.

| Input | Example |
|---|---|
| Tap | `{"t": "tap", "x": 0.5, "y": 0.9}` |
| Swipe (`ms` 50–5000, default 300) | `{"t": "swipe", "x1": 0.5, "y1": 0.8, "x2": 0.5, "y2": 0.2, "ms": 300}` |
| One finger, step by step | `{"t": "touch", "phase": "down", "x": 0.5, "y": 0.5}`, then `"move"`, then `"up"` |
| Hardware button | `{"t": "button", "name": "home"}`: `home`, `lock`, `volume-up`, `volume-down`, `mute`, `siri` |
| Type text (US keyboard layout, up to 2000 characters) | `{"t": "text", "text": "Hello"}` |
| One key down or up ([HID keyboard usage](https://usb.org/sites/default/files/hut1_5.pdf), page 7) | `{"t": "key", "usage": 40, "down": true}` (40 is Return) |
| Ask for a fresh full frame | `{"t": "keyframe"}` |
| Restart the picture (if it froze; the app also does this by itself after a few seconds) | `{"t": "refresh"}` |

```bash
curl -X POST http://127.0.0.1:8766/api/screen/start -H "Content-Type: application/json" -d '{}'
```

```bash
curl -X POST http://127.0.0.1:8766/api/screen/input -H "Content-Type: application/json" -d '{"t":"tap","x":0.5,"y":0.5}'
```

```bash
curl -o screen.png http://127.0.0.1:8766/api/screen/screenshot
```

### The WebSocket

`ws://127.0.0.1:8766/api/screen/ws` sends:

* **text**: `{"type": "status", "status": {...}}` when the status changes (one straight away), and `{"type": "error", "detail": "..."}` when an input you sent failed;
* **binary**, first byte is the kind, numbers are big-endian:
  * `1` decoder setup: `width` (u16), `height` (u16), codec string length (u8), codec string (for example `hev1.1.6.L150.90`), then the HEVC decoder configuration record (`hvcC`);
  * `2` frame: keyframe flag (u8, 1 = key), timestamp in microseconds (u64), then the frame as HEVC NAL units, each with a 4-byte length;
  * `3` sound: timestamp in microseconds (u64), then PCM: 16-bit little-endian, 48 kHz, stereo, interleaved.

A new connection gets the decoder setup (if the picture has started) and frames from the next keyframe on. Send inputs as text messages with the same JSON as `POST /api/screen/input`.

To decode in a browser, use WebCodecs: `VideoDecoder.configure({codec, description: hvcC})`, then `new EncodedVideoChunk({type: key ? "key" : "delta", timestamp, data})` for each frame. To save the raw stream for a player, write the `hvcC` parameter sets and each NAL unit with `00 00 00 01` start codes instead of lengths; `ffplay` and VLC play the result.

### Example: Python

```python
import json, requests
from websockets.sync.client import connect  # pip install requests websockets

base = "http://127.0.0.1:8766"
print(requests.post(f"{base}/api/screen/start", json={}).json())
requests.post(f"{base}/api/screen/input", json={"t": "button", "name": "home"}).raise_for_status()
requests.post(f"{base}/api/screen/input", json={"t": "swipe", "x1": 0.5, "y1": 0.8, "x2": 0.5, "y2": 0.3})
open("screen.png", "wb").write(requests.get(f"{base}/api/screen/screenshot").content)

with connect("ws://127.0.0.1:8766/api/screen/ws") as ws:
    for _ in range(100):
        msg = ws.recv()
        if isinstance(msg, str):
            print(json.loads(msg))
        elif msg[0] == 2:
            print("frame", "key" if msg[1] else "delta", len(msg) - 10, "bytes")
```

## AI agents (MCP)

[`agents/sideport_mcp.py`](../agents/sideport_mcp.py) is a [Model Context Protocol](https://modelcontextprotocol.io) server that gives AI agents (Claude Code, Claude Desktop, Cursor and others) tools to use the iPhone through this API: `screenshot`, `tap`, `swipe`, `long_press`, `gesture` (home, app switcher, Control Centre, Notification Centre, back), `press_button`, `type_text`, `press_key`, `set_location`, `clear_location`, `start_screen`, `stop_screen`, `iphone_status`, `list_devices` and `connect_device`. It needs Python 3.9 or newer (nothing to install) and Sideport open.

Claude Code:

```bash
claude mcp add sideport -- python C:\path\to\sideport_mcp.py
```

Claude Desktop, Cursor and other apps take the same command in their MCP settings, for example:

```json
{ "mcpServers": { "sideport": { "command": "python", "args": ["C:\\path\\to\\sideport_mcp.py"] } } }
```

[`agents/skills/sideport-iphone`](../agents/skills/sideport-iphone/SKILL.md) is a skill that teaches an agent how to work with the phone (look, act, look again; positions as fractions; what not to do on someone's real phone). For Claude Code, copy the folder to `~/.claude/skills/` (or a project's `.claude/skills/`).

The agent sees what the screenshots show, and the AI service it runs on receives them. Keep an eye on what it does: its taps act on the real phone.
