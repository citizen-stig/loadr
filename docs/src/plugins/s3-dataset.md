# S3 dataset plugin

> **Status:** planned — not yet in the signed [plugin index](installing.md). This
> page documents the intended shape; the config keys and metric names may still
> change before the first release.

`loadr-plugin-s3-dataset` is a **service** plugin in the *data sources &
feeders* role. Instead of driving a target, it acts as a **data source**: it
fetches a single object from Amazon S3 at startup, parses it into rows, and
hands each VU the next row through the usual feeder interpolation. It turns a
CSV or JSON object living in a bucket into a `data:` feeder — the same shape as
a local `type: csv` file, but sourced from S3 so the fixture lives next to the
system under test rather than in the repo.

The fetch is a plain **HTTPS `GET`** on the object, signed with **AWS Signature
Version 4**. It reuses loadr's own [hyper](https://github.com/hyperium/hyper)
HTTP stack plus a small **pure-Rust SigV4 signer** — no AWS SDK, no `aws-*`
crates, no C dependency — so installing it adds nothing to the build toolchain
and keeps the artifact small.

The contract it uses is documented in
[Developing a plugin](developing.md#services).

## Install

`s3-dataset` will ship in the signed [plugin index](installing.md), so once
released you install it by name — no build toolchain required:

```bash
loadr plugin install s3-dataset
loadr plugin info s3-dataset
```

This resolves `s3-dataset` in the index, picks the artifact for your host
target, checks it against the plugin ABI your `loadr` build provides, downloads
it, verifies its sha256 and unpacks it into your plugins directory
(`~/.loadr/plugins/s3-dataset/`, or `$LOADR_PLUGINS_DIR`).

The installed manifest declares a service plugin:

```toml
[plugin]
name = "s3-dataset"
kind = "service"
type = "native"
entry = "libloadr_plugin_s3_dataset.so"
```

To run straight from a build tree instead, point the plan's `plugins:` entry at
the built artifact
(`path: target/release/libloadr_plugin_s3_dataset.so`) rather than resolving it
by name.

## Use it in a test

List the plugin under `plugins:`, then declare a `data:` feeder whose
`service:` names it. The plugin's `config:` block says which object to fetch;
each row it parses binds values the VUs reference through the usual
`${data.<name>.<column>}` interpolation — exactly like a CSV feeder, but the
file comes from S3.

```yaml
plugins:
  - name: s3-dataset          # or: { name: s3-dataset, path: target/release/libloadr_plugin_s3_dataset.so }

data:
  users:
    type: service             # this feeder is filled by a service plugin
    service: s3-dataset        # the plugin that fills it
    config:
      bucket: data            # S3 bucket name
      key: users.csv          # object key within the bucket
      region: eu-west-2       # bucket region (used to build the endpoint + sign)
    pick: sequential          # standard feeder strategy
    on_eof: recycle           # standard EOF policy

scenarios:
  login_flow:
    executor: constant-vus
    vus: 25
    duration: 5m
    flow:
      - request:
          name: log in
          method: POST
          url: https://api.example.com/login
          body:
            json:
              email: "${data.users.email}"
              password: "${data.users.password}"
          checks:
            - { type: status, equals: 200 }
```

A `users.csv` object with an `email,password` header exposes each column as
`${data.users.email}` and `${data.users.password}`. The object is fetched once
when the run starts; rows are then served from memory, so no per-VU S3 traffic
happens during the test.

## Config reference

The feeder is wired to the plugin with two keys on the `data.<name>` block —
`type: service` (route this feeder to a service plugin) and `service: s3-dataset`
(which plugin fills it) — and its behaviour is set through the plugin `config:`
block:

| Key       | Required | Default            | Meaning |
|-----------|----------|--------------------|---------|
| `bucket`  | yes      | —                  | S3 bucket holding the object. |
| `key`     | yes      | —                  | Object key within the bucket (e.g. `users.csv`, `seed/skus.json`). |
| `region`  | yes      | —                  | Bucket region — used both to build the request endpoint and as the SigV4 region. |
| `format`  | no       | inferred from `key` | `csv` or `json`. Inferred from the key's extension when omitted. |
| `endpoint`| no       | `https://{bucket}.s3.{region}.amazonaws.com` | Override the S3 endpoint (S3-compatible stores, VPC endpoints, MinIO). |

Credentials for the SigV4 signature are taken from the standard AWS environment
variables — `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, and
`AWS_SESSION_TOKEN` when present. Keep them out of the plan file and supply them
through the environment (or `aws-vault exec …`) as usual.

Parsing follows the resolved `format`:

- **`csv`** — the first line is the header; each subsequent line is a row and
  each column is bound as `${data.<name>.<column>}`.
- **`json`** — an array of objects, each object a row, each field bound as
  `${data.<name>.<field>}` (the same shape as a local `type: json` feeder).

Standard feeder controls apply on top: `mode` (shared / per-VU), `pick`
(`sequential` | `random` | `shuffle`) and `on_eof` (`recycle` | `stop`) behave
exactly as they do for a local CSV/JSON source — see
[Feeder strategies](../yaml/feeders.md).

## Metrics

The plugin emits two counters describing the fetch:

| Metric             | Kind    | Meaning |
|--------------------|---------|---------|
| `s3_dataset_rows`  | counter | Rows parsed out of the fetched object. |
| `s3_dataset_bytes` | counter | Bytes downloaded from S3 for the object. |

Use them to confirm the dataset loaded and to size it — a `count>0` on
`s3_dataset_rows` catches an empty or misparsed object before the run leans on
it:

```yaml
thresholds:
  s3_dataset_rows: [ "count>0" ]
```

## Notes

- **No AWS SDK, no C dependency.** The object is fetched with loadr's own hyper
  HTTP client and signed by a pure-Rust SigV4 implementation. There is no
  `aws-sdk-*` crate and no OpenSSL/C client in the artifact, which is why the
  plugin installs by name with no build toolchain.
- **Fetched once, served from memory.** The object is downloaded and parsed when
  the run starts, then rows are handed out locally. The S3 traffic (and the
  `s3_dataset_bytes` count) is the one-time load cost, not per-iteration — the
  dataset does not add request load to S3 during the test.
- **CSV vs JSON.** `format` is inferred from the key's extension; set it
  explicitly when the key has no extension or an unusual one. Column/field names
  drive interpolation, so a header row (CSV) or object keys (JSON) are required.
- **S3-compatible stores.** Set `endpoint` to point at MinIO, a VPC gateway
  endpoint, or another SigV4-compatible object store; `region` still supplies the
  signing region.
- **Credentials via the environment.** Signing uses the standard AWS environment
  variables; supply them through `aws-vault exec` (or your usual credential
  helper) rather than putting keys in the plan file.
