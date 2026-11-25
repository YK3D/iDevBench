"""The MCP server for AI agents (agents/sideport_mcp.py) against this edition's API, with the simulator."""

import importlib.util
import json
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

spec = importlib.util.spec_from_file_location("sideport_mcp", Path(__file__).resolve().parents[2] / "agents" / "sideport_mcp.py")
mcp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mcp)


@pytest.fixture()
def rpc(tmp_path, monkeypatch):
    monkeypatch.setenv("SPOOFER_DATA_DIR", str(tmp_path))
    from spoofer import server
    from spoofer.store import Store

    server.app.state.s = server.AppState()
    server.app.state.s.store = Store(tmp_path / "data.json")
    with TestClient(server.app, base_url="http://127.0.0.1:8765") as c:
        def http(method, path, body=None, timeout=70):
            r = c.request(method, path, json=body)
            if r.status_code >= 400:
                raise mcp.ToolError(r.json().get("detail"))
            return r.content, r.headers.get("content-type", "")

        monkeypatch.setattr(mcp, "http", http)
        monkeypatch.setattr(mcp.time, "sleep", lambda s: None)
        ids = iter(range(1, 1000))
        yield lambda method, params=None: mcp.handle({"jsonrpc": "2.0", "id": next(ids), "method": method, "params": params or {}})


def call(rpc, tool, **args):
    return rpc("tools/call", {"name": tool, "arguments": args})["result"]


def test_handshake_and_tool_list(rpc):
    init = rpc("initialize", {"protocolVersion": "2025-06-18"})["result"]
    assert init["serverInfo"]["name"] == "sideport" and "tools" in init["capabilities"]
    assert mcp.handle({"jsonrpc": "2.0", "method": "notifications/initialized"}) is None
    tools = {t["name"]: t for t in rpc("tools/list")["result"]["tools"]}
    assert {"screenshot", "tap", "swipe", "gesture", "type_text", "set_location"} <= set(tools)
    assert tools["tap"]["inputSchema"]["required"] == ["x", "y"]
    assert rpc("nope")["error"]["code"] == -32601


def test_drives_the_simulated_iphone(rpc):
    assert call(rpc, "tap", x=0.5, y=0.5)["isError"]  # not mirroring yet
    s = json.loads(call(rpc, "start_screen", simulator=True)["content"][0]["text"])
    assert s["state"] == "on"
    for tool, args in [("tap", {"x": 0.5, "y": 0.9}), ("swipe", {"x1": 0.5, "y1": 0.8, "x2": 0.5, "y2": 0.2}),
                       ("gesture", {"name": "app_switcher"}), ("press_button", {"name": "home"}), ("type_text", {"text": "hi"}),
                       ("press_key", {"key": "a", "command": True}), ("long_press", {"x": 0.2, "y": 0.2})]:
        assert not call(rpc, tool, **args).get("isError"), tool
    sent = __import__("spoofer.server", fromlist=["app"]).app.state.s.screen.sim_inputs
    assert ("text", "hi") in sent and ("key", 227, True) in sent and sent[-1][:2] == ("touch", "up")
    bad = call(rpc, "tap", x=2, y=0)
    assert bad["isError"] and "fraction" in bad["content"][0]["text"]
    assert call(rpc, "screenshot")["isError"]  # the simulator has no screen
    assert json.loads(call(rpc, "iphone_status")["content"][0]["text"])["screen"]["simulated"] is True


def test_location_tools(rpc):
    call(rpc, "connect_device", simulator=True)
    assert not call(rpc, "set_location", lat=48.8584, lon=2.2945).get("isError")
    assert not call(rpc, "clear_location").get("isError")
