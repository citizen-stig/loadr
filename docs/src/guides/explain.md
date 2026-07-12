# Explaining a run (loadr explain)

`loadr explain` reads a run's summary and gives you a plain-language root-cause
read — the threshold verdict, error rate, latency tail, and a heuristic *likely
cause* — without you squinting at a table of percentiles.

```console
$ loadr run plan.yaml --summary-export summary.json
$ loadr explain summary.json
loadr explain  checkout load
✗ 1 threshold(s) failed — the run did not meet its SLOs.
  ✗ http_req_duration: p99 < 300 (observed 3021.0)
✗ Error rate is 12.0% — a large fraction of requests failed; check the status/timeout breakdown.
• Latency: p50 50ms · p95 800ms · p99 3021ms.
! Heavy tail: p99 3021ms is 60× the median 50ms — a slow minority, not average slowness.
✗ Likely cause: past the knee — latency and errors climbed together, the signature of saturation. Reduce load or add capacity, then re-test.
```

## What it reads

- **Threshold verdict** — every failed threshold with its observed value.
- **Error rate** — flagged above a ~0.1% budget, strongly above 5%.
- **Latency tail** — when p99 is ≥5× the median, it calls out the slow minority
  (coordinated omission, GC pauses, lock contention, a cold path).
- **Likely cause** — a heuristic read:
  - latency **and** errors up together → saturation, past the knee;
  - tail latency **without** errors → a slow code path, not capacity;
  - clean → a healthy run.

## Generating a scenario from a description

The other half of the copilot goes the other way — natural language to a
*validated* plan:

```console
$ export ANTHROPIC_API_KEY=sk-...
$ loadr scenario "ramp to 500 rps on /checkout over 2m, hold 10m, p95 < 400ms" -o plan.yaml
$ loadr run plan.yaml
```

It sends your request plus loadr's own JSON Schema to the model, extracts the
YAML, and runs it through `loadr` validation — with one automatic **repair
round** if the first attempt doesn't validate, so what you get back is a plan
that actually loads. Pick the model with `--model`. (Provider-agnostic under the
hood; Anthropic today, set `ANTHROPIC_API_KEY`.)

## The deterministic reader

`loadr explain` is the offline path of the copilot: deterministic, no model, no network.
It works on any `loadr run --summary-export` file, including the ones your CI
already produces — pipe a regression straight into `loadr explain` for a
first-pass diagnosis in the PR.
