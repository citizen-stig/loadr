# S3 archive plugin

`loadr-plugin-s3-archive` is a **native output plugin**: instead of streaming a
run's metrics to a live dashboard, it captures the whole run and parks the
finished report as a single compressed object in Amazon S3. It is not built
into loadr core — install it, then add it to a plan's `outputs:` list. Point a
run at a bucket and every result lands as a durable, self-describing artifact
you can pull back for comparison, CI archival, or long-term trend analysis.

The plugin buffers each one-second snapshot locally as the test runs, then in
`finish()` gzip-compresses the accumulated report and uploads it with a single
**HTTPS `PUT`**, signed with **AWS Signature Version 4**. It reuses loadr's own
[hyper](https://github.com/hyperium/hyper) HTTP stack plus a small **pure-Rust
SigV4 signer** — no AWS SDK, no `aws-*` crates, no C dependency — so installing
it adds nothing to the build toolchain and keeps the artifact small.

It follows the same `start` / `on_snapshot` / `finish` lifecycle as the shipped
[`native-output` example](native.md), so if you have read that plugin this one
will feel familiar.

> **Status:** planned. The design is fixed and the transport is pure `hyper`
> plus a pure-Rust SigV4 signer, but the plugin is not part of a published
> release yet. Track it before depending on it in CI.

## Install

`s3-archive` will ship in the signed [plugin index](installing.md), so once
released you install it by name — no build toolchain required:

```bash
loadr plugin install s3-archive
loadr plugin info s3-archive
```

This resolves `s3-archive` in the index, picks the artifact for your host
target, checks it against the plugin ABI your `loadr` build provides, downloads
it, verifies its sha256 and unpacks it into your plugins directory
(`~/.loadr/plugins/s3-archive/`, or `$LOADR_PLUGINS_DIR`).

The installed manifest declares an output plugin:

```toml
[plugin]
name = "s3-archive"
kind = "output"
type = "native"
entry = "libloadr_plugin_s3_archive.so"
description = "Buffers the run report and uploads it gzip-compressed to S3 (SigV4)"
```

To run straight from a build tree instead, install a staged directory that
holds `plugin.toml` next to the built cdylib:

```bash
cargo build -p loadr-plugin-s3-archive --release

mkdir -p dist
cp plugins/loadr-plugin-s3-archive/plugin.toml dist/
cp target/release/libloadr_plugin_s3_archive.so dist/   # .dylib on macOS, .dll on Windows
loadr plugin install dist
```

## Use it in a test

An output plugin is wired in through the plan's `outputs:` list as a
`type: plugin` entry, naming the installed plugin and passing its `config`
straight through to `start`:

```yaml
name: checkout-load

outputs:
  - type: plugin
    name: s3-archive
    config:
      bucket: reports          # S3 bucket the report is written to
      key_prefix: runs/        # object keys are prefixed with this
      region: eu-west-2        # bucket region (endpoint + SigV4 region)

scenarios:
  main:
    executor: constant-vus
    vus: 50
    duration: 10m
    flow:
      - request: { name: list, url: https://api.example.com/items }
      - request:
          name: checkout
          url: https://api.example.com/checkout
          method: POST
          checks: [ { type: status, equals: 200 } ]
```

With the config above the run's report is uploaded to
`s3://reports/runs/<run_id>.json.gz` when the test finishes.

You can run any number of outputs alongside it — a `prometheus` scrape endpoint
or a local `json` archive next to the S3 upload, for example. When the `config`
lives in the plan, the plugin is also reachable ad hoc from the CLI:

```bash
loadr run --output plugin=s3-archive test.yaml
```

## Config reference

The object under `config:` is handed to the plugin's `start` as JSON.

| Key          | Required | Default | Meaning |
|--------------|----------|---------|---------|
| `bucket`     | yes      | —       | S3 bucket the compressed report is uploaded to. |
| `key_prefix` | no       | `""`    | Prefix prepended to the generated object key; the run's `run_id` and a `.json.gz` suffix complete it (e.g. `runs/` → `runs/<run_id>.json.gz`). Use a trailing `/` for a folder-style layout. |
| `region`     | yes      | —       | Bucket region — used both to build the request endpoint and as the SigV4 region. |
| `endpoint`   | no       | `https://{bucket}.s3.{region}.amazonaws.com` | Override the S3 endpoint (S3-compatible stores, VPC endpoints, MinIO). |
| `compression`| no       | `gzip`  | Compression applied to the buffered report before upload. `gzip` or `none`; `none` uploads the raw JSON (and drops the `.gz` suffix). |

Credentials for the SigV4 signature are taken from the standard AWS environment
variables — `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, and
`AWS_SESSION_TOKEN` when present. Keep them out of the plan file and supply them
through the environment (or `aws-vault exec …`) as usual.

`${env.…}` and other interpolation resolve before the config reaches the
plugin, so a bucket or prefix can be templated per environment without editing
the plan.

## What gets uploaded

The plugin does its work at the two ends of the run's lifecycle, not on the hot
path:

- `start` validates the config, resolves credentials, and opens the `hyper`
  client — a bad bucket, missing region, or absent credentials fails the run
  early rather than at the end.
- `on_snapshot` appends each one-second snapshot to an in-memory buffer. There
  is **no per-second network traffic** — snapshots accumulate locally, so the
  export never stalls the load generator and never adds request load to S3
  during the test.
- `finish` serialises the accumulated report plus the end-of-run summary,
  gzip-compresses it, and uploads the whole thing as a **single signed `PUT`**
  to `s3://{bucket}/{key_prefix}{run_id}.json.gz`. Because it is one object per
  run, an archive of past runs is just a listing of the prefix.

The uploaded document has the same shape as loadr's `--summary-export` JSON, so
it can be fed straight back into `loadr report` for a rendered summary or diffed
against an earlier run.

## Metrics

The plugin reports its own health back into the run so an upload problem is
visible in loadr's summary rather than silently losing the archive:

| Metric               | Kind    | Meaning |
|----------------------|---------|---------|
| `s3_archive_objects` | counter | Report objects successfully uploaded to S3 (normally `1` per run). |
| `s3_archive_bytes`   | counter | Compressed bytes `PUT` to S3 for the report. |

A healthy run ends with `s3_archive_objects` at `1` and `s3_archive_bytes`
equal to the object size. A gate on the object count turns a failed upload into
a failed run so a broken archive step does not pass silently in CI:

```yaml
thresholds:
  s3_archive_objects: [ "count>0" ]
```

## Notes

- **No AWS SDK, no C dependency.** The report is uploaded with loadr's own
  hyper HTTP client and signed by a pure-Rust SigV4 implementation. There is no
  `aws-sdk-*` crate and no OpenSSL/C client in the artifact, which is why the
  plugin installs by name with no build toolchain.
- **Buffered, then flushed once.** Snapshots accumulate in memory and the upload
  happens only in `finish()`, so the export adds no per-iteration cost and the
  S3 traffic (and the `s3_archive_bytes` count) is a single end-of-run write.
- **One object per run.** The `run_id` in the key keeps concurrent and repeated
  runs separable; point `key_prefix` at a per-service or per-branch folder and
  the bucket becomes a browsable history of results.
- **S3-compatible stores.** Set `endpoint` to point at MinIO, a VPC gateway
  endpoint, or another SigV4-compatible object store; `region` still supplies
  the signing region.
- **Credentials via the environment.** Signing uses the standard AWS
  environment variables; supply them through `aws-vault exec` (or your usual
  credential helper) rather than putting keys in the plan file.
- **Fail-fast on config, fail-loud on upload.** A missing bucket, region, or
  credentials fails at `start`; an upload error in `finish` is surfaced through
  `s3_archive_objects` staying at `0`, so gate on it in CI rather than assuming
  the archive landed.
```
