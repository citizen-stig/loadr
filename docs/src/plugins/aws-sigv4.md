# aws-sigv4 plugin

> **Status:** planned — this plugin is not in the signed [plugin index](installing.md)
> yet. The shape below describes the intended signer contract; the config keys and
> metric names may still change before the first release.

`loadr-plugin-aws-sigv4` is a **service plugin** (`kind = "service"`, role: *auth
& signers*). It is a small in-process **AWS Signature Version 4 signer**: given a
request, a set of credentials, and a `region` + `service`, it computes the SigV4
**canonical request**, derives the signing key, and returns the `Authorization`
header (plus the `X-Amz-Date` and, for temporary credentials, `X-Amz-Security-Token`
headers) that AWS expects. A request that names it as its signer gets those
headers stamped on just before it goes out.

It is **pure Rust** — the SHA-256 hashing and HMAC are done with `sha2` and `hmac`,
with **no AWS SDK, no `aws-*` crates, and no C dependency**. That is the same
signer the [`s3-archive`](s3-archive.md) and [`cloudwatch`](cloudwatch.md) output
plugins use internally to sign their own uploads; packaged as a `service` plugin,
the same code becomes reusable as a **request signer hook** so any HTTP request in
a plan can be SigV4-signed against any AWS service.

The service lifecycle it uses is the native `FfiService` contract
(`start(config_json) → stop()`) documented in
[Native plugins](native.md#the-interface); it implements the `ServicePlugin` trait
and is invoked per-request through the signer hook rather than polling on a timer.

## Install

Once published, `aws-sigv4` will resolve from the signed
[plugin index](installing.md) by name — no build toolchain required:

```bash
loadr plugin install aws-sigv4
loadr plugin info aws-sigv4
```

This picks the artifact for your host target, checks it against the plugin ABI
your `loadr` build provides, verifies its sha256 and unpacks it into your plugins
directory (`~/.loadr/plugins/aws-sigv4/`, or `$LOADR_PLUGINS_DIR`).

The installed manifest declares a native service plugin:

```toml
[plugin]
name = "aws-sigv4"
version = "0.1.0"
kind = "service"
type = "native"
entry = "libloadr_plugin_aws_sigv4.so"
description = "Pure-Rust AWS SigV4 request signer (canonical request + Authorization header)"
```

To run straight from a build tree instead, point the plan's `plugins:` entry at
the built artifact (`path: target/release/libloadr_plugin_aws_sigv4.so`) rather
than resolving it by name.

## Use it in a test

List the plugin under `plugins:`, then attach it to a request as its signer. A
request's `sign:` block routes the request through a service plugin: `type: plugin`
selects the signer mechanism, `service:` names the plugin, and `config:` carries
the `region` + `service` the signature is scoped to. loadr computes the canonical
request and stamps the `Authorization` header on the request just before it is
sent — the body, headers, and query are all signed as they leave.

```yaml
plugins:
  - name: aws-sigv4                     # or: { name: aws-sigv4, path: target/release/libloadr_plugin_aws_sigv4.so }

scenarios:
  s3_reads:
    executor: constant-vus
    vus: 20
    duration: 5m
    flow:
      - request:
          name: get object
          method: GET
          url: https://my-bucket.s3.eu-west-2.amazonaws.com/reports/latest.json
          sign:
            type: plugin              # sign via a service plugin
            service: aws-sigv4        # the signer that stamps the request
            config:
              region: eu-west-2       # SigV4 region scope
              service: s3             # SigV4 service scope
          checks:
            - { type: status, equals: 200 }
```

The same signer works against any SigV4 service — swap `service: s3` for
`execute-api` to hit a signed API Gateway endpoint, `dynamodb` for a signed
DynamoDB call, and so on. Because it is a plain hook, you can sign some requests
in a flow and leave others unsigned; a request without a `sign:` block goes out
untouched.

Credentials are **not** put in the plan. The signer resolves them from the standard
AWS environment chain — `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, and
`AWS_SESSION_TOKEN` when present — so supply them through `aws-vault exec` (or your
usual credential helper) at run time:

```bash
aws-vault exec my-profile -- loadr run examples/aws-sigv4.yaml
```

## Config reference

Config is the JSON object under the request's `sign:` block, handed to the signer
at each request (e.g. `{"region":"eu-west-2","service":"s3"}`).

| Key | Type | Default | Meaning |
|---|---|---|---|
| `region` | string | `${AWS_REGION}` | AWS region the signature is scoped to (the `region` element of the SigV4 credential scope). Required when `AWS_REGION` is unset. |
| `service` | string | *(required)* | AWS service the signature is scoped to — `s3`, `execute-api`, `dynamodb`, `lambda`, etc. This is the `service` element of the credential scope and must match the endpoint being called. |
| `unsigned_payload` | bool | `false` | When `true`, signs with the `UNSIGNED-PAYLOAD` content hash instead of the SHA-256 of the body — useful for large or streamed `s3` bodies where hashing the whole payload up front is undesirable. |

Credentials come from the environment, never `config`: `AWS_ACCESS_KEY_ID`,
`AWS_SECRET_ACCESS_KEY`, and `AWS_SESSION_TOKEN` (for temporary/STS credentials).
They are **redacted from logs and exports**; a missing access key or secret fails
the run at start rather than sending unsigned requests.

`${env.…}` and other interpolation resolve before the config reaches the plugin,
so a per-environment `region` or `service` can be templated without editing the
plan.

## Metrics

The signer emits one counter for the signatures it computes, so you can confirm
requests are actually being signed and gate on it in `thresholds`:

| Metric | Kind | Meaning |
|---|---|---|
| `sigv4_signatures` | counter | One increment per request signed (canonical request built and `Authorization` header produced). |

A healthy run shows `sigv4_signatures` climbing in step with the signed requests
in the flow; a flat counter means the `sign:` hook is not wired to the requests
you expected.

```yaml
thresholds:
  sigv4_signatures: [ "count>0" ]     # fail if nothing was actually signed
```

## Notes

- **No AWS SDK, no C dependency.** The canonical request, SHA-256 payload hash,
  and HMAC signing key are computed in pure Rust (`sha2` + `hmac`). There is no
  `aws-sdk-*` crate and no OpenSSL/C client, which is why the plugin installs by
  name with no build toolchain.
- **Shared with the AWS output plugins.** This is the exact signer the
  [`s3-archive`](s3-archive.md) and [`cloudwatch`](cloudwatch.md) plugins use to
  sign their own HTTPS calls; the `service` plugin just exposes it as a per-request
  hook so any request in a plan can reuse it.
- **Credentials via the environment.** Signing uses the standard AWS environment
  variables; supply them through `aws-vault exec` (or your usual credential helper)
  rather than putting keys in the plan file. They are redacted from logs.
- **Scope must match the endpoint.** `region` and `service` are part of the
  signature, so they must line up with the host being called — a `service: s3`
  signature sent to an `execute-api` endpoint is rejected by AWS with a signature
  mismatch. Set both to match the URL.
- **Per-request, on the hot path but cheap.** Signing runs inline before each
  request, but a SigV4 signature is a handful of HMAC-SHA256 rounds — negligible
  next to the network round-trip — and the derived signing key is reused across
  requests in the same date/region/service scope.
- **In-process service.** Native service plugins run in-process with full
  privileges (see [Native plugins](native.md#safety-notes)); the signer does no
  network or disk I/O of its own — it only transforms the request headers.
