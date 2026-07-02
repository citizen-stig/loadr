# jwt-decode plugin

> **Status:** planned — this plugin is not in the signed index yet. The shape
> below describes the intended extractor contract; treat it as a design note
> until it ships.

`loadr-plugin-jwt-decode` is an **extractor plugin** (`kind = "extractor"`,
role: *Extractors*). It locates a JWT on a response — in a header, a JSON body
field, or a cookie — **base64url-decodes the payload segment** and returns a
single named claim as a string, ready to correlate into the next request with
`${...}`.

It is **pure Rust** built on `base64` and `serde_json`: it only *decodes* the
token, it does not *verify* it. There is no signature check, no key handling and
no crypto dependency — decoding a JWT payload needs none of that. If you need a
value out of a token for chaining (a `sub`, a tenant id, a session handle), this
does it without pulling a JWT library into the run.

Like every extractor it runs as a sandboxed **WASM component** (wasmtime, no
filesystem, no network — see the [plugin overview](overview.md#two-mechanisms))
against the WIT `extractor` interface, which the engine drives through the
`PluginExtractor` contract:

```wit
interface extractor {
  /// body + headers + the plugin's JSON config -> extracted value (or none)
  extract: func(body: list<u8>, headers: list<tuple<string,string>>, config: string) -> option<string>;
}
```

Because the plugin receives both the response **body** and its **headers**, it
can find the token wherever it lives — an `Authorization` header, a `Set-Cookie`
header, or a field in a JSON body.

## Install

Once published, `jwt-decode` will resolve from the signed
[plugin index](installing.md) by name — no build toolchain required:

```bash
loadr plugin install jwt-decode
loadr plugin info jwt-decode
```

This picks the artifact for your host target, checks it against the plugin ABI
your `loadr` build provides, verifies its sha256 and unpacks it into your
plugins directory (`~/.loadr/plugins/jwt-decode/`, or `$LOADR_PLUGINS_DIR`).

The installed manifest declares a WASM extractor plugin:

```toml
[plugin]
name = "jwt-decode"
version = "0.1.0"
kind = "extractor"
type = "wasm"
entry = "jwt_decode.wasm"
description = "Decode a JWT payload and extract a named claim for correlation"
```

To run straight from a build tree instead, point the plan's `plugins:` entry at
the built component (`path: target/wasm32-wasip2/release/jwt_decode.wasm`)
rather than resolving it by name.

## Use it in a test

List the plugin under `plugins:`, then reference it from a request's `extract:`
block with `type: plugin` and `plugin: jwt-decode`. The extracted claim lands in
the named variable and interpolates into later steps as `${...}`:

```yaml
plugins:
  - name: jwt-decode                     # or: { name: jwt-decode, path: target/wasm32-wasip2/release/jwt_decode.wasm }
    config:
      source: "header:authorization"     # where the token lives (default for every call)

scenarios:
  api:
    executor: constant-vus
    vus: 20
    duration: 1m
    flow:
      - request:
          name: login
          method: POST
          url: https://api.example.com/login
          body:
            json: { user: "u${vu}", pass: "secret" }
          extract:
            # Pull `sub` out of the bearer token the login handed back.
            - { type: plugin, name: user_id, plugin: jwt-decode, config: { claim: sub } }
          checks:
            - { type: status, equals: 200 }

      - request:
          name: whoami
          url: https://api.example.com/users/${user_id}
          headers:
            Authorization: "Bearer ${access_token}"
          checks:
            - { type: jsonpath, name: id echoes, expression: "$.id", equals: "${user_id}" }
```

The per-use `config:` on an `extract:` entry is **merged over** the defaults from
the `plugins:` entry, so a fixed `source:` can live on the plugin while each
extraction names the `claim:` it wants. A token that is missing, malformed, or
lacks the requested claim yields **no match** (the extractor returns `none`), so
the entry misses like any other extractor rather than failing the run — supply a
`default:` on the entry if you want a fallback value.

## Config reference

Config is the JSON object handed to the extractor per call (manifest / `plugins:`
defaults merged with the per-use `config:` on the `extract:` entry).

| Key      | Type   | Default              | Meaning |
|----------|--------|----------------------|---------|
| `source` | string | `header:authorization` | Where to find the JWT. See the source grammar below. |
| `claim`  | string | *(required)*         | Name of the payload claim to return, e.g. `sub`, `tid`, `email`. Dotted paths (`a.b`) index into nested claim objects. |

### `source` grammar

`source` is `"<location>:<name>"`:

| Form                  | Reads from | Notes |
|-----------------------|------------|-------|
| `header:<name>`       | a response header | Case-insensitive. A leading `Bearer ` (or `bearer `) prefix is stripped before decoding. |
| `cookie:<name>`       | a `Set-Cookie` header | The named cookie's value is the token. |
| `json:<path>`         | the JSON body | Dotted path to the string field holding the token, e.g. `json:data.token`. |

The token is split on `.`; only the **payload** (second) segment is
base64url-decoded (URL-safe alphabet, padding optional) and parsed as JSON. The
header and signature segments are ignored — again, **no verification** happens.

## Metrics

None. An extractor is a pure function over a response it is already given; it
issues no requests and emits no metric family of its own. Its effect shows up in
the metrics of the requests it feeds — a decoded claim that fails to correlate
surfaces as a `checks` failure or a bad status on the *next* request, not as a
counter here.

## Notes

- **Decode, not verify.** This extractor never checks a signature and pulls in
  no crypto. If a run must *reject* invalid tokens, assert on the protected
  endpoint's response instead — decoding a payload is a correlation step, not a
  security control.
- **Misses are soft.** A missing header/field, a value that is not a JWT, an
  undecodable payload, or an absent claim all return `none`, so the entry misses
  quietly; pair it with a `default:` when you need a placeholder to continue.
- **Sandboxed.** As a WASM component it runs with no filesystem and no network;
  it only ever sees the body, headers and config the engine passes in, and the
  worst a broken build can do is waste CPU (see [WASM plugins](wasm.md)).
- **String out.** The claim is returned as a string for `${...}` interpolation;
  a numeric or boolean claim is rendered as its JSON text (`42`, `true`), and an
  object/array claim as its compact JSON.
