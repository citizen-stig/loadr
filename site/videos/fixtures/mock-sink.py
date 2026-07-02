from http.server import BaseHTTPRequestHandler, HTTPServer
import json
class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def _ok(self, code=200, body=b'{"ok":true}'):
        self.send_response(code); self.send_header('Content-Type','application/json')
        self.send_header('Content-Length', str(len(body))); self.end_headers(); self.wfile.write(body)
    def do_POST(self):
        self.rfile.read(int(self.headers.get('Content-Length') or 0))
        if 'oauth' in self.path: self._ok(200, json.dumps({"access_token":"demo-token-abc123","token_type":"Bearer","expires_in":3600}).encode())
        elif 'series' in self.path: self._ok(202, b'{}')
        else: self._ok()
    def do_GET(self): self._ok()
    def do_PUT(self): self.rfile.read(int(self.headers.get('Content-Length') or 0)); self._ok()
HTTPServer(('127.0.0.1', 9999), H).serve_forever()
