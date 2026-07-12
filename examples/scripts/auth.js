// Per-VU auth for examples/36-auth-tokens.yaml.
//
// Every VU keeps its own short-lived token in `session.vars.auth`. The
// beforeRequest hook fires ahead of each YAML `request:` step: if the VU has no
// token, or its token is about to expire, it mints a fresh one and bumps the
// `token_refreshes` counter. In a real test, replace mint() with a call to your
// identity provider (it can use the synchronous `http` client from loadr/http).

const TTL_MS = 60_000; // token lifetime; refresh a little before this elapses
const SKEW_MS = 5_000; // refresh this long before actual expiry

// Synthesize a token. Swap for: http.post('/oauth/token', ...) and read r.json().
function mint() {
  const now = Date.now();
  return { token: `vu${session.vu}-${now}`, expiresAt: now + TTL_MS };
}

// Fired before every YAML `request:` — ensure a fresh bearer token is attached.
export function beforeRequest(req) {
  let auth = session.vars.auth;
  if (!auth || Date.now() >= auth.expiresAt - SKEW_MS) {
    auth = mint();
    session.vars.auth = auth;
    session.counterAdd('token_refreshes', 1);
  }
  req.headers = req.headers || {};
  req.headers['Authorization'] = `Bearer ${auth.token}`;
  return req;
}
