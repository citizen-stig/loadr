# Variables, secrets & interpolation

`${...}` placeholders work in URLs, headers, params, bodies (string leaves),
request names, WebSocket frames, gRPC messages and GraphQL variables.

| Form | Resolves to |
|---|---|
| `${env.NAME}` | process environment variable |
| `${vars.name}` | the `variables:` block |
| `${secrets.name}` | the `secrets:` block (redacted from logs/reports) |
| `${data.source.column}` | current data row |
| `${name}` | extracted variable / JS-set `session.vars.name` |
| `${vu}` / `${iteration}` / `${scenario}` | the running VU id / iteration index / scenario name |
| `${js: expr}` | evaluate JS in the VU's runtime, e.g. `${js: Date.now()}` |

Escape a literal with `$${` → `${`.

```yaml
variables:
  tenant: acme
  api_base: "https://${env.REGION}.api.example.com"   # env resolved at startup

secrets:
  api_key: { env: API_KEY }          # from the environment
  db_pass: { file: ./secrets/db }    # from a file (trimmed)

scenarios:
  s:
    executor: constant-vus
    vus: 1
    duration: 1m
    flow:
      - request:
          url: /tenants/${vars.tenant}/ping
          headers:
            X-Api-Key: ${secrets.api_key}
            X-Request-Id: "${js: crypto.uuidv4()}"
```

Notes:

- `variables` values may interpolate `${env.*}` — resolved once at startup.
  Other namespaces resolve per use, inside the iteration.
- Secrets never appear in console output, summaries or validation messages.
- `loadr validate` errors on `${vars.*}` / `${secrets.*}` / `${data.*}`
  references that don't exist (with did-you-mean), and warns on bare names no
  extractor produces.
