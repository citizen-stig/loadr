// Per-VU lifecycle hooks + a custom end-of-run report for
// examples/22-lifecycle.yaml.
import http from 'k6/http';
import { check } from 'k6';

// Runs ONCE per VU, before that VU's first iteration (scenario `on_start`).
// Use it for per-user setup such as logging in once per simulated user.
export function login(data) {
  const res = http.post('/auth/login', JSON.stringify({
    user: `vu-${__VU}`,
    secret: __ENV.LOGIN_SECRET || 'demo',
  }), { headers: { 'Content-Type': 'application/json' } });
  check(res, { 'logged in': (r) => r.status === 200 });
  // Stash per-VU state for the iteration body to reuse.
  session.vars.token = res.json() ? res.json().token : 'demo-token';
}

// Runs once per iteration for each VU (scenario `exec`).
export function browse(data) {
  http.get('/feed', {
    headers: { Authorization: `Bearer ${session.vars.token}` },
    tags: { endpoint: 'feed' },
  });
}

// Runs ONCE per VU, when that VU retires (scenario `on_stop`).
// Use it for per-user cleanup such as logging out.
export function logout(data) {
  http.post('/auth/logout', JSON.stringify({ token: session.vars.token }), {
    headers: { 'Content-Type': 'application/json' },
  });
}

// Runs ONCE after teardown(). Returning a string replaces the default
// console summary (the k6 handleSummary equivalent).
export function handleSummary(data) {
  const reqs = data.metrics.find((m) => m.metric === 'http_reqs');
  const dur = data.metrics.find((m) => m.metric === 'http_req_duration');
  return [
    '',
    `  run ${data.run_id} — ${data.duration_secs.toFixed(1)}s over ${data.scenarios.length} scenario(s)`,
    `  total requests: ${reqs ? reqs.agg.sum : 0}`,
    `  p95 latency:    ${dur ? dur.agg.p95.toFixed(1) : 0} ms`,
    `  thresholds:     ${data.thresholds_passed ? 'PASS' : 'FAIL'}`,
    '',
  ].join('\n');
}
