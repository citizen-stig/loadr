# ADR-005: Metrics engine — HDR histograms over tagged series

**Status**: accepted

## Decision

Four k6-compatible metric kinds (Counter, Gauge, Rate, Trend). Samples carry
an interned metric name + an immutable tag set (`Arc<BTreeMap>`); the
aggregator keys series by (metric, tags). Trends record into HDR histograms —
3 significant figures, auto-resizing, values stored ×1000 (microsecond
resolution for millisecond metrics).

VUs emit samples over an unbounded mpsc to a single aggregator task, which
snapshots once per second (live UI/outputs/thresholds), supports tag-filtered
merged views for threshold selectors (`metric{tag:value}`), and produces
serializable deltas for distributed mode.

## Rationale

- **HDR histograms** give exact-enough percentiles (0.1% relative error) at
  fixed memory, O(1) record, and — the killer feature — **lossless merging**,
  which makes distributed percentiles correct and threshold evaluation on
  arbitrary `p(N)` cheap.
- Tag-set series (rather than pre-aggregated names) let one recording answer
  every slicing question later: per scenario, per request name, per status,
  per agent.
- A single aggregator task removes locking from the hot path; VUs only do an
  mpsc send with pre-interned `Arc` names/tags. At 1 Hz snapshot cadence the
  drain loop is far from saturation at realistic sample rates.

## Consequences

- Tag cardinality is the user's responsibility (request `name` defaults to
  the URL *template*, not the rendered URL, specifically to keep cardinality
  sane).
- Trends assume non-negative values (durations); custom trend metrics share
  that constraint.
