# Data parameterization

Feed iterations from CSV files or inline rows. A row is consumed **once per
iteration per source** (the first reference fetches it; later references in
the same iteration see the same row).

```yaml
data:
  users:
    type: csv
    path: data/users.csv     # relative to the test file
    mode: shared             # shared | per_vu
    on_eof: recycle          # recycle | stop
    delimiter: ","           # default ,
    has_header: true         # default true; otherwise columns are col0, col1, ...
  fixtures:
    type: inline
    rows:
      - { sku: W-1, qty: 1 }
      - { sku: W-2, qty: 3 }

scenarios:
  buy:
    executor: per-vu-iterations
    vus: 5
    iterations: 100
    flow:
      - request:
          method: POST
          url: /cart
          body: { form: { user: "${data.users.username}", sku: "${data.fixtures.sku}" } }
```

## Modes

- **`shared`** — one cursor for the whole run; VUs pull the next row
  atomically. Rows are spread across VUs (each row used once per lap).
- **`per_vu`** — every VU iterates the full data set from the top
  independently.

## End of data

- **`recycle`** — wrap to the first row (default).
- **`stop`** — the VU that hits EOF stops iterating (JMeter's
  "stop thread on EOF"). With shared mode this winds the test down as the
  data runs out — handy for "process each row exactly once" jobs.

From JS, fetch the current row with `session.data('users')` →
`{username: "...", password: "..."}`.
