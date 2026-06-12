# Metric aggregation

## The percentile trap

Most homegrown distributed setups report per-node percentiles and average
them. **That number is wrong** — often wildly. If agent A's p99 is 100 ms and
agent B's p99 is 1000 ms, the fleet's true p99 is *not* 550 ms; it depends on
the full shape of both distributions.

loadr never averages percentiles:

1. Every agent records trend metrics into **HDR histograms** (3 significant
   figures, auto-resizing).
2. Each second, the agent serializes a *delta* histogram (HDR V2 encoding)
   and streams it to the controller.
3. The controller **merges histograms** — a lossless operation — into a
   central aggregator per (metric, tag set).
4. Percentiles, thresholds, the live UI and the final summary are computed
   from the merged histograms only.

Counters and rates merge as exact sums (`passes`/`total`); gauges keep the
most recent value plus min/max envelopes.

This is verified by tests: two in-process agents record disjoint latency
ranges (1–1000 ms and 1001–2000 ms); the merged p99 must equal the true p99
of the union (~1980 ms), where naive averaging would claim ~1485 ms.

## Tags & per-agent visibility

Every sample an agent emits carries an `instance: <agent-name>` tag, so the
fleet view can show per-agent breakdowns and you can threshold per instance:

```yaml
thresholds:
  "http_req_duration{instance:agent-1}": [ "p(95)<500" ]
```

## Threshold evaluation

Thresholds run **centrally** against the merged data — `abort_on_fail`
decisions consider fleet-wide reality, then fan `stop` commands out to every
agent. Local evaluation on agents is disabled in distributed runs to avoid
split-brain aborts.
