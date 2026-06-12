# ADR-003: Coordination protocol — gRPC bidirectional stream

**Status**: accepted

## Context

Distributed mode needs: agent registration/health, shipping test definitions
+ data files, a synchronized start, live metric aggregation, control
(stop/pause/scale), and resilience to agent loss — over one
ops-friendly port, with optional mutual authentication.

## Decision

A single bidirectional gRPC stream per agent (`loadr.coordination.v1`,
tonic), protos compiled in-process with **protox** (no system protoc).
Heartbeats ride the stream; reconnection is jittered exponential backoff with
re-registration; the protocol carries an explicit version checked at
registration. TLS and mTLS via rustls.

Metric transport: each agent keeps a local aggregator and ships **deltas**
once per second — counters/rates as numeric deltas, trends as HDR-V2-encoded
delta histograms. The controller merges histograms; percentiles are computed
only after merging (see [metric aggregation](../distributed/metrics-merging.md)).

## Rationale

- One stream = one connection through firewalls/load balancers, natural
  ordering, cheap heartbeats, server push for control.
- gRPC/tonic gives typed evolution (proto), TLS/mTLS, and flow control for
  free; hand-rolled TCP framing or HTTP polling would re-invent all of it.
- Delta histograms bound bandwidth (KBs/second/agent regardless of request
  rate) while losing nothing statistically — vs raw sample shipping
  (unbounded) or pre-computed percentiles (mathematically wrong to combine).
- Synchronized start via a controller-stamped `start_unix_ms` barrier keeps
  multi-agent ramps aligned to within clock skew, which is sufficient for
  load shaping (we are not coordinating transactions).

## Consequences

- Agents are stateless and trivially scalable; the controller is a single
  point of coordination (acceptable: a run's lifetime is minutes/hours, and
  agents fail-safe by stopping load when orphaned).
- Data files travel in the assignment message — fine for the CSV/proto/script
  sizes tests actually use; huge corpora should live on the agents' disks.
