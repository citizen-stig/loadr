# DB seeder plugin

> **Status:** planned — not yet in the signed [plugin index](installing.md). This
> page documents the intended shape; the config keys and metric name may still
> change before the first release.

`loadr-plugin-db-seeder` is a **service plugin** (`kind = "service"`, role:
*fixtures & lifecycle*). It does not drive the target and it does not feed VUs:
it **brackets the run**. In `start()` it opens a connection to your database and
executes your **setup** SQL scripts — creating tables, truncating, and inserting
the fixtures the test assumes; in `stop()` it runs your **teardown** SQL to put
the database back. The result is a **known state per run**: every execution
starts from the same seeded baseline and cleans up after itself, so a load test
is reproducible instead of accreting rows from previous runs.

It is **near-pure Rust over [`sqlx`](https://github.com/launchbadge/sqlx)** —
the same driver the [postgres](postgres.md) and [mysql](mysql.md) protocol
plugins are built on — so there is no `psql`/`mysql` shell-out and no extra C
dependency. It enables the `sqlx` feature matching the URL scheme it is given
(`postgres` for `postgres://`, `mysql` for `mysql://`), speaks the wire protocol
directly, and streams each script's statements to the server.

The service lifecycle it uses is the native `FfiService` contract
(`start(config_json) → stop()`) documented in
[Native plugins](native.md#the-interface): `start()` runs the setup scripts
before any VU begins, and `stop()` runs the teardown scripts after the last VU
retires.

## Install

Once published, `db-seeder` will resolve from the signed
[plugin index](installing.md) by name — no build toolchain required:

```bash
loadr plugin install db-seeder
loadr plugin info db-seeder
```

This picks the artifact for your host target, checks it against the plugin ABI
your `loadr` build provides, verifies its sha256 and unpacks it into your plugins
directory (`~/.loadr/plugins/db-seeder/`, or `$LOADR_PLUGINS_DIR`).

The installed manifest declares a native service plugin:

```toml
[plugin]
name = "db-seeder"
version = "0.1.0"
kind = "service"
type = "native"
entry = "libloadr_plugin_db_seeder.so"
description = "Runs setup SQL before a run and teardown SQL after, for a known fixture state per run"
```

To run straight from a build tree instead, point the plan's `plugins:` entry at
the built artifact
(`path: target/release/libloadr_plugin_db_seeder.so`) rather than resolving it
by name.

## Use it in a test

List the plugin under `plugins:`, then declare it under the plan's `services:`
block. `type: plugin` routes the step to a service plugin and `service:` names
it; the `config:` block carries the database URL and the setup / teardown script
lists. The service starts **once** at the beginning of the run — before any VU —
runs the setup scripts in order, and runs the teardown scripts in `stop()` after
the run finishes:

```yaml
plugins:
  - name: db-seeder            # or: { name: db-seeder, path: target/release/libloadr_plugin_db_seeder.so }

services:
  fixtures:
    type: plugin
    service: db-seeder
    config:
      url: postgres://loadr:loadr@db.example.com:5432/loadr   # ${env.DATABASE_URL} keeps creds out of the plan
      setup:
        - sql/schema.sql       # scripts run in listed order, before the run
        - sql/seed.sql
      teardown:
        - sql/clean.sql        # scripts run in listed order, after the run

defaults:
  http:
    base_url: https://api.example.com

scenarios:
  checkout:
    executor: constant-vus
    vus: 50
    duration: 5m
    flow:
      - request: { name: list,     url: /products,        checks: [ { type: status, equals: 200 } ] }
      - request: { name: checkout, url: /checkout, method: POST, checks: [ { type: status, equals: 200 } ] }

thresholds:
  http_req_failed:   [ "rate<0.01" ]
  http_req_duration: [ "p(95)<400ms" ]
```

Because seeding happens in `start()` before the executor spins up, the very
first request already sees the fixtures; because teardown happens in `stop()`,
the database is clean whether the run passed, failed a threshold, or was
interrupted. A missing script file or a failing setup statement fails `start()`,
so the plan is rejected **before** load begins rather than driving traffic
against a half-seeded database.

Inline SQL is also accepted in place of a file path, for a one-liner that isn't
worth a separate file:

```yaml
config:
  url: ${env.DATABASE_URL}
  setup:
    - "TRUNCATE orders, order_items RESTART IDENTITY CASCADE;"
    - sql/seed.sql
  teardown:
    - "TRUNCATE orders, order_items RESTART IDENTITY CASCADE;"
```

## Config reference

Config is the JSON object under the service's `config:` key. It is handed to the
service's `start()` verbatim at run start
(`{"url":"postgres://…","setup":["seed.sql"],"teardown":["clean.sql"]}`).

| Key             | Type              | Default        | Meaning |
|-----------------|-------------------|----------------|---------|
| `url`           | string            | *(required)*   | Database connection URL. `postgres://` / `postgresql://` selects the PostgreSQL driver, `mysql://` the MySQL driver; anything `sqlx` accepts works (including `?sslmode=require`). A missing or malformed URL fails `start()`. Pull it from `${env.…}` so credentials stay out of the plan. |
| `setup`         | array of string   | `[]`           | Scripts run once, in listed order, in `start()` before any VU begins. Each entry is a **path to a `.sql` file** (relative to the plan) or an **inline SQL string**. A statement error aborts `start()`, so the run does not begin against a bad fixture. |
| `teardown`      | array of string   | `[]`           | Scripts run once, in listed order, in `stop()` after the run ends. Same file-or-inline form as `setup`. Run on a best-effort basis so cleanup happens even after a failed run (see notes). |
| `on_setup_error`| string            | `abort`        | What a failing setup statement does: `abort` fails `start()` and the run never begins; `continue` logs the error and proceeds to the next statement (useful for idempotent `CREATE … IF NOT EXISTS` scripts). |
| `transaction`   | bool              | `false`        | Wrap each script in a single transaction, so a script is applied all-or-nothing. Leave `false` for scripts containing statements that cannot run inside a transaction (e.g. some DDL). |

`${env.…}` / `${secrets.…}` and other interpolation resolve before the config
reaches the plugin, so the connection URL and any inline values stay out of the
plan file. The URL is **redacted from logs and exports**.

Statements within a single file are split and executed in order; parameters are
not bound (these are fixture scripts, not per-VU queries), so write literal SQL.
Use the [postgres](postgres.md) / [mysql](mysql.md) protocol plugins when the
database is the thing under test.

## Metrics

The plugin reports its own progress back into the run, so a seeding problem is
visible in loadr's summary rather than silently leaving the database in the wrong
state:

| Metric                  | Kind    | Meaning |
|-------------------------|---------|---------|
| `db_seeder_statements`  | counter | One per SQL statement successfully executed, across every setup and teardown script. |

A healthy run shows `db_seeder_statements` reaching the total number of setup
statements before load starts, then advancing again by the teardown count at the
end. Gate on it to prove the fixtures were actually applied:

```yaml
thresholds:
  db_seeder_statements: [ "count>0" ]
```

## Notes

- **Known state per run.** The whole point of the plugin: `start()` seeds a
  deterministic baseline and `stop()` tears it down, so consecutive runs are
  reproducible and don't accumulate rows or drift. Pair a `TRUNCATE`/seed
  `setup` with a matching `teardown` for a clean slate every time.
- **Fails fast at startup.** With the default `on_setup_error: abort`, a missing
  script or a failing statement fails `start()` before any VU begins — a broken
  fixture surfaces up front, not as a wall of 500s once load is running.
- **Teardown is best-effort.** `stop()` runs the teardown scripts even when the
  run failed a threshold or was interrupted, so the database is left clean. A
  teardown error is logged and counted but does not change the run's exit code,
  which stays governed by `thresholds:`.
- **Keep credentials in the environment.** The connection `url` is a credential —
  pass it via `${env.DATABASE_URL}` / `${secrets.…}`, never hard-coded in a
  committed plan. The URL is redacted from logs and exports.
- **Fixture scripts, not per-VU queries.** Setup/teardown SQL is literal and runs
  once on the controller; it takes no bind parameters. To exercise the database
  *under load*, drive it with the [postgres](postgres.md) or [mysql](mysql.md)
  protocol plugin from `flow:` instead.
- **Distributed runs.** Seeding and teardown happen once, on the controller — not
  per agent — so the shared database is seeded a single time for the whole fleet,
  and there is no race between workers truncating and re-seeding the same tables.
- **In-process service.** Native service plugins run in-process with full
  privileges (see [Native plugins](native.md#safety-notes)); this one does no I/O
  beyond the configured database connection.
```
