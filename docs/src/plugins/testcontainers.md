# Testcontainers plugin

> **Status:** planned — not yet in the signed [plugin index](installing.md). This
> page documents the intended shape; the config keys and metric names may still
> change before the first release.

`loadr-plugin-testcontainers` is a **native service** plugin in the *fixtures &
lifecycle* role. It stands the throwaway backing services a test needs — a
Postgres, a Redis, a mock API — **up before the run and tears them down after**,
so a plan is self-contained and a CI job needs no `docker compose` sidecar step.

It implements the [`ServicePlugin`](developing.md) trait: `start()` creates and
launches the declared containers, waits for each one to become ready, and
publishes their **mapped host ports** so requests can reach them; `stop()`
removes every container it created and is idempotent, so a crashed or
interrupted run does not leak containers.

The plugin talks to the **Docker Engine directly over its HTTP API** on the
local `/var/run/docker.sock` (or the `DOCKER_HOST` you point it at) using
loadr's own [`hyper`](https://hyper.rs/) stack — **no Docker client library, no
CLI shell-out, no testcontainers SDK**. That keeps it a small, dependency-light
native plugin that builds against the current core.

## Install

`testcontainers` will ship in the signed [plugin index](installing.md), so once
released you install it by name — no build toolchain required:

```bash
loadr plugin install testcontainers
loadr plugin info testcontainers
```

Until then you can build and stage it from source like any native plugin:

```bash
cargo build -p loadr-plugin-testcontainers --release

# `loadr plugin install` copies a directory that holds plugin.toml next to the
# artifact named by its `entry`. Stage the built cdylib beside the manifest:
mkdir -p dist
cp plugins/loadr-plugin-testcontainers/plugin.toml dist/
cp target/release/libloadr_plugin_testcontainers.so dist/   # .dylib on macOS, .dll on Windows
loadr plugin install dist
```

Installing copies `plugin.toml` and the artifact into
`~/.loadr/plugins/testcontainers/` (override with `LOADR_PLUGINS_DIR` or
`--plugins-dir`). The manifest declares it as a native service:

```toml
[plugin]
name = "testcontainers"
kind = "service"
type = "native"
entry = "libloadr_plugin_testcontainers.so"
description = "Starts declared containers for the run and removes them afterwards"
```

## Use it in a test

Service plugins are declared in the plan's `plugins:` list. loadr starts every
listed service before the scenarios run and stops it once they finish. Pass the
containers you want under `config`:

```yaml
plugins:
  - name: testcontainers
    config:
      containers:
        - image: postgres:16
          wait: log:database system is ready to accept connections
          port: 5432
          env:
            POSTGRES_PASSWORD: test
        - image: redis:7-alpine
          wait: log:Ready to accept connections
          port: 6379

scenarios:
  main:
    executor: constant-vus
    vus: 20
    duration: 30s
    flow:
      - request:
          name: query
          url: postgres://postgres:test@127.0.0.1:${env.LOADR_TC_POSTGRES_5432}/postgres
          protocol: postgres
          plugin:
            query: "SELECT 1"
      - request:
          name: cache get
          url: redis://127.0.0.1:${env.LOADR_TC_REDIS_6379}
          plugin:
            command: ["PING"]
```

The minimal form from the task description works too — a single container, a log
wait condition, and one published port:

```yaml
plugins:
  - name: testcontainers
    config:
      containers:
        - { image: "postgres:16", wait: "log:ready", port: 5432 }
```

## How the mapped ports reach your requests

Docker maps each published container port to an **ephemeral host port**, so the
plan cannot hard-code it. `start()` waits for readiness, reads the actual host
port Docker assigned, and **exports it as an environment variable** named
`LOADR_TC_<IMAGE>_<CONTAINER_PORT>` (the image basename upper-cased, tag
stripped). Your requests reference it through normal `${env.…}` interpolation —
`${env.LOADR_TC_POSTGRES_5432}` above resolves to whatever Docker bound
`5432/tcp` to on the host. The string `start()` returns (per the `ServicePlugin`
contract) is a JSON summary of the started containers and their `image → host
port` mappings, which loadr logs at run start.

## Config reference

Config is the JSON object handed to `start()`. The only top-level key is
`containers`; each entry describes one container to create.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `containers` | array | *(required)* | The containers to start. An empty or missing list fails `start()`, so a misconfigured fixture is caught before the run rather than launching nothing. |
| `containers[].image` | string | *(required)* | The image reference, e.g. `postgres:16`. If the image is not present locally the plugin pulls it via the Engine API first. |
| `containers[].port` | number or array | — | Container port(s) to publish to an ephemeral host port. Each becomes a `LOADR_TC_<IMAGE>_<PORT>` env var. Omit for a container that needs no inbound access. |
| `containers[].wait` | string | — | Readiness gate the plugin blocks on before the run starts. `log:<substring>` waits for the substring to appear in the container's log stream; `port` (or `port:<n>`) waits for the mapped port to accept a TCP connection; `healthy` waits for the image's Docker `HEALTHCHECK` to report healthy. With no `wait`, readiness is "container is running". |
| `containers[].env` | object (string → string) | `{}` | Environment variables set inside the container. Values interpolate (`${env.…}`), so pull credentials from the host environment rather than hard-coding them. |
| `containers[].cmd` | array of string | — | Overrides the image's default command/entrypoint arguments. |
| `containers[].name` | string | auto | A stable container name; otherwise a unique `loadr-tc-…` name is generated so parallel runs never collide. |
| `startup_timeout` | duration | `60s` | How long `start()` waits for **all** containers to satisfy their `wait` condition. On timeout it removes anything it already created and fails the run, so a stuck fixture never leaves orphans. |

`${env.…}` and other interpolation resolve before the config reaches the plugin,
so secrets stay out of the plan file.

## Metrics

The plugin reports its own lifecycle back into the run so a fixture problem is
visible in loadr's summary:

| Metric | Kind | Meaning |
|---|---|---|
| `containers_started` | counter | Containers successfully created and marked ready during `start()`. |
| `containers_removed` | counter | Containers removed during `stop()` (and during rollback if `start()` fails partway). |

A clean run shows `containers_started` and `containers_removed` equal to the
number of declared containers. `containers_removed` lower than
`containers_started` after a run means a container survived teardown and should
be cleaned up manually.

## Notes

- **Lifecycle, not load.** This is a fixtures plugin: it does not generate
  traffic or emit request metrics. Pair it with a `protocol` plugin (`postgres`,
  `redis`, …) that actually drives the container it stood up.
- **Idempotent teardown.** `stop()` is safe to call more than once and removes
  containers by the IDs `start()` recorded, so a second Ctrl-C or an early
  failure still cleans up. If `start()` fails halfway, it rolls back the
  containers it already created before returning the error.
- **Docker must be reachable.** The plugin needs a running Docker Engine on the
  local socket (or `DOCKER_HOST`). A missing socket or a permission error fails
  `start()` with a clear message rather than starting the run against nothing.
- **Wait for readiness, not just running.** A container reports "running" long
  before Postgres accepts connections. Use a `wait:` condition (`log:…`,
  `port`, or `healthy`) so the first VU does not race a half-booted service.
- **Ephemeral ports, always.** Ports are mapped to host-assigned ephemeral ports
  and surfaced as `LOADR_TC_*` env vars — reference those, never a fixed host
  port, so concurrent runs on the same machine never clash.
- **Not for production targets.** These are disposable, per-run fixtures. Point a
  soak or capacity test at a real, provisioned environment, not a container this
  plugin spins up and throws away.
</content>
</invoke>
