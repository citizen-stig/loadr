#!/usr/bin/env python3
"""Real-protocol echo/stream server for the loadr example harness.
  WS  :8081  -> replies {"type":"ack"} to every frame
  SSE :8082  -> streams a few `data:{...status...}` events then `event: done`
  TCP :7000  -> replies "PONG\n"
  UDP :8125  -> echoes the datagram back
These use real protocol stacks (websockets / asyncio sockets / HTTP streaming);
only the application replies are canned, exactly as your own service would be."""
import asyncio, threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

class SSE(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def log_message(self, *a): pass
    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.end_headers()
        try:
            for i in range(5):
                self.wfile.write(f'event: update\ndata: {{"status":"ok","seq":{i}}}\n\n'.encode())
                self.wfile.flush()
            self.wfile.write(b'event: done\ndata: {"status":"done"}\n\n')
            self.wfile.flush()
        except Exception:
            pass

def serve_sse():
    ThreadingHTTPServer(("0.0.0.0", 8082), SSE).serve_forever()

async def ws_main():
    import websockets
    async def handler(ws):
        try:
            async for _ in ws:
                await ws.send('{"type":"ack","ok":true}')
        except Exception:
            pass
    def select_subprotocol(conn, offered):
        return offered[0] if offered else None
    async with websockets.serve(handler, "0.0.0.0", 8081, select_subprotocol=select_subprotocol):
        await asyncio.Future()

async def tcp_main():
    async def handle(r, w):
        try:
            await r.read(1024)
            w.write(b"PONG\n")
            await w.drain()
        except Exception:
            pass
        finally:
            try: w.close()
            except Exception: pass
    s = await asyncio.start_server(handle, "0.0.0.0", 7000)
    async with s:
        await s.serve_forever()

class UDPEcho(asyncio.DatagramProtocol):
    def connection_made(self, t): self.t = t
    def datagram_received(self, data, addr): self.t.sendto(data or b"ok", addr)

async def udp_main():
    loop = asyncio.get_running_loop()
    await loop.create_datagram_endpoint(UDPEcho, local_addr=("0.0.0.0", 8125))
    await asyncio.Future()

def main():
    threading.Thread(target=serve_sse, daemon=True).start()
    print("echo: WS:8081 SSE:8082 TCP:7000 UDP:8125 — ready", flush=True)
    async def run_all():
        await asyncio.gather(ws_main(), tcp_main(), udp_main())
    asyncio.run(run_all())

if __name__ == "__main__":
    main()
