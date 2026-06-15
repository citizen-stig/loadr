# SQL plugin (PostgreSQL & MySQL)

`loadr-plugin-sql` adds **PostgreSQL** and **MySQL** as load-test targets. It is
a **native protocol plugin**: SQL support is not built into loadr core — the
heavy [`sqlx`](https://github.com/launchbadge/sqlx) driver (and its transitive
`rsa` dependency) ships only inside this plugin's dynamic library. Once the
plugin is installed, a request to a `postgres://`, `postgresql://`, or `mysql://`
URL routes straight to it.

The contract it uses is documented in
[Developing a plugin](developing.md#native-protocol-plugins).

## When to use

Reach for this when the thing under test *is* the database: validating a schema
or index under write pressure, sizing a connection pool, finding the row count
at which a query falls over, or proving latency holds at a steady query rate.
For an application that merely *uses* a database behind an HTTP API, test the API
with the `http` handler instead.

## Build and install

```bash
cargo build -p loadr-plugin-sql --release

# `loadr plugin install` copies a directory that holds plugin.toml next to the
# artifact named by its `entry`. Stage the built cdylib beside the manifest:
mkdir -p dist
cp plugins/loadr-plugin-sql/plugin.toml dist/
cp target/release/libloadr_plugin_sql.so dist/   # .dylib on macOS, .dll on Windows
loadr plugin install dist
loadr plugin info sql
```

Installing copies `plugin.toml` and the artifact into `~/.loadr/plugins/sql/`.
The manifest declares the URL schemes the plugin serves:

```toml
[plugin]
name = "sql"
kind = "protocol"
type = "native"
entry = "libloadr_plugin_sql.so"
schemes = ["postgres", "postgresql", "mysql", "sql"]
```

## The target URL

```
postgres://[user[:password]@]host[:port][/database][?params]
mysql://[user[:password]@]host[:port][/database][?params]
```

- **scheme** selects the backend: `postgres` (alias `postgresql`) or `mysql`.
- **port** defaults to the backend's standard port (`5432` / `3306`).
- **credentials, database, and query parameters** are passed straight to the
  driver, so any URL `sqlx` accepts works here — including `?sslmode=require`
  (PostgreSQL) or `?ssl-mode=REQUIRED` (MySQL) for TLS.

## Use it in a test

List the plugin under `plugins:` and target a `postgres://` or `mysql://` URL.
The statement and its bind parameters go in the request's **`sql`** block:

```yaml
plugins:
  - name: sql            # or: { name: sql, path: target/release/libloadr_plugin_sql.so }

scenarios:
  main:
    executor: constant-vus
    vus: 10
    duration: 30s
    flow:
      - request:
          name: list cheap products
          url: postgres://loadr:loadr@db.example.com:5432/loadr
          sql:
            query: SELECT id, name, price FROM products WHERE price < $1 ORDER BY price
            params: ["50"]
          checks:
            - { type: status, equals: 1 }                 # 1 = ok, 0 = DB error
            - { type: duration, name: query is fast, max: 250ms }

      - request:
          name: insert order
          url: postgres://loadr:loadr@db.example.com:5432/loadr
          sql:
            query: INSERT INTO orders (sku, qty) VALUES ($1, $2)
            params: ["${row.sku}", "${row.qty}"]
          assert:
            - { type: status, equals: 1 }
```

A complete runnable plan is in `examples/27-sql.yaml`.

## Expressing the query

- **`query`** — the SQL to run. Use the backend's placeholder syntax for
  parameters: `$1, $2, …` for PostgreSQL and `?` for MySQL.
- **`params`** — positional bind values, bound *safely* by the driver (never
  string-spliced, so there is no SQL-injection surface). Each value is given as
  text; the plugin infers a type so comparisons against numeric columns work — a
  value that parses as an integer binds as an integer, a decimal as a float,
  everything else as text.

`${...}` interpolation works in both `query` and `params`, so per-VU values and
data-feed columns flow straight into the statement. As a shorthand, a request
with no `sql` block uses its **`body`** as the query text (no parameters); an
empty query is rejected.

## Status, rows, and errors

A request **succeeds** when the query executes without a database error:

| Outcome | `status` | `error` | `extras.rows` |
|---------|----------|---------|---------------|
| SELECT / WITH / SHOW … | `1` | — | rows returned |
| INSERT / UPDATE / DELETE | `1` | — | rows affected |
| database error (bad SQL, constraint, …) | `0` | the DB message | — |
| connection failure / timeout | `0` | the transport error | — |

`extras` carries:

- `extras.backend` — `postgres` or `mysql`.
- `extras.rows` — rows returned (row-producing statements) or affected (DML).

The response **body** is the row count rendered as text, so `body`-based checks
and extraction still work.

## Metrics

loadr turns the plugin's response into the `sql` metric family:

| Metric | Kind | Meaning |
|--------|------|---------|
| `sql_reqs` | counter | queries executed |
| `sql_req_duration` | trend (time) | per-query latency (ms) |
| `sql_rows` | counter | total rows returned/affected |

```yaml
thresholds:
  checks: [ "rate>0.99" ]
  sql_req_duration: [ "p(95)<100ms" ]
```

A request is marked **failed** when the query errors (response `status` 0), so
`http_req_failed` (the shared failure-rate metric) tracks DB errors too.

## Connection pooling

The plugin keeps an internal `sqlx::Pool` keyed by the full connection URI,
shared across every VU. A pool is itself a set of cheaply-cloned, reused
connections, so one per distinct URI is the correct model under load — the first
request for a URI establishes it, and all subsequent requests (any VU) reuse it.
The plugin owns a single Tokio runtime and `block_on`s the async driver, because
the protocol ABI is synchronous and carries no per-VU context across the FFI
boundary.

## Testing against a real server

The example harness brings up PostgreSQL and MySQL with a seeded `products`
table:

```bash
docker compose -f examples/harness/docker-compose.yml up -d postgres mysql

LOADR_TEST_POSTGRES_URL=postgres://loadr:loadr@127.0.0.1:5432/loadr \
LOADR_TEST_MYSQL_URL=mysql://loadr:loadr@127.0.0.1:3306/loadr \
  cargo test -p loadr-plugin-sql
```

The integration tests no-op when their connection env var is unset.
