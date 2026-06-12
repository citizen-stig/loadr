# Environments

One test file, many targets. The `env:` block holds named overlays that
deep-merge over the document when selected with `-e`:

```yaml
defaults:
  http: { base_url: https://prod.example.com, timeout: 10s }

env:
  staging:
    defaults:
      http:
        base_url: https://staging.example.com    # only this key changes
        tls: { insecure_skip_verify: true }
  ci:
    scenarios:
      api: { vus: 1, duration: 10s }             # tiny load in CI
    thresholds:
      http_req_duration: [ "p(95)<5000" ]        # lax CI thresholds

scenarios:
  api:
    executor: constant-vus
    vus: 20
    duration: 5m
    flow: [ { request: { url: /health } } ]
```

```bash
loadr run test.yaml               # production values
loadr run -e staging test.yaml    # staging overlay
loadr run -e ci test.yaml         # CI overlay
```

Merge rules:

- **Mappings merge recursively** — you only write the keys that differ.
- **Scalars and lists replace** — an overlay `outputs:` list replaces the
  base list entirely.
- The `env:` block itself is removed before the merge (overlays can't nest).
- Unknown `-e` names fail fast, listing the available environments.

Combine with `${env.*}` interpolation and `secrets:` for values that differ
per machine rather than per environment.
