# hmac-signer plugin

> **Status:** planned — this plugin is not in the signed [plugin index](installing.md)
> yet. The shape below describes the intended signer contract; the config keys and
> metric names may still change before the first release.

`loadr-plugin-hmac-signer` is a **service plugin** (`kind = "service"`, role: *auth
& signers*). It is a small in-process **HMAC signer**: for each request it builds a
**canonical string** from a template you supply, computes an HMAC (SHA-256 or
SHA-512) over it with a shared secret, and stamps the result onto the request as a
header. A request that names it as its signer gets that header added just before it
goes out — the pattern most partner and webhook APIs require to prove a request is
authentic.

It is **pure Rust** — the hashing and keyed MAC are done with the `hmac` and `sha2`
crates, with **no OpenSSL and no C dependency** — so it installs by name with no
build toolchain. Where [`aws-sigv4`](aws-sigv4.md) implements one fixed
canonicalization (AWS SigV4) and [`response-signature`](response-signature.md)
*verifies* an inbound signature, `hmac-signer` is the general **outbound** case: you
declare the canonical string and header the partner expects, and it produces the
matching signature on every request.

The service lifecycle it uses is the native `FfiService` contract
(`start(config_json) → stop()`) documented in
[Native plugins](native.md#the-interface); it implements the `ServicePlugin` trait
and is invoked per-request through the signer hook rather than polling on a timer.

[`hmac`]: https://docs.rs/hmac
[`sha2`]: https://docs.rs/sha2

## Install

Once published, `hmac-signer` will resolve from the signed
[plugin index](installing.md) by name — no build toolchain required:

```bash
loadr plugin install hmac-signer
loadr plugin info hmac-signer
```

This picks the artifact for your host target, checks it against the plugin ABI
your `loadr` build provides, verifies its sha256 and unpacks it into your plugins
directory (`~/.loadr/plugins/hmac-signer/`, or `$LOADR_PLUGINS_DIR`).

The installed manifest declares a native service plugin:

```toml
[plugin]
name = "hmac-signer"
version = "0.1.0"
kind = "service"
type = "native"
entry = "libloadr_plugin_hmac_signer.so"
description = "Pure-Rust HMAC (SHA-256/512) request signer over a configurable canonical string"
```

To run straight from a build tree instead, point the plan's `plugins:` entry at
the built artifact (`path: target/release/libloadr_plugin_hmac_signer.so`) rather
than resolving it by name.

## Use it in a test

List the plugin under `plugins:`, then attach it to a request as its signer. A
request's `sign:` block routes the request through a service plugin: `type: plugin`
selects the signer mechanism, `service:` names the plugin, and `config:` carries
the secret, algorithm, header, and canonical-string `template`. loadr renders the
template, computes the HMAC, and stamps the header on the request just before it is
sent.

```yaml
plugins:
  - name: hmac-signer                   # or: { name: hmac-signer, path: target/release/libloadr_plugin_hmac_signer.so }

scenarios:
  partner_api:
    executor: constant-vus
    vus: 20
    duration: 5m
    flow:
      - request:
          name: create order
          method: POST
          url: https://partner.example.com/v1/orders
          body: '{"sku":"${sku}","qty":${qty}}'
          sign:
            type: plugin              # sign via a service plugin
            service: hmac-signer      # the signer that stamps the request
            config:
              secret: ${PARTNER_SECRET}       # from the environment, not the plan
              algo: sha256                    # sha256 | sha512
              header: x-signature             # header the signature is written to
              template: "{method}{path}{body}"
          checks:
            - { type: status, equals: 201 }
```

Because it is a plain hook, you can sign some requests in a flow and leave others
unsigned; a request without a `sign:` block goes out untouched. To sign different
endpoints under different secrets, headers, or templates in one plan, add a
per-request `sign:` block with its own `config:` — the settings are fixed per hook,
not global.

The secret is **not** put in the plan. Pull it from the environment with
`${PARTNER_SECRET}` (or a `${secret.…}` store) and supply it at run time:

```bash
PARTNER_SECRET=… loadr run examples/hmac-signer.yaml
```

## Config reference

Config is the JSON object under the request's `sign:` block, handed to the signer
at each request (e.g.
`{"secret":"…","algo":"sha256","header":"x-signature","template":"{method}{path}{body}"}`).

| Key | Type | Default | Meaning |
|---|---|---|---|
| `secret` | string | *(required)* | Shared secret keying the HMAC. Supports `${…}` interpolation, so it resolves from the environment rather than the committed plan. A missing or empty secret fails the run at start rather than sending unsigned requests. |
| `algo` | string | `sha256` | HMAC hash: `sha256` (HMAC-SHA256) or `sha512` (HMAC-SHA512). An unknown value is a configuration error surfaced when the signer is first called. |
| `header` | string | `x-signature` | Name of the request header the signature is written to — e.g. `x-signature`, `x-hub-signature-256`, `x-webhook-signature`. |
| `template` | string | `{method}{path}{body}` | The **canonical string** the HMAC is taken over. `{…}` placeholders (see below) are substituted per request; any surrounding literal text (separators, a scheme prefix) is signed verbatim. `${…}` interpolation also resolves here for per-VU or feeder values. |
| `encoding` | string | `hex` | How the signature bytes are encoded into the header value: `hex` (lowercase) or `base64`. |
| `prefix` | string | `""` | Literal text prepended to the encoded signature in the header — e.g. `sha256=` for GitHub-style `x-hub-signature-256`. |

`${env.…}` and other interpolation resolve before the config reaches the plugin, so
a per-environment `secret` or `header` can be templated without editing the plan.

### Template placeholders

The `template` is the exact byte string signed; match the partner's documented
canonicalization precisely or every signature is rejected. The placeholders are
substituted from the outgoing request:

| Placeholder | Expands to |
|---|---|
| `{method}` | HTTP method, upper-case (`POST`) |
| `{path}` | Request path, including query string |
| `{url}` | Full request URL |
| `{body}` | Raw request body bytes, as they leave |
| `{timestamp}` | Unix seconds at signing time |

```yaml
# GitHub-style webhook: sha256= prefix, hex encoding, HMAC-SHA256 over the body alone.
sign:
  type: plugin
  service: hmac-signer
  config:
    secret: ${WEBHOOK_SECRET}
    algo: sha256
    header: x-hub-signature-256
    prefix: "sha256="
    template: "{body}"

# Timestamped canonical string, base64 signature (pair {timestamp} with an
# x-timestamp header the receiver reads back to recompute the same string).
sign:
  type: plugin
  service: hmac-signer
  config:
    secret: ${PARTNER_SECRET}
    algo: sha512
    header: x-signature
    encoding: base64
    template: "{timestamp}.{method}.{path}.{body}"
```

## Metrics

The signer emits one counter for the signatures it computes, so you can confirm
requests are actually being signed and gate on it in `thresholds`:

| Metric | Kind | Meaning |
|---|---|---|
| `hmac_signatures` | counter | One increment per request signed (canonical string rendered and the header stamped). |

A healthy run shows `hmac_signatures` climbing in step with the signed requests in
the flow; a flat counter means the `sign:` hook is not wired to the requests you
expected.

```yaml
thresholds:
  hmac_signatures: [ "count>0" ]     # fail if nothing was actually signed
```

## Notes

- **No C dependency.** The keyed MAC and hash are computed in pure Rust (`hmac` +
  `sha2`); there is no OpenSSL and no external signing tool, which is why the plugin
  installs by name with no build toolchain.
- **The template is a contract.** The bytes you sign must match the partner's
  canonicalization exactly — the same field order, separators, and body form. A
  `{method}{path}{body}` string and a `{body}`-only string produce different
  signatures, so mirror the provider's spec rather than guessing.
- **Sign the body as it leaves.** `{body}` is the raw request body after `${…}`
  interpolation, exactly as it goes on the wire — sign the rendered body, not the
  template. If the server re-serializes the JSON before verifying, canonicalize the
  body in the plan so both sides hash identical bytes.
- **Keep the secret in the environment.** Pass `secret` via `${PARTNER_SECRET}` (or
  a `${secret.…}` store) rather than committing it to the plan; it is redacted from
  logs and exports.
- **Per-request, on the hot path but cheap.** Signing runs inline before each
  request, but an HMAC is a couple of hash rounds — negligible next to the network
  round-trip — so it adds no meaningful overhead to the load generator.
- **In-process service.** Native service plugins run in-process with full privileges
  (see [Native plugins](native.md#safety-notes)); the signer does no network or disk
  I/O of its own — it only transforms the request headers.
- **Verifying, not signing?** To *check* an inbound signature on a response instead
  of producing one on a request, use the [`response-signature`](response-signature.md)
  assertion plugin; `hmac-signer` is the outbound counterpart.
```
