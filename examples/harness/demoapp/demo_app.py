#!/usr/bin/env python3
"""Real HTTP demo-app backend for the loadr examples.

A normal HTTP server (real protocol) that implements the application endpoints
the examples assert against — logins that round-trip a token, carts that return
201, listings with an `items` array, a checkout/order flow, GraphQL, etc. This
is the "application under test"; only it is example-specific (Redis, gRPC, WS,
SSE are the real products/stacks)."""
import base64, json, re
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

def tok(user): return "tok-" + base64.urlsafe_b64encode(user.encode()).decode().rstrip("=")
def user_from(auth):
    if not auth: return "demo"
    t = auth.replace("Bearer ", "").strip()
    if t.startswith("tok-"):
        s = t[4:]; s += "=" * (-len(s) % 4)
        try: return base64.urlsafe_b64decode(s).decode()
        except Exception: return "demo"
    return "demo"

def parse_form(body):
    out = {}
    for pair in body.split("&"):
        if "=" in pair:
            k, v = pair.split("=", 1)
            out[k] = v.replace("+", " ")
    return out

ITEMS = [{"id": 1, "name": "alpha", "price": 9}, {"id": 2, "name": "beta", "price": 19}]

class App(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def log_message(self, *a): pass

    def _json(self, code, obj, extra=None):
        b = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(b)))
        self.send_header("X-Request-Id", "req-123")
        self.send_header("ETag", '"abc123"')
        self.send_header("Cache-Control", "max-age=60")
        for k, v in (extra or []): self.send_header(k, v)
        self.end_headers()
        if self.command != "HEAD": self.wfile.write(b)

    def _html(self, body):
        b = body.encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/html")
        self.send_header("Content-Length", str(len(b)))
        self.send_header("X-Request-Id", "req-123")
        self.end_headers()
        if self.command != "HEAD": self.wfile.write(b)

    def _body(self):
        n = int(self.headers.get("Content-Length") or 0)
        return self.rfile.read(n).decode("utf-8", "replace") if n else ""

    def do_GET(self):
        p = self.path.split("?")[0]
        if "checkout/start" in p:
            return self._html('<!doctype html><html><body>'
                              '<form><input name="csrf" value="csrf-xyz"></form>'
                              '<div data-trace="trace-123">x</div></body></html>')
        if p.startswith("/orders/"):
            return self._json(200, {"order": {"id": p.rsplit("/", 1)[-1]}, "status": "pending"})
        if p == "/me":
            return self._json(200, {"username": user_from(self.headers.get("Authorization")), "id": 1})
        if "items" in p or "inventory" in p or p == "/feed" or p == "/":
            return self._json(200, {"items": ITEMS, "results": ITEMS, "count": len(ITEMS), "status": "ok"})
        return self._json(200, {"items": ITEMS, "id": 1, "token": "tok-demo", "status": "ok", "value": 42})
    def do_HEAD(self): self.do_GET()

    def do_POST(self):
        p = self.path.split("?")[0]
        body = self._body()
        if "checkout/submit" in p:
            return self._json(200, {"order": {"id": "ord-1"}})
        if p == "/orders" or p.endswith("/orders"):
            return self._json(201, {"order": {"id": "ord-1"}, "status": "PENDING"})
        if p == "/cart" or p.endswith("/cart"):
            return self._json(201, {"id": "cart-1", "sku": "W-1"})
        if "login" in p:
            form = parse_form(body)
            user = form.get("username")
            if not user:
                try: user = (json.loads(body) or {}).get("user") or (json.loads(body) or {}).get("username")
                except Exception: user = None
            user = user or "demo"
            return self._json(200, {"token": tok(user), "username": user})
        if "auth/token" in p:
            return self._json(200, {"token": "tok-demo", "access_token": "tok-demo"})
        if "graphql" in p or "query" in body:
            return self._json(200, {"data": {
                "products": {"totalCount": len(ITEMS), "items": ITEMS},
                "items": ITEMS, "user": {"id": 1, "name": "demo"}}})
        if "export" in p or "auth/" in p:
            return self._json(200, {"ok": True})
        return self._json(201, {"id": "obj-1", "data": {"items": ITEMS}})
    def do_PUT(self): self.do_POST()
    def do_DELETE(self): self._json(200, {"deleted": True})

if __name__ == "__main__":
    print("demo-app on 8085", flush=True)
    ThreadingHTTPServer(("0.0.0.0", 8085), App).serve_forever()
