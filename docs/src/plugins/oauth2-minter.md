# OAuth2 minter plugin

> **Status:** planned — not yet in the signed [plugin index](installing.md). This
> page documents the intended shape; the config keys and metric names may still
> change before the first release.

`loadr-plugin-oauth2-minter` is a **service plugin** (`kind = "service"`, role:
*auth & signers*). It runs an OAuth2 **client-credentials** (or **refresh-token**)
grant against a token endpoint once, holds the resulting **bearer token** in the
controller, and hands the same live token to every VU. A background task
**refreshes the token before it expires**, so the fleet always attaches a valid
`Authorization` header without any VU ever making its own auth round-trip.

The alternative — each VU minting and refreshing its own token (see
`examples/36-auth-tokens.yaml`, which does exactly that with a `js:` hook) — means
`vus` extra token requests on your auth server and a fresh grant on every worker.
This plugin does the grant **once, centrally**, and shares the result: a token
provider, not a per-request cost. At high VU counts that is the difference between
one token request every few minutes and thousands of them competing with the load
you actually want to measure.

It is **pure HTTP over [hyper](https://github.com/hyperium/hyper)** — loadr's own
HTTP stack — so there is no OAuth SDK and no extra C dependency. It speaks the
token endpoint directly: a `POST` with the grant form, parse the JSON response,
schedule the next refresh from `expires_in`.

The service lifecycle it uses is the native `FfiService` contract
(`start(config_json) → stop()`) documented in
[Native plugins](native.md#the-interface).

## Install

Once published, `oauth2-minter` will resolve from the signed
[plugin index](installing.md) by name — no build toolchain required:

```bash
loadr plugin install oauth2-minter
loadr plugin info oauth2-minter
```

This picks the artifact for your host target, checks it against the plugin ABI
your `loadr` build provides, verifies its sha256 and unpacks it into your plugins
directory (`~/.loadr/plugins/oauth2-minter/`, or `$LOADR_PLUGINS_DIR`).

The installed manifest declares a native service plugin:

```toml
[plugin]
name = "oauth2-minter"
version = "0.1.0"
kind = "service"
type = "native"
entry = "libloadr_plugin_oauth2_minter.so"
description = "Mints and auto-refreshes an OAuth2 bearer token shared by every VU"
```

To run straight from a build tree instead, point the plan's `plugins:` entry at
the built artifact
(`path: target/release/libloadr_plugin_oauth2_minter.so`) rather than resolving
it by name.

## Use it in a test

List the plugin under `plugins:`, then declare it under the plan's `services:`
block. `type: plugin` routes the step to a service plugin and `service:` names it;
the `config:` block carries the token endpoint and credentials. The service starts
once at the beginning of the run, performs the initial grant, and exposes the
current token as `${services.<name>.token}` — reference it from any request's
`Authorization` header:

```yaml
plugins:
  - name: oauth2-minter            # or: { name: oauth2-minter, path: target/release/libloadr_plugin_oauth2_minter.so }

services:
  auth:
    type: plugin
    service: oauth2-minter
    config:
      token_url: https://id.example.com/oauth2/token
      client_id: ${env.OAUTH_CLIENT_ID}
      client_secret: ${env.OAUTH_CLIENT_SECRET}
      scope: api.read api.write      # optional
      # grant_type: client_credentials  # the default; use refresh_token for a refresh grant

defaults:
  http:
    base_url: https://api.example.com
    headers:
      Authorization: "Bearer ${services.auth.token}"   # every request rides the shared token

scenarios:
  authed_traffic:
    executor: constant-vus
    vus: 200
    duration: 10m
    flow:
      - request: { name: profile, url: /me,     checks: [ { type: status, equals: 200 } ] }
      - request: { name: orders,  url: /orders, checks: [ { type: status, equals: 200 } ] }

thresholds:
  http_req_failed:   [ "rate<0.01" ]
  http_req_duration: [ "p(95)<400ms" ]
```

Because the header is set once in `defaults.http.headers`, all 200 VUs share the
same token. When it nears expiry the background task swaps in a fresh one and the
next request transparently picks it up — no VU stalls, and there is exactly one
grant in flight at a time.

## Config reference

Config is the JSON object under the service's `config:` key. It is handed to the
service's `start()` verbatim at run start
(`{"token_url":"…","client_id":"…","client_secret":"…"}`).

| Key             | Type   | Default              | Meaning |
|-----------------|--------|----------------------|---------|
| `token_url`     | string | *(required)*         | The OAuth2 token endpoint the grant is `POST`ed to. Must be `http(s)://…`; a missing or malformed URL fails `start()` so the plan is rejected before the run rather than failing mid-load. |
| `client_id`     | string | *(required)*         | OAuth2 client identifier sent in the grant. |
| `client_secret` | string | *(required)*         | OAuth2 client secret. Pull it from `${env.…}` / `${secrets.…}` — never inline it in a committed plan. |
| `grant_type`    | string | `client_credentials` | The grant to run: `client_credentials` (the default) or `refresh_token`. |
| `refresh_token` | string | —                    | Required when `grant_type: refresh_token`; the refresh token exchanged for an access token. |
| `scope`         | string | —                    | Space-separated OAuth2 scopes to request. Omit to take the endpoint's default. |
| `audience`      | string | —                    | Optional `audience` parameter for endpoints that require it (e.g. Auth0). |
| `auth_style`    | string | `body`               | How credentials are presented: `body` (form fields) or `basic` (HTTP Basic `Authorization` header). |
| `refresh_skew`  | duration | `30s`              | Refresh this long **before** the token's `expires_in`, so a token never expires between the refresh and the requests using it. |

`${env.…}` / `${secrets.…}` and other interpolation resolve before the config
reaches the plugin, so credentials stay out of the plan file. The minted token is
**redacted from logs and exports**.

The token endpoint's JSON response is expected to carry `access_token` and
`expires_in`; the plugin schedules the next refresh at `expires_in − refresh_skew`.
An endpoint that omits `expires_in` is refreshed on a conservative default
interval.

## Metrics

The plugin reports its own health back into the run, so an auth problem is visible
in loadr's summary rather than silently attaching a stale token:

| Metric                   | Kind    | Meaning |
|--------------------------|---------|---------|
| `oauth2_token_refreshes` | counter | One per successful token grant — the initial mint plus every pre-expiry refresh. |
| `oauth2_refresh_errors`  | counter | One per failed grant attempt: a connection error, a timeout, or a non-2xx / unparseable token response. |

A healthy run shows `oauth2_token_refreshes` ticking up slowly (once per token
lifetime) and `oauth2_refresh_errors` at zero. A climbing `oauth2_refresh_errors`
usually means bad credentials, a wrong `token_url`, or blocked egress to the
identity provider — gate on it so a broken mint fails the run instead of driving
load with an expired token:

```yaml
thresholds:
  oauth2_refresh_errors: [ "count==0" ]
  oauth2_token_refreshes: [ "count>0" ]
```

## Notes

- **One grant, shared by every VU.** The whole point of the plugin: the token
  lives in the controller, not in each VU. This removes the per-VU auth
  round-trip you get from a `js:` `beforeRequest` mint (as in
  `examples/36-auth-tokens.yaml`), so your auth server sees one grant per token
  lifetime instead of one per VU.
- **Refreshes before expiry.** The background task refreshes at
  `expires_in − refresh_skew`, so the shared token is swapped out ahead of time
  and no request ever rides an expired token. Increase `refresh_skew` if your
  requests can be slow enough to straddle the expiry boundary.
- **Keep secrets in the environment.** `client_secret` (and `refresh_token`) are
  credentials — pass them via `${env.…}` or `${secrets.…}`, never hard-coded in
  the plan. The minted access token is redacted from logs and exports.
- **Fails fast at startup.** A bad `token_url` or missing credential fails
  `start()` before any VU begins, so a misconfigured grant is caught up front
  rather than surfacing as a wall of 401s once load is running.
- **Distributed runs.** The mint happens once, on the controller, and the token
  is distributed to the workers — so there is a single grant against the identity
  provider for the whole fleet, not one per worker.
- **In-process service.** Native service plugins run in-process with full
  privileges (see [Native plugins](native.md#safety-notes)); this one does no I/O
  beyond the token endpoint calls.
