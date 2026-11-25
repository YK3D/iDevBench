"""The MCP endpoint for AI agents (/mcp, tools from ui/mcp-tools.json), driving the simulator (same cases as the standalone)."""

import json

import pytest
from fastapi.testclient import TestClient


@pytest.fixture()
def rpc(tmp_path, monkeypatch):
    monkeypatch.setenv("SPOOFER_DATA_DIR", str(tmp_path))
    from spoofer import server
    from spoofer.store import Store

    server.app.state.s = server.AppState()
    server.app.state.s.store = Store(tmp_path / "data.json")
    with TestClient(server.app, base_url="http://127.0.0.1:8765") as c:
        def send(method, params=None):
            r = c.post("/mcp", json={"jsonrpc": "2.0", "id": 1, "method": method, "params": params or {}})
            return r.json()

        send.client = c
        yield send


def call(rpc, tool, **args):
    return rpc("tools/call", {"name": tool, "arguments": args})["result"]


def test_handshake_and_tool_list(rpc):
    init = rpc("initialize", {"protocolVersion": "2025-03-26"})["result"]
    assert (init["protocolVersion"], init["serverInfo"]["name"]) == ("2025-03-26", "idevbench")
    tools = {t["name"]: t for t in rpc("tools/list")["result"]["tools"]}
    assert len(tools) >= 30 and "call" not in tools["tap"] and tools["tap"]["inputSchema"]["required"] == ["x", "y"]
    note = rpc.client.post("/mcp", json={"jsonrpc": "2.0", "method": "notifications/initialized"})
    assert note.status_code == 202
    assert rpc("nope")["error"]["code"] == -32601
    assert rpc.client.post("/mcp", json={"jsonrpc": "2.0", "id": 1}, headers={"origin": "https://evil.example"}).status_code == 403


def test_drives_location_and_mirroring(rpc):
    from spoofer import server

    assert call(rpc, "tap", x=0.5, y=0.5)["isError"]  # not mirroring yet
    for tool, args in [("connect_device", {"driver": "mock"}), ("set_location", {"lat": 48.8584, "lon": 2.2945}),
                       ("start_mirroring", {"driver": "mock"}), ("tap", {"x": 0.5, "y": 0.9}), ("gesture", {"name": "app_switcher"}),
                       ("press_key", {"usage": 4, "command": True}), ("phone_performance", {}), ("wait", {"ms": 1})]:
        r = call(rpc, tool, **args)
        assert not r.get("isError"), (tool, r)
    assert ("gesture", "app_switcher") in server.app.state.s.screen.sim_inputs
    route = call(rpc, "drive_route", waypoints=[{"lat": 51.5, "lon": -0.12}, {"lat": 51.501, "lon": -0.12}], legs=[{"mode": "straight"}])
    out = json.loads(route["content"][0]["text"])
    assert isinstance(out["id"], str) and isinstance(out["summary"], dict), route
    assert server.app.state.s.player.status()["state"] == "playing"
    assert "settings" not in call(rpc, "list_favourites")["content"][0]["text"]
    assert call(rpc, "screenshot")["isError"]  # the simulator has no screen
    bad = call(rpc, "tap", x=2, y=0)
    assert bad["isError"] and "0 to 1" in bad["content"][0]["text"]
