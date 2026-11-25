#!/usr/bin/env python3
"""Sideport MCP server: lets AI agents (Claude Code, Claude Desktop, Cursor, ...) see and use an iPhone.

It speaks the Model Context Protocol over stdio and forwards each tool to Sideport's local API
(docs/API.md), so Sideport must be open on this computer. Python 3.9+ standard library only.

    claude mcp add sideport -- python /path/to/sideport_mcp.py

Sideport's address is found by itself (the standalone app on port 8766, the browser edition on
8765); set SIDEPORT_URL to use another one.
"""

from __future__ import annotations

import base64
import json
import os
import sys
import time
import urllib.error
import urllib.request
from typing import Any, Callable

VERSION = "1.0.0"
CANDIDATES = [os.environ["SIDEPORT_URL"].rstrip("/")] if os.environ.get("SIDEPORT_URL") else ["http://127.0.0.1:8766", "http://127.0.0.1:8765"]
_base: list[str] = []  # the address that answered last


class ToolError(Exception):
    """Shown to the agent as the tool's result (isError)."""


def http(method: str, path: str, body: Any = None, timeout: float = 70) -> tuple[bytes, str]:
    """One request to Sideport: (body, content type). Tries each known address until one answers."""
    data = json.dumps(body).encode() if body is not None else None
    for base in _base + [c for c in CANDIDATES if c not in _base]:
        req = urllib.request.Request(base + path, data=data, method=method, headers={"Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=timeout) as r:
                _base[:] = [base]
                return r.read(), r.headers.get("Content-Type", "")
        except urllib.error.HTTPError as e:
            _base[:] = [base]
            try:
                detail = json.loads(e.read()).get("detail")
            except Exception:  # noqa: BLE001 - not JSON
                detail = None
            raise ToolError(str(detail or f"Sideport answered HTTP {e.code}")) from None
        except (urllib.error.URLError, OSError):
            continue
    raise ToolError("Sideport is not running. Open the Sideport app on this computer, then try again.")


def api(method: str, path: str, body: Any = None) -> Any:
    return json.loads(http(method, path, body)[0] or b"null")


def screen_input(msg: dict) -> None:
    api("POST", "/api/screen/input", msg)


# ---------------------------------------------------------------- tools
def _num(a: dict, k: str) -> float:
    v = a.get(k)
    if isinstance(v, bool) or not isinstance(v, (int, float)) or not 0 <= v <= 1:
        raise ToolError(f"'{k}' must be a number from 0 to 1 (a fraction of the screen's width or height)")
    return float(v)


def t_status(a: dict) -> list:
    s = api("GET", "/api/status")
    out = {"app_version": s.get("version"), "device": s.get("device"), "location_playback": (s.get("player") or {}).get("state"),
           "screen": api("GET", "/api/screen")}
    return [_text(json.dumps(out, indent=2))]


def t_devices(a: dict) -> list:
    return [_text(json.dumps(api("GET", "/api/devices?driver=pmd3"), indent=2))]


def t_connect(a: dict) -> list:
    r = api("POST", "/api/connect", {"driver": "mock" if a.get("simulator") else "pmd3", "udid": a.get("udid")})
    return [_text(json.dumps(r, indent=2))]


def t_start(a: dict) -> list:
    s = api("POST", "/api/screen/start", {"udid": a.get("udid"), "audio": bool(a.get("audio", False)),
                                          **({"driver": "mock"} if a.get("simulator") else {})})
    return [_text(json.dumps(s, indent=2))]


def t_stop(a: dict) -> list:
    return [_text(json.dumps(api("POST", "/api/screen/stop", {}), indent=2))]


def t_screenshot(a: dict) -> list:
    wait = a.get("wait_ms", 0)
    if isinstance(wait, (int, float)) and not isinstance(wait, bool) and wait > 0:
        time.sleep(min(wait, 10000) / 1000)
    png, _ = http("GET", "/api/screen/screenshot")
    s = api("GET", "/api/screen")
    note = f"iPhone screen ({s.get('width')}×{s.get('height')} pixels). Give positions as fractions: x = pixel x / width, y = pixel y / height."
    return [{"type": "image", "data": base64.b64encode(png).decode(), "mimeType": "image/png"}, _text(note)]


def t_tap(a: dict) -> list:
    screen_input({"t": "tap", "x": _num(a, "x"), "y": _num(a, "y")})
    return [_text("Tapped.")]


def t_swipe(a: dict) -> list:
    ms = a.get("ms", 300)
    screen_input({"t": "swipe", "x1": _num(a, "x1"), "y1": _num(a, "y1"), "x2": _num(a, "x2"), "y2": _num(a, "y2"),
                  "ms": ms if isinstance(ms, int) and not isinstance(ms, bool) else 300})
    return [_text("Swiped.")]


def t_long_press(a: dict) -> list:
    x, y = _num(a, "x"), _num(a, "y")
    ms = a.get("ms", 800)
    screen_input({"t": "touch", "phase": "down", "x": x, "y": y})
    time.sleep(min(max(ms if isinstance(ms, (int, float)) else 800, 100), 5000) / 1000)
    screen_input({"t": "touch", "phase": "up", "x": x, "y": y})
    return [_text("Pressed and held.")]


# System gestures: start and end point (portrait), drag time, hold before lifting.
GESTURES = {
    "home": ((0.5, 0.995), (0.5, 0.55), 0.2, 0),
    "app_switcher": ((0.5, 0.995), (0.5, 0.62), 0.26, 0.45),
    "control_centre": ((0.92, 0.004), (0.92, 0.45), 0.26, 0),
    "notification_centre": ((0.3, 0.004), (0.3, 0.6), 0.26, 0),
    "back": ((0.004, 0.5), (0.65, 0.5), 0.26, 0),
}


def t_gesture(a: dict) -> list:
    name = a.get("name")
    if name not in GESTURES:
        raise ToolError(f"'name' must be one of: {', '.join(GESTURES)}")
    (x1, y1), (x2, y2), secs, hold = GESTURES[name]
    screen_input({"t": "touch", "phase": "down", "x": x1, "y": y1})
    steps = 12
    for i in range(1, steps + 1):
        time.sleep(secs / steps)
        screen_input({"t": "touch", "phase": "move", "x": x1 + (x2 - x1) * i / steps, "y": y1 + (y2 - y1) * i / steps})
    time.sleep(hold)
    screen_input({"t": "touch", "phase": "up", "x": x2, "y": y2})
    return [_text(f"Did the {name.replace('_', ' ')} gesture.")]


BUTTONS = ["home", "lock", "volume-up", "volume-down", "mute", "siri"]


def t_button(a: dict) -> list:
    if a.get("name") not in BUTTONS:
        raise ToolError(f"'name' must be one of: {', '.join(BUTTONS)}")
    screen_input({"t": "button", "name": a["name"]})
    return [_text(f"Pressed {a['name']}.")]


def t_type(a: dict) -> list:
    text = a.get("text")
    if not isinstance(text, str) or not text:
        raise ToolError("'text' must be some text")
    screen_input({"t": "text", "text": text})
    return [_text("Typed.")]


KEYS = {"return": 40, "escape": 41, "backspace": 42, "tab": 43, "space": 44, "delete": 76, "home": 74, "end": 77,
        "page_up": 75, "page_down": 78, "right": 79, "left": 80, "down": 81, "up": 82}
KEYS.update({chr(c): 4 + c - ord("a") for c in range(ord("a"), ord("z") + 1)})
COMMAND = 227


def t_key(a: dict) -> list:
    usage = KEYS.get(str(a.get("key", "")).lower())
    if usage is None:
        raise ToolError(f"'key' must be one of: {', '.join(KEYS)}")
    held = [COMMAND, usage] if a.get("command") else [usage]
    for u in held:
        screen_input({"t": "key", "usage": u, "down": True})
    for u in reversed(held):
        screen_input({"t": "key", "usage": u, "down": False})
    return [_text("Pressed " + ("⌘" if a.get("command") else "") + str(a["key"]) + ".")]


def t_set_location(a: dict) -> list:
    lat, lon = a.get("lat"), a.get("lon")
    if not all(isinstance(v, (int, float)) and not isinstance(v, bool) for v in (lat, lon)):
        raise ToolError("'lat' and 'lon' must be numbers")
    api("POST", "/api/location/set", {"lat": lat, "lon": lon})
    return [_text(f"The iPhone now reports {lat}, {lon}.")]


def t_clear_location(a: dict) -> list:
    api("POST", "/api/location/clear", {})
    return [_text("The iPhone is back on its real location.")]


def _text(t: str) -> dict:
    return {"type": "text", "text": t}


def _schema(props: dict | None = None, required: tuple = ()) -> dict:
    return {"type": "object", "properties": props or {}, "required": list(required), "additionalProperties": False}


FRACTION = {"type": "number", "minimum": 0, "maximum": 1}
TOOLS: dict[str, tuple[str, dict, Callable[[dict], list]]] = {
    "iphone_status": ("Sideport's state: the connected iPhone, the location playback and screen mirroring (state, width, "
                      "height, rotation).", _schema(), t_status),
    "list_devices": ("iPhones plugged into this computer.", _schema(), t_devices),
    "connect_device": ("Connect to an iPhone for location tools (the first one plugged in unless udid is given). "
                       "simulator: true uses a pretend phone.", _schema({"udid": {"type": "string"}, "simulator": {"type": "boolean"}}), t_connect),
    "start_screen": ("Start screen mirroring; needed before screenshots and any touch, button or typing. iOS 27+, "
                     "Developer Mode, cable. Unlock the iPhone first.",
                     _schema({"udid": {"type": "string"}, "audio": {"type": "boolean"}, "simulator": {"type": "boolean"}}), t_start),
    "stop_screen": ("Stop screen mirroring.", _schema(), t_stop),
    "screenshot": ("See the iPhone's screen now (a PNG). wait_ms waits first, e.g. for an animation or a page to load.",
                   _schema({"wait_ms": {"type": "integer", "minimum": 0, "maximum": 10000}}), t_screenshot),
    "tap": ("Tap a point. x and y are fractions of the screenshot's width and height (0,0 top left; 1,1 bottom right).",
            _schema({"x": FRACTION, "y": FRACTION}, ("x", "y")), t_tap),
    "long_press": ("Touch and hold a point (context menus, rearranging icons). ms: how long, default 800.",
                   _schema({"x": FRACTION, "y": FRACTION, "ms": {"type": "integer", "minimum": 100, "maximum": 5000}}, ("x", "y")), t_long_press),
    "swipe": ("Drag a finger from (x1,y1) to (x2,y2) over ms milliseconds (default 300); to scroll down, swipe upwards.",
              _schema({"x1": FRACTION, "y1": FRACTION, "x2": FRACTION, "y2": FRACTION, "ms": {"type": "integer", "minimum": 50, "maximum": 5000}},
                      ("x1", "y1", "x2", "y2")), t_swipe),
    "gesture": ("A system gesture: home (go to the Home Screen), app_switcher, control_centre, notification_centre, "
                "back (swipe in from the left edge).", _schema({"name": {"type": "string", "enum": list(GESTURES)}}, ("name",)), t_gesture),
    "press_button": ("Press a hardware button. siri is a long press.", _schema({"name": {"type": "string", "enum": BUTTONS}}, ("name",)), t_button),
    "type_text": ("Type text into the focused field (tap the field first). US keyboard layout; up to 2000 characters.",
                  _schema({"text": {"type": "string"}}, ("text",)), t_type),
    "press_key": ("Press one key, optionally with ⌘ (command: true), e.g. return to submit, or a with command for Select All.",
                  _schema({"key": {"type": "string", "enum": list(KEYS)}, "command": {"type": "boolean"}}, ("key",)), t_key),
    "set_location": ("Make the iPhone report this GPS position (connect_device first).",
                     _schema({"lat": {"type": "number", "minimum": -90, "maximum": 90}, "lon": {"type": "number", "minimum": -180, "maximum": 180}},
                             ("lat", "lon")), t_set_location),
    "clear_location": ("Send the iPhone back to its real GPS position.", _schema(), t_clear_location),
}

INSTRUCTIONS = ("Controls a real iPhone through the Sideport app. Call start_screen once, then loop: screenshot, decide, "
                "act (tap, swipe, type_text, gesture), screenshot again to check. Positions are fractions of the screenshot.")


# ---------------------------------------------------------------- protocol (JSON-RPC 2.0, one message per line)
def handle(msg: dict) -> dict | None:
    method, mid = msg.get("method"), msg.get("id")
    if mid is None:  # a notification: nothing to answer
        return None
    if method == "initialize":
        version = (msg.get("params") or {}).get("protocolVersion") or "2025-06-18"
        result: Any = {"protocolVersion": version, "capabilities": {"tools": {}},
                       "serverInfo": {"name": "sideport", "version": VERSION}, "instructions": INSTRUCTIONS}
    elif method == "ping":
        result = {}
    elif method == "tools/list":
        result = {"tools": [{"name": n, "description": d, "inputSchema": s} for n, (d, s, _) in TOOLS.items()]}
    elif method == "tools/call":
        params = msg.get("params") or {}
        tool = TOOLS.get(params.get("name"))
        if tool is None:
            return {"jsonrpc": "2.0", "id": mid, "error": {"code": -32602, "message": f"Unknown tool {params.get('name')!r}"}}
        try:
            result = {"content": tool[2](params.get("arguments") or {})}
        except ToolError as e:
            result = {"content": [_text(str(e))], "isError": True}
    else:
        return {"jsonrpc": "2.0", "id": mid, "error": {"code": -32601, "message": f"Method not found: {method}"}}
    return {"jsonrpc": "2.0", "id": mid, "result": result}


def main() -> None:
    for line in sys.stdin:
        if not line.strip():
            continue
        try:
            msg = json.loads(line)
            reply = handle(msg) if isinstance(msg, dict) else {"jsonrpc": "2.0", "id": None, "error": {"code": -32600, "message": "Invalid request"}}
        except json.JSONDecodeError:
            reply = {"jsonrpc": "2.0", "id": None, "error": {"code": -32700, "message": "Parse error"}}
        if reply is not None:
            sys.stdout.write(json.dumps(reply) + "\n")
            sys.stdout.flush()


if __name__ == "__main__":
    main()
