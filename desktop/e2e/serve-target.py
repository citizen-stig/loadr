#!/usr/bin/env python3
"""The demo target: one local process every example walkthrough runs against.

Serves, localhost-only:
  * HTTP  — routes derived from the plans' own contracts (routes.json from
            gen-routes.py); anything unrouted gets a friendly 200 JSON echo
  * SSE   — text/event-stream on the routes marked kind=sse (an event every
            300 ms carrying a "status", `event: done` after 10)
  * WS    — replies {"type":"ack"} to every message and pushes periodic chat
            messages so receive_count-style sessions fill up
  * TCP   — line echo, PONG-prefixed (the raw-sockets demo asserts PONG)
  * UDP   — datagram echo

    python3 serve-target.py routes.json --http 9801 --ws 9802 --tcp 9803 --udp 9804
"""
import asyncio
import json
import re
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def load_routes(path):
    routes = json.load(open(path))
    return [r for r in routes if r.get("kind") != "sse"], [r for r in routes if r.get("kind") == "sse"]


def path_match(pattern: str, actual: str) -> bool:
    if pattern == actual:
        return True
    rx = "^" + re.escape(pattern).replace(r"\*", "[^/]+") + "$"
    return re.match(rx, actual) is not None


def body_for(route) -> tuple[bytes, str]:
    """Build a body satisfying json fields + contains + html/xml shapes."""
    if route.get("html_inputs") or route.get("boundaries"):
        inputs = "".join(
            f'<input type="hidden" name="{k}" value="{v}">' for k, v in route.get("html_inputs", {}).items()
        )
        bounds = "".join(f"<div>{l}bnd-{i}-xyz{r}</div>" for i, (l, r) in enumerate(route.get("boundaries", [])))
        extra = "".join(f"<p>{c}</p>" for c in route.get("contains", []))
        return (f"<!doctype html><html><body><form>{inputs}</form>{bounds}{extra}</body></html>".encode(),
                "text/html")
    if route.get("xml"):
        elems = "".join(f"<{n}>1.0871</{n}>" for n in dict.fromkeys(route.get("xml_elems", [])))
        extra = "".join(route.get("contains", []))
        return (f'<?xml version="1.0"?><response><status>ok</status><id>42</id>{elems}{extra}</response>'.encode(),
                "application/xml")
    # start from friendly defaults, then overlay the plan-derived contract —
    # extra fields never break a check, missing ones do
    payload = {"ok": True, "id": 42, "status": "ok", "token": "tok-demo-123",
               "items": [{"name": "alpha", "price": 9}, {"name": "beta", "price": 19}], "count": 2}
    payload.update(route.get("json") or {})
    body = json.dumps(payload)
    for c in route.get("contains", []):
        if c not in body:
            payload.setdefault("_notes", []).append(c)
            body = json.dumps(payload)
    return body.encode(), "application/json"


class Handler(BaseHTTPRequestHandler):
    routes: list = []
    sse_paths: list = []
    protocol_version = "HTTP/1.1"

    def log_message(self, *a):  # quiet
        pass

    def _respond(self):
        path = self.path.split("?")[0]
        # httpbin-style semantics several examples rely on
        m = re.match(r"^/status/(\d{3})$", path)
        if m:
            code = int(m.group(1))
            body = json.dumps({"status": code}).encode()
            self.send_response(code)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        m = re.match(r"^/delay/(\d+(?:\.\d+)?)$", path)
        if m:
            time.sleep(min(float(m.group(1)), 10))
            body = json.dumps({"delayed": float(m.group(1))}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        # SSE stream?
        if any(path_match(r["path"], path) for r in self.sse_paths):
            return self._sse()
        for r in self.routes:
            if r["method"] == self.command and path_match(r["path"], path):
                body, ctype = body_for(r)
                self.send_response(r.get("status", 200))
                self.send_header("Content-Type", ctype)
                self.send_header("Content-Length", str(len(body)))
                for k, v in (r.get("headers") or {}).items():
                    self.send_header(k, v)
                self.end_headers()
                self.wfile.write(body)
                return
        # default: friendly echo (covers templated URLs and un-checked steps)
        body = json.dumps({"ok": True, "path": path, "method": self.command,
                           "id": 42, "status": "ok", "token": "tok-demo-123",
                           "items": [{"name": "alpha", "price": 9}, {"name": "beta", "price": 19}],
                           "count": 2}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Set-Cookie", "session=sess-demo-1; Path=/")
        self.send_header("X-Request-Id", "req-abc-123")
        self.end_headers()
        self.wfile.write(body)

    def _sse(self):
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.end_headers()
        try:
            for i in range(10):
                self.wfile.write(f'data: {{"status":"shipped","seq":{i}}}\n\n'.encode())
                self.wfile.flush()
                time.sleep(0.3)
            self.wfile.write(b'event: done\ndata: {"status":"done"}\n\n')
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass

    do_GET = do_POST = do_PUT = do_DELETE = do_PATCH = do_HEAD = do_OPTIONS = _respond

    def handle_one_request(self):
        # read & discard request bodies so keep-alive framing stays intact
        super().handle_one_request()

    def parse_request(self):
        ok = super().parse_request()
        if ok:
            length = int(self.headers.get("Content-Length") or 0)
            if length:
                self.rfile.read(length)
        return ok


async def ws_main(port: int):
    import websockets

    async def handler(conn):
        async def pusher():
            i = 0
            try:
                while True:
                    await asyncio.sleep(0.5)
                    await conn.send(json.dumps({"type": "message", "body": f"chat-{i}", "seq": i}))
                    i += 1
            except Exception:
                pass

        push = asyncio.create_task(pusher())
        try:
            async for msg in conn:
                await conn.send(json.dumps({"type": "ack", "echo": str(msg)[:80]}))
        except Exception:
            pass
        finally:
            push.cancel()

    async with websockets.serve(handler, "127.0.0.1", port, subprotocols=["chat.v2"]):
        await asyncio.Future()


class UdpEcho(asyncio.DatagramProtocol):
    def connection_made(self, transport):
        self.transport = transport

    def datagram_received(self, data, addr):
        self.transport.sendto(data, addr)


async def tcp_udp_main(tcp_port: int, udp_port: int):
    async def tcp_client(reader, writer):
        try:
            while True:
                data = await reader.read(1024)
                if not data:
                    break
                # pad to 64+ bytes: clients doing read_bytes:64 shouldn't stall
                reply = (b"PONG " + data.strip()).ljust(64, b".") + b"\n"
                writer.write(reply)
                await writer.drain()
        except Exception:
            pass
        finally:
            writer.close()

    server = await asyncio.start_server(tcp_client, "127.0.0.1", tcp_port)
    loop = asyncio.get_running_loop()
    await loop.create_datagram_endpoint(UdpEcho, local_addr=("127.0.0.1", udp_port))
    async with server:
        await server.serve_forever()


def main():
    argv = sys.argv[1:]

    def opt(name, default):
        return int(argv[argv.index(name) + 1]) if name in argv else default

    http_port, ws_port = opt("--http", 9801), opt("--ws", 9802)
    tcp_port, udp_port = opt("--tcp", 9803), opt("--udp", 9804)
    Handler.routes, Handler.sse_paths = load_routes(argv[0])

    httpd = ThreadingHTTPServer(("127.0.0.1", http_port), Handler)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    print(f"demo target: http/sse :{http_port}  ws :{ws_port}  tcp :{tcp_port}  udp :{udp_port} "
          f"({len(Handler.routes)} routes, {len(Handler.sse_paths)} sse)")

    async def run_async():
        await asyncio.gather(ws_main(ws_port), tcp_udp_main(tcp_port, udp_port))

    asyncio.run(run_async())


if __name__ == "__main__":
    main()
