"""Model Context Protocol for AI agents (Claude, Cursor, ...): ``POST /mcp``, and ``--mcp`` over stdio.

Same tools and behaviour as the standalone app (standalone/src/server.rs, ``mcp``): both read the
list from ui/mcp-tools.json. Each tool is one or more calls to this app's own API, made in-process;
``{key}`` in a path comes from the arguments or the previous answer.
"""

from __future__ import annotations

import asyncio
import base64
import json
import sys
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any, Optional
from urllib.parse import quote

import httpx

from . import __version__

VERSIONS = ("2025-06-18", "2025-03-26", "2024-11-05")
_spec: Optional[dict] = None


def spec(static: Path) -> dict:
    global _spec
    if _spec is None:
        _spec = json.loads((static / "mcp-tools.json").read_text(encoding="utf-8"))
    return _spec


def _reply(mid: Any, result: Any) -> dict:
    return {"jsonrpc": "2.0", "id": mid, "result": result}


def _error(mid: Any, code: int, message: str) -> dict:
    return {"jsonrpc": "2.0", "id": mid, "error": {"code": code, "message": message}}


async def handle(msg: Any, app: Any, static: Path) -> Optional[dict]:
    """One JSON-RPC message -> the answer, or None for a notification."""
    if not isinstance(msg, dict):
        return _error(None, -32600, "Send one JSON-RPC message at a time")
    if "id" not in msg:
        return None
    mid, method, params = msg["id"], msg.get("method"), msg.get("params") or {}
    s = spec(static)
    if method == "initialize":
        asked = params.get("protocolVersion")
        return _reply(mid, {"protocolVersion": asked if asked in VERSIONS else VERSIONS[0], "capabilities": {"tools": {}},
                            "instructions": s["instructions"], "serverInfo": {"name": "idevbench", "version": __version__}})
    if method == "ping":
        return _reply(mid, {})
    if method == "tools/list":
        return _reply(mid, {"tools": [{k: t[k] for k in ("name", "description", "inputSchema")} for t in s["tools"]]})
    if method == "tools/call":
        tool = next((t for t in s["tools"] if t["name"] == params.get("name")), None)
        if tool is None:
            return _error(mid, -32602, f"Unknown tool '{params.get('name')}'")
        args = params.get("arguments") if isinstance(params.get("arguments"), dict) else {}
        try:
            return _reply(mid, {"content": await run(tool, dict(args), app)})
        except _ToolError as e:
            return _reply(mid, {"content": [_text(str(e))], "isError": True})
    return _error(mid, -32601, f"Method not found: {method}")


class _ToolError(Exception):
    pass


def _text(v: Any) -> dict:
    return {"type": "text", "text": v if isinstance(v, str) else json.dumps(v, indent=2, ensure_ascii=False)}


async def run(tool: dict, args: dict, app: Any) -> list:
    steps = tool["call"]
    if steps[0][0] == "WAIT":
        ms = min(int(args.get("ms") or 1000), 10000)
        await asyncio.sleep(ms / 1000)
        return [_text(f"Waited {ms} ms.")]
    answers: list = []
    transport = httpx.ASGITransport(app=app)
    async with httpx.AsyncClient(transport=transport, base_url="http://127.0.0.1", timeout=120) as client:
        for i, step in enumerate(steps):
            method, path = step[0], step[1]
            while "{" in path:
                a, b = path.index("{"), path.index("}")
                key = path[a + 1 : b]
                v = args.pop(key, None) if i == 0 else None
                if v is None and answers and isinstance(answers[-1], dict):
                    v = answers[-1].get(key)
                if v is None:
                    raise _ToolError(f"'{key}' is missing")
                path = path[:a] + quote(str(v), safe="") + path[b + 1 :]
            # The tool's arguments go to the first call: as the query of a GET, else merged into its body.
            body = dict(step[2]) if len(step) > 2 else {}
            if i == 0 and method == "GET":
                r = await client.get(path, params={k: v if isinstance(v, str) else json.dumps(v) for k, v in args.items()})
            else:
                if i == 0:
                    body.update(args)
                r = await client.request(method, path, json=body)
            kind = r.headers.get("content-type", "")
            if kind.startswith("image/") and r.is_success:
                return [{"type": "image", "data": base64.b64encode(r.content).decode(), "mimeType": kind}]
            try:
                v = r.json()
            except ValueError:
                v = r.text
            if not r.is_success:
                raise _ToolError(v.get("detail") if isinstance(v, dict) and isinstance(v.get("detail"), str) else f"HTTP {r.status_code}: {v}")
            answers.append(v)
    returns = tool.get("returns")
    out = {k: answers[returns[0]].get(k) for k in returns[1]} if returns else answers[-1]
    return [_text(out)]


def bridge(port: Optional[int]) -> None:
    """``--mcp``: for AI apps that start MCP servers as programs (Claude Desktop and others). Passes each
    message from stdin to the running app's /mcp and prints the answer; the app must be open."""
    urls = [f"http://127.0.0.1:{port}/mcp"] if port else ["http://127.0.0.1:8766/mcp", "http://127.0.0.1:8765/mcp"]
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    for line in sys.stdin:
        if not line.strip():
            continue
        answer = None
        for url in urls:
            req = urllib.request.Request(url, data=line.encode(), method="POST",
                                         headers={"Content-Type": "application/json", "Accept": "application/json, text/event-stream"})
            try:
                with opener.open(req, timeout=300) as r:
                    answer = r.read().decode()
                break
            except urllib.error.HTTPError as e:
                answer = e.read().decode()
                break
            except OSError:
                continue
        if answer is None:
            try:
                msg = json.loads(line)
            except ValueError:
                msg = None
            if isinstance(msg, dict) and "id" in msg:
                answer = json.dumps(_error(msg["id"], -32000, "iDevBench is not running. Open it, then try again."))
        if answer and answer.strip():
            sys.stdout.write(answer.strip() + "\n")
            sys.stdout.flush()
