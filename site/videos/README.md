# Demo recordings

Terminal demos are [VHS](https://github.com/charmbracelet/vhs) tapes
(`NN-name.tape`); the Web-UI / report demos are Playwright recipes
(`record-*.js` / `record-*.sh`). Both render `.mp4` (+ a poster `.jpg`) into
`out/`, which is git-ignored and re-produced before a site deploy. The site
embeds them from `/videos/<name>.mp4`; `site/build-demos.py` maps a demo to its
recording via the `"video"` field.

## Recording a tape

Each tape `cd`s into `/tmp/loadr-demo`, puts the debug binary on `PATH`
(`$LOADR_BIN_DIR`) and runs a short, demo-tailored fixture staged there — the
same convention as the existing `01`–`22` tapes (their fixtures are staged at
record time, not committed). Bring up the example harness
(`examples/harness`) for the tapes that hit real services, stage the fixture,
then:

```bash
export LOADR_BIN_DIR=$PWD/target/debug
vhs site/videos/23-spike.tape        # → site/videos/out/23-spike.mp4
```

## Ready-to-record tapes (not yet wired into demos)

These 8 cover the highest-value demos that had no recording. They are **not**
referenced by `build-demos.py` yet — wire each one in (add `"video": "NN-name"`
to the matching demo) **after** its `.mp4` exists in `out/`, so no demo page
ships a broken player.

| tape | source example | fixture (staged) | target | demo slug to wire |
|------|----------------|------------------|--------|-------------------|
| `23-spike` | `04-spike-test.yaml` | `spike.yaml` | httpbin | `spike-test` |
| `24-correlation` | `06-correlation.yaml` | `correlation.yaml` | httpbin | `correlation` |
| `25-custom-metrics` | `39-custom-metrics.yaml` | `custom-metrics.yaml` | httpbin | `custom-metrics` |
| `26-scenario-weights` | `40-scenario-weights.yaml` | `weights.yaml` | httpbin | `scenario-weights` |
| `27-auth-tokens` | `36-auth-tokens.yaml` | `auth-tokens.yaml` | httpbin | `auth-tokens` |
| `28-websocket` | `09-websocket.yaml` | `websocket.yaml` | echo (harness) | `websocket` |
| `29-grpc` | `10-grpc.yaml` | `grpc.yaml` | greeter (harness) | `grpc` |
| `30-graphql` | `11-graphql.yaml` | `graphql.yaml` | httpbin | `graphql` |

Still un-recorded and worth doing as Playwright recipes (browser, not a tape):
the **browser** demo and the **Prometheus + Grafana** dashboard.
