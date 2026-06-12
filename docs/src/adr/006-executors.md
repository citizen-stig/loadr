# ADR-006: Executor model — k6's seven, open and closed

**Status**: accepted

## Decision

Implement k6's full executor set with matching semantics:
`constant-vus`, `ramping-vus`, `per-vu-iterations`, `shared-iterations`
(closed models — VU loops drive iterations), `constant-arrival-rate`,
`ramping-arrival-rate` (open models — an arrival clock starts iterations on
schedule), and `externally-controlled`.

Open-model mechanics: a dispatcher integrates the (possibly ramping) rate
function and fires iteration starts at idle workers; if none is idle it grows
the pool up to `max_vus`, beyond which it records `dropped_iterations`
instead of queueing. Closed-model ramping uses a watch channel of "allowed
VUs" with linear interpolation at 100 ms resolution; de-allocated VUs get
`graceful_ramp_down` to finish in-flight iterations.

## Rationale

- The open/closed distinction is the single most important correctness
  property in load generation: closed models suffer coordinated omission
  (a slow server *reduces* offered load, hiding the problem). Users must be
  able to choose, and k6's executor vocabulary is the de-facto standard.
- **Dropping, not queueing**, when starved at `max_vus` keeps the offered
  rate honest and makes saturation *visible* as a first-class metric.
- k6-identical names and parameters make migration mechanical (the converter
  maps `options.scenarios` 1:1) and documentation transferable.

## Consequences

- Every executor funnels through one `run_iteration` path (flow + exec +
  metrics + outcome handling), so features like pause, graceful stop, data
  exhaustion and abort actions behave identically everywhere.
- Distributed partitioning is a pure function over executor specs
  (VUs/iterations split with remainders, rates divided exactly) — unit-tested
  invariant: partitions always sum to the original.
