# Test definition overview

A loadr test is one YAML file. Every top-level key:

```yaml
name: my-test                # display name (optional)
description: what it does    # free text (optional)

defaults: { ... }            # request defaults: base URL, headers, timeouts, TLS, tags
env: { ... }                 # named environment overlays (-e <name>)
variables: { ... }           # static values: ${vars.name}
secrets: { ... }             # values from env/file: ${secrets.name} (redacted)
data: { ... }                # CSV / inline data sources: ${data.source.column}
metrics: { ... }             # custom metric declarations
js: { ... }                  # embedded JavaScript module + limits

scenarios: { ... }           # REQUIRED: the workloads
thresholds: { ... }          # pass/fail criteria over metrics
outputs: [ ... ]             # exporters: jsonl, csv, prometheus, influxdb, otlp, statsd
plugins: [ ... ]             # plugins to load
```

Unknown keys are rejected with a did-you-mean suggestion. Durations are
strings like `300ms`, `30s`, `1m30s`, `1h` (bare numbers mean seconds).

## Defaults

```yaml
defaults:
  http:
    base_url: https://api.example.com   # joined with relative request URLs
    headers: { User-Agent: loadr/0.1 }
    timeout: 30s                        # per request (default 30s)
    follow_redirects: true              # default true
    max_redirects: 10
    version: auto                       # auto | http1 | http2 | http2-prior-knowledge
    compression: true                   # Accept-Encoding + auto-decompress
    keep_alive: true                    # reuse connections within a VU
    proxy: http://proxy.internal:3128
    cookies: true                       # automatic per-VU cookie jar
    tls:
      insecure_skip_verify: false
      ca_file: ./ca.pem                 # extra trusted roots
      cert_file: ./client.pem           # mTLS client certificate
      key_file: ./client-key.pem
      server_name: override.sni.name
  tags: { team: payments }              # added to every sample
  think_time: { type: uniform, min: 1s, max: 2s }   # default pause after each request
```

## Minimal complete test

```yaml
scenarios:
  s:
    executor: constant-vus
    vus: 1
    duration: 10s
    flow:
      - request: { url: https://example.com/ }
```

Everything else is optional. See the following chapters for each block, or
generate the JSON Schema (`loadr schema`) for the exhaustive picture.
