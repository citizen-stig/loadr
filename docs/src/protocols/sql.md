# SQL (PostgreSQL & MySQL)

Load-test a database directly. loadr connects to **PostgreSQL** or **MySQL**
with [`sqlx`](https://github.com/launchbadge/sqlx) (a pure-Rust async driver),
runs a configured query as the "request", and times it end to end — recording
query latency, the number of rows returned or affected, and any database error.

```yaml
- request:
    name: list cheap products
    url: postgres://loadr:loadr@db.example.com:5432/shop
    sql:
      query: SELECT id, name, price FROM products WHERE price < $1 ORDER BY price
      params: ["50"]
    checks:
      - { type: status, equals: 0 }                 # 0 = no DB error
      - { type: duration, name: query is fast, max: 50ms }
```

## When to use

Reach for this when the thing under test *is* the database: validating a schema
or index under write pressure, sizing a connection pool, finding the row count
at which a query falls over, or proving latency holds at a steady query rate.
For an application that merely *uses* a database behind an HTTP API, test the API
with the `http` handler instead.

## The target URL

```
postgres://[user[:password]@]host[:port][/database][?params]
mysql://[user[:password]@]host[:port][/database][?params]
```

- **scheme** selects the backend: `postgres` (alias `postgresql`) or `mysql`.
  You may also set `protocol: sql` explicitly and let the scheme route the
  driver.
- **port** defaults to the backend's standard port (`5432` / `3306`).
- **credentials, database, and query parameters** are passed straight to the
  driver, so any URL `sqlx` accepts works here — including `?sslmode=require`
  (PostgreSQL) or `?ssl-mode=REQUIRED` (MySQL) for TLS.

```yaml
url: postgres://loadr:loadr@127.0.0.1:5432/shop
url: mysql://loadr:loadr@db.internal/shop?ssl-mode=REQUIRED
```

## Expressing the query

The statement and its bind parameters go in the request's **`sql`** block:

```yaml
sql:
  query: SELECT name FROM products WHERE id = $1   # postgres placeholders
  params: ["${vu}"]                                # bound positionally
```

- **`query`** — the SQL to run. Use the backend's placeholder syntax for
  parameters: `$1, $2, …` for PostgreSQL and `?` for MySQL.
- **`params`** — positional bind values, bound *safely* by the driver (never
  string-spliced, so there is no SQL-injection surface). Each value is given as
  text; loadr infers a type so comparisons against numeric columns work — a
  value that parses as an integer binds as an integer, a decimal as a float,
  everything else as text.

`${...}` interpolation works in both `query` and `params`, so per-VU values and
data-feed columns flow straight into the statement:

```yaml
- request:
    name: insert order
    url: postgres://loadr:loadr@db/shop
    sql:
      query: INSERT INTO orders (sku, qty) VALUES ($1, $2)
      params: ["${row.sku}", "${row.qty}"]
```

As a shorthand, a request with no `sql` block uses its **`body`** as the query
text (no parameters). An empty query is rejected.

## Connection pooling

Connections are **pooled per virtual user**, keyed by the database URL:

- The first query from a VU to a given URL opens a `sqlx` pool (one live
  connection) and keeps it. This shows up in the timings as a non-zero
  `connect` phase.
- Every later query from that VU to the same URL **reuses** the open
  connection — reused requests report a zero `connect` phase.
- `sqlx` transparently re-establishes a connection that the server has dropped.

Pools are per-VU, so N virtual users hold up to N live connections per database
— size your scenario `vus` with the server's `max_connections` in mind.

## Status, rows, and errors

A request **succeeds** when the query executes without a database error:

| Outcome | `status` | `error` | `extras.rows` |
|---------|----------|---------|---------------|
| SELECT / WITH / SHOW … | `0` | — | rows returned |
| INSERT / UPDATE / DELETE | `0` | — | rows affected |
| database error (bad SQL, constraint, …) | non-zero | the DB message | — |
| connection failure / timeout | non-zero | the transport error | — |

`extras` carries:

- `extras.backend` — `postgres` or `mysql`.
- `extras.rows` — rows returned (row-producing statements) or affected (DML).

The response **body** is the row count rendered as text, so `body`-based checks
and extraction still work.

## Checks and assertions

```yaml
- request:
    name: count in-stock
    url: mysql://loadr:loadr@db/shop
    sql:
      query: SELECT COUNT(*) FROM products WHERE stock > ?
      params: ["0"]
    assert:
      - { type: status, equals: 0 }                 # query succeeded
    checks:
      - { type: duration, name: query is fast, max: 50ms }
```

- `status` — `equals: 0` requires the query to have run without a DB error.
- `duration` — cap the per-query round trip.

Checks are recorded to the `checks` metric and never fail the request; `assert`
entries mark the request failed (and can abort via `on_failure`).

## Timings & metrics

The handler measures the query lifecycle: the `connect` phase (first query to a
URL only) and `waiting` while the statement executes. `duration` is the total.

Alongside the standard `data_sent` / `data_received` series, the SQL family
adds:

| Metric | Kind | Meaning |
|--------|------|---------|
| `sql_reqs` | counter | queries executed |
| `sql_req_duration` | trend (time) | per-query latency |
| `sql_rows` | counter | total rows returned/affected |

Thresholds work as for any protocol:

```yaml
thresholds:
  checks: [ "rate>0.99" ]
  sql_req_duration: [ "p(95)<100ms" ]
```

A failed query also counts toward `http_req_failed`, the shared
failure-rate metric, so `http_req_failed` thresholds catch DB errors too.

## Trying it locally

The example harness ships PostgreSQL and MySQL with a seeded `products` table:

```sh
docker compose -f examples/harness/docker-compose.yml up -d postgres mysql
loadr run examples/27-sql.yaml
```
