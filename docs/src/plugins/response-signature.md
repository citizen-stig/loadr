# response-signature plugin

> **Status:** planned — this page documents the intended contract; the plugin is
> not yet shipped in the plugin index.

`loadr-plugin-response-signature` adds a **signature-verification assertion**. It
is a native `assertion` plugin: it recomputes a signature over the response body
(and, optionally, a selected set of response headers) with a shared secret or
public key, then compares that signature to the one the server sent in a
signature header — in **constant time** — failing the check on any mismatch. It
proves that a webhook or API response is authentic and untampered, not just that
its shape is right.

Verification is pure Rust — HMAC via [`hmac`]/[`sha2`], and RSA via [`rsa`] —
so there is no external tool, no OpenSSL, and no C dependency. The comparison
uses a constant-time equality check, so a failing signature reveals nothing
about how many leading bytes matched.

It exists to gate a response on its *authenticity*: that the body you received
is exactly the body the server signed, under a key only the two of you share.
When the recomputed signature matches the header the check passes; when it does
not — a tampered body, a wrong secret, a missing header — the request is marked
failed and the error says which.

The plugin implements the native `PluginAssertion` trait; the contract is
documented in [Developing a plugin](developing.md).

[`hmac`]: https://docs.rs/hmac
[`sha2`]: https://docs.rs/sha2
[`rsa`]: https://docs.rs/rsa

## Build and install

```bash
cargo build -p loadr-plugin-response-signature --release

# `loadr plugin install` copies a directory that holds plugin.toml next to the
# artifact named by its `entry`. Stage the built cdylib beside the manifest:
mkdir -p dist
cp plugins/loadr-plugin-response-signature/plugin.toml dist/
cp target/release/libloadr_plugin_response_signature.so dist/   # .dylib on macOS, .dll on Windows
loadr plugin install dist
loadr plugin info response-signature
```

Installing copies `plugin.toml` and the artifact into
`~/.loadr/plugins/response-signature/`. The manifest declares it as a native
assertion:

```toml
[plugin]
name = "response-signature"
kind = "assertion"
type = "native"
entry = "libloadr_plugin_response_signature.so"
description = "Verify a response signature header against a recomputed HMAC/RSA signature"
```

## Use it in a test

List the plugin under `plugins:` with its `config`, then reference it from an
`assert:` step by `type: plugin`. Plugin assertions are addressed by plugin
**name**; the `config` from the `plugins:` entry is passed to every call.

```yaml
plugins:
  - name: response-signature
    config:
      header: x-signature
      algo:   hmac-sha256
      secret: ${WEBHOOK_SECRET}      # interpolated from the environment

scenarios:
  webhooks:
    executor: constant-vus
    vus: 10
    duration: 30s
    flow:
      - request:
          name: deliver webhook
          method: POST
          url: /hooks/order.created
          body: '{"id":"${order_id}","status":"paid"}'
          assert:
            - { type: status, equals: 200 }
            # Fail the request unless x-signature matches HMAC-SHA256(body, secret)
            - { type: plugin, name: signature is authentic, plugin: response-signature }
```

The `config` is JSON-shaped and handed to the plugin as-is, e.g.
`{"header":"x-signature","algo":"hmac-sha256","secret":"…"}`.

## Config reference

| Key         | Type            | Required | Meaning |
|-------------|-----------------|----------|---------|
| `header`    | string          | yes      | Name of the response header carrying the signature to check against (case-insensitive), e.g. `x-signature`, `x-hub-signature-256`. |
| `algo`      | string          | yes      | Signature algorithm: `hmac-sha256`, `hmac-sha512`, `hmac-sha1`, `rsa-sha256`, or `rsa-sha512`. |
| `secret`    | string          | yes\*    | Shared secret for the `hmac-*` algorithms. Supports `${…}` interpolation, so it can come from the environment rather than the plan. |
| `public_key`| string          | yes\*    | PEM public key (or a path to one, resolved relative to the plan file) for the `rsa-*` algorithms. |
| `encoding`  | string          | no       | How the header encodes the signature bytes: `hex` (default) or `base64`. |
| `prefix`    | string          | no       | A fixed prefix stripped from the header value before decoding, e.g. `sha256=` for GitHub-style `x-hub-signature-256`. |
| `headers`   | list of strings | no       | Response headers to fold into the signed message, in order, before the body. Each is appended as `name:value`. Omit to sign the body alone. |

\* Provide `secret` for the `hmac-*` algorithms and `public_key` for the `rsa-*`
algorithms. A missing or malformed key/secret, an unknown `algo`, or an
undecodable `encoding` is a **configuration error** surfaced when the plugin is
first called — the run stops rather than silently passing.

```yaml
# GitHub-style webhook: sha256= prefix, hex encoding, HMAC-SHA256 over the body.
plugins:
  - name: response-signature
    config:
      header: x-hub-signature-256
      algo:   hmac-sha256
      prefix: "sha256="
      encoding: hex
      secret: ${WEBHOOK_SECRET}

# RSA-signed response, signature base64-encoded, covering two headers + the body.
plugins:
  - name: response-signature
    config:
      header:  x-signature
      algo:    rsa-sha256
      encoding: base64
      headers: [ "x-timestamp", "x-request-id" ]
      public_key: ./keys/webhook.pub.pem
```

The check **passes** when the header is present, decodes cleanly, and the
recomputed signature matches it in constant time. It **fails** when the header is
absent, when it fails to decode under the configured `encoding`/`prefix`, or when
the signatures differ — the check message says which case occurred (e.g.
`missing header x-signature`, or `signature mismatch`), never the expected bytes.

## Metrics

**n/a.** Assertion plugins run inside an existing request's lifecycle and do not
emit their own metric family. A failed verification marks the surrounding request
as failed, so it flows into the standard `checks` rate and `http_req_failed`
just like any built-in `assert:` entry — gate on those with `thresholds:`.

```yaml
thresholds:
  checks: [ "rate>0.99" ]
```

## Notes

- **Constant-time compare.** The recomputed and received signatures are compared
  with a fixed-time equality check, so a mismatch discloses nothing about how
  many bytes lined up — the same reason a real webhook receiver avoids `==`.
- **The signed message is `[headers…] + body`.** With no `headers:` the message
  is the raw response body exactly as received (no re-serialization). When
  `headers:` is set, each named header is folded in first, in the listed order,
  so the plan must match the order the server signed — mirror the provider's
  canonicalization exactly or every check fails.
- **Secrets out of the plan.** Prefer `${WEBHOOK_SECRET}` interpolation over an
  inline `secret:` so the shared key stays in the environment, not the committed
  YAML.
- **HMAC vs RSA.** The `hmac-*` algorithms need the shared `secret`; the `rsa-*`
  algorithms verify with a `public_key` and never need the private signing key,
  so a test plan can prove authenticity without holding the secret that produced
  the signature.
- **Body-shape checks are separate.** This plugin proves the body is *authentic*,
  not that it is *well-formed*. Pair it with the built-in `type: jsonpath` /
  `body_contains` assertions, or the [json-schema](json-schema.md) plugin, when
  you also need to gate on the body's contents.
- **Fixed per entry.** The `config` (header, algorithm, key) is fixed per
  `plugins:` entry. To verify different endpoints under different keys or headers
  in one plan, list the plugin more than once under distinct names, each with its
  own `config`.
