// Helpers for examples/08-javascript.yaml — a loadr scripting module.
import http from 'loadr/http';
import { check, sleep, group } from 'loadr';
import { Trend } from 'loadr/metrics';

const apiLatency = new Trend('api_latency_custom');

// Runs ONCE before any VU starts. The return value is shared with every VU.
export function setup() {
  const res = http.post('/auth/token', JSON.stringify({
    client_id: __ENV.CLIENT_ID || 'demo',
    client_secret: __ENV.CLIENT_SECRET || 'demo-secret',
  }), { headers: { 'Content-Type': 'application/json' } });
  check(res, { 'token issued': (r) => r.status === 200 });
  return { token: res.json() ? res.json().token : 'demo-token' };
}

// Runs once per iteration for the `scripted` scenario (see `exec:`).
export function scriptedIteration(data) {
  group('catalogue', () => {
    const res = http.get('/items?limit=20', {
      headers: { Authorization: `Bearer ${data.token}` },
      tags: { endpoint: 'items' },
    });
    apiLatency.add(res.duration_ms);
    check(res, {
      'status 200': (r) => r.status === 200,
      'has items': (r) => { const b = r.json(); return b && Array.isArray(b.items); },
    });
  });
  sleep(Math.random() * 2 + 0.5);
}

// Fired before every YAML `request:` step — mutate and return the request.
export function beforeRequest(req) {
  req.headers = req.headers || {};
  req.headers['X-Correlation-Id'] = crypto.uuidv4();
  return req;
}

// Fired after every YAML `request:` step.
export function afterRequest(res) {
  if (res.status >= 500) {
    console.warn(`server error on ${res.url}: ${res.status}`);
  }
}

// Runs ONCE after the test ends.
export function teardown(data) {
  http.post('/auth/revoke', JSON.stringify({ token: data.token }), {
    headers: { 'Content-Type': 'application/json' },
  });
}
