# Flow control

Beyond a straight sequence of steps, a flow can loop, branch and choose at
random — covering Gatling's `repeat`/`during`/`asLongAs`/`doIf`/`randomSwitch`
and Locust's weighted-task model in declarative YAML.

## `repeat` — a fixed number of times

```yaml
flow:
  - repeat:
      times: 3
      counter: attempt          # 0-based loop index, readable from JS (default `index`)
      steps:
        - request: { url: /poll }
        - think_time: { type: constant, duration: 1s }
```

## `while` — as long as a condition holds

The condition is a JavaScript expression evaluated in the VU's runtime before
each pass. `max_iterations` (default 10000) prevents runaway loops.

```yaml
flow:
  - js: "session.vars.page = 0"
  - while:
      condition: "Number(session.vars.page) < 5"
      max_iterations: 20
      steps:
        - request: { url: "/feed?page=${page}" }
        - js: "session.vars.page = Number(session.vars.page) + 1"
```

## `if` / `else` — branch on a condition

```yaml
flow:
  - if:
      condition: "response && JSON.parse(session.vars.cart||'{}').items > 0"
      then:
        - request: { method: POST, url: /checkout }
      else:
        - request: { url: /cart/empty }
```

(`else` is optional.)

## `random` — weighted / uniform / round-robin branches

The headline Locust paradigm (`@task(weight)`) and Gatling's switches. Each
branch's samples are tagged with the branch name (or `branch-<n>`).

```yaml
flow:
  - random:
      strategy: weighted          # weighted (default) | uniform | round_robin
      choices:
        - weight: 70
          name: browse
          steps:
            - request: { url: /search?q=widget }
        - weight: 25
          name: add_to_cart
          steps:
            - request: { method: POST, url: /cart, body: { json: { sku: W-1 } } }
        - weight: 5
          name: checkout
          steps:
            - request: { method: POST, url: /checkout }
```

| Strategy | Behaviour |
|---|---|
| `weighted` | pick proportional to `weight` (default 1.0 each) — Gatling `randomSwitch`, Locust task weights |
| `uniform` | every branch equally likely — Gatling `uniformRandomSwitch` |
| `round_robin` | cycle through branches in order — Gatling `roundRobinSwitch` |

## Nesting

Control-flow steps nest arbitrarily — a `random` branch can contain a
`while`, a `repeat` can wrap an `if`, and `group` still tags everything inside.
This is how you model realistic user journeys: *browse 1–5 pages, then with
some probability add to cart, then maybe check out, retrying the payment up to
3 times.* See [`examples/16-flow-control.yaml`](https://github.com/reaandrew/loadr.io/blob/main/examples/16-flow-control.yaml).
