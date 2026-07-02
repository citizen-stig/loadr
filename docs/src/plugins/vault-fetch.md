# vault-fetch plugin

> **Status:** planned — not yet in the signed [plugin index](installing.md). This
> page documents the intended shape; the config keys and metric names may still
> change before the first release.

`loadr-plugin-vault-fetch` is a **service** plugin in the *auth & signers* role.
Instead of driving a target, it runs once at the start of a test: it
**authenticates to [HashiCorp Vault](https://www.vaultproject.io/)** (a token or
an AppRole login), reads one or more **KV secrets** over HTTPS, and **exposes
them as loadr secrets/env** for the VUs to reference. Credentials never live in
the plan file — the test refers to `${secrets.<name>}`, and the actual values are
pulled from Vault at run start.

The transport is nothing but plain HTTPS over
[`hyper`](https://hyper.rs/) — loadr's own HTTP stack. There is no Vault SDK, no
`vault` CLI, and no extra C dependency: the login and the KV read are hand-rolled
Vault HTTP API calls, so the plugin is trivially buildable against the current
core.

The service lifecycle it uses is the native `ServicePlugin` contract
(`start(config_json) → stop()`) documented in
[Native plugins](native.md#the-interface): `start()` performs the login and the
KV read and returns once the secrets are staged; `stop()` revokes the lease (best
effort) at the end of the run.

## Install

Once published, `vault-fetch` will resolve from the signed
[plugin index](installing.md) by name — no build toolchain required:

```bash
loadr plugin install vault-fetch
loadr plugin info vault-fetch
```

This picks the artifact for your host target, checks it against the plugin ABI
your `loadr` build provides, verifies its sha256 and unpacks it into your plugins
directory (`~/.loadr/plugins/vault-fetch/`, or `$LOADR_PLUGINS_DIR`).

The installed manifest declares a native service plugin:

```toml
[plugin]
name = "vault-fetch"
version = "0.1.0"
kind = "service"
type = "native"
entry = "libloadr_plugin_vault_fetch.so"
description = "Fetches KV secrets from Vault at run start and exposes them to VUs"
```

To run straight from a build tree instead, point the plan's `plugins:` entry at
the built artifact (`path: target/release/libloadr_plugin_vault_fetch.so`) rather
than resolving it by name.

## Use it in a test

List the plugin under `plugins:` with its Vault connection and auth config. At
run start the service logs in, reads the KV path, and stages every field of the
secret under the plugin's namespace. Reference the fetched values with the normal
`${secrets.<name>}` interpolation — the same syntax a `secrets:` entry sourced
from the process environment uses.

```yaml
plugins:
  - name: vault-fetch                        # or: { name: vault-fetch, path: target/release/libloadr_plugin_vault_fetch.so }
    config:
      addr: https://vault:8200               # Vault API address (HTTPS)
      path: secret/data/app                  # KV v2 read path
      auth:
        approle:
          role_id:   ${env.VAULT_ROLE_ID}    # bootstrap creds still come from env,
          secret_id: ${env.VAULT_SECRET_ID}  # not the plan file

secrets:
  db_password: { plugin: vault-fetch, key: db_password }   # a field of the KV secret
  api_token:   { plugin: vault-fetch, key: api_token }

scenarios:
  main:
    executor: constant-vus
    vus: 50
    duration: 5m
    flow:
      - request:
          name: login
          method: POST
          url: https://api.example.com/login
          body:
            json:
              password: "${secrets.db_password}"
          checks:
            - { type: status, equals: 200 }
      - request:
          name: fetch resource
          url: https://api.example.com/resource
          headers:
            Authorization: "Bearer ${secrets.api_token}"
          checks:
            - { type: status, equals: 200 }
```

Because the service runs once in `start()` before any VU is spun up, the fetch
adds no per-request overhead and never touches the hot path. The KV fields are
resolved into `${secrets.…}` before the load starts, so a Vault outage or a bad
credential fails the run at startup rather than mid-test.

Token auth is the same shape with a static token instead of the AppRole block:

```yaml
plugins:
  - name: vault-fetch
    config:
      addr: https://vault:8200
      path: secret/data/app
      auth:
        token: ${env.VAULT_TOKEN}
```

## Config reference

Config is the JSON object under the plugin's `config:` key. It is handed to the
service's `start()` verbatim at run start.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `addr` | string | *(required)* | Vault API address, e.g. `https://vault:8200`. Must be `https://…` in any non-local setup; a missing or malformed address fails `start()` so a typo is caught before the run. |
| `path` | string | *(required)* | The KV read path, e.g. `secret/data/app` for a KV v2 mount. Each field of the returned secret becomes a fetchable `key` (see `secrets:` below). |
| `auth` | object | *(required)* | Exactly one auth method — `token` or `approle` (see below). |
| `namespace` | string | — | Vault Enterprise namespace, sent as the `X-Vault-Namespace` header. |
| `ca_cert` | string | — | Path to a PEM CA bundle used to verify Vault's TLS certificate. Omit to use the system trust store. |
| `renew` | bool | `false` | When true, the plugin keeps the auth lease alive for the length of the run, renewing it before it expires (see `vault_renewals`). |
| `timeout` | duration | `10s` | Per-request timeout for the login and KV read. Exceeding it fails `start()`. |

### Auth methods

`auth` selects exactly one login method:

| Form | Shape | Meaning |
|---|---|---|
| `token` | `{ token: "<token>" }` | Use a pre-issued Vault token directly. Usually pulled from `${env.VAULT_TOKEN}` so it stays out of the plan. |
| `approle` | `{ approle: { role_id, secret_id } }` | Log in via the [AppRole](https://developer.hashicorp.com/vault/docs/auth/approle) backend and exchange the pair for a token. Prefer this in CI, where the `secret_id` is short-lived. |

`${env.…}` and other interpolation resolve before the config reaches the plugin,
so the bootstrap credentials that let loadr *reach* Vault also stay out of the
plan file.

### Consuming the fetched secrets

A `secrets:` entry sourced from the plugin binds one field of the KV secret to a
name the VUs reference:

```yaml
secrets:
  db_password: { plugin: vault-fetch, key: db_password }
```

`key` names a field inside the KV secret read from `path`; the bound name
(`db_password`) is what `${secrets.db_password}` resolves to. Fetched values are
treated as secrets — they are redacted from logs and never printed.

## Metrics

The plugin reports its own activity back into the run so a fetch or renewal
problem is visible in loadr's summary rather than failing silently:

| Metric | Kind | Meaning |
|---|---|---|
| `vault_secrets_fetched` | counter | One per KV secret field successfully read and staged at run start |
| `vault_renewals` | counter | One per successful lease renewal (only non-zero when `renew: true`) |

A healthy run shows `vault_secrets_fetched` equal to the number of fields you
bound and `vault_renewals` climbing quietly over a long run; a `start()` that
cannot log in or read the path fails the run before either counter moves.

## Notes

- **Secrets never live in the plan.** The whole point of the plugin: the plan
  refers to `${secrets.<name>}`, and the values are pulled from Vault at run
  start. Only the *bootstrap* credential (a token or an AppRole `role_id` /
  `secret_id`) is supplied, and that comes from `${env.…}`, not the file.
- **Fail fast at startup.** The login and KV read happen in `start()`, before any
  VU runs. A Vault outage, an expired `secret_id`, or a wrong `path` fails the run
  immediately rather than surfacing as a wave of auth failures mid-test.
- **Pure `hyper`, HTTPS only.** The Vault API calls go over loadr's own HTTP
  stack — no Vault SDK, no `vault` binary. Use `https://` and, for a private CA,
  point `ca_cert` at your PEM bundle rather than disabling verification.
- **Lease renewal is opt-in.** For short runs the initial token is enough; set
  `renew: true` on a long-running test so the lease is kept alive and
  `vault_renewals` advances. `stop()` revokes the lease on the way out, best
  effort.
- **In-process service.** Native service plugins run in-process with full
  privileges (see [Native plugins](native.md#safety-notes)); this one does no I/O
  beyond the Vault login, the KV read, and any renewals.
