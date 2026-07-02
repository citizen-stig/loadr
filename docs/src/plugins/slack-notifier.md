# Slack notifier plugin

> **Status:** planned — not yet in the signed [plugin index](installing.md). This
> page documents the intended shape; the config keys and metric names may still
> change before the first release.

`loadr-plugin-slack-notifier` is an **output** plugin in the *outputs &
exporters* role. Unlike a streaming exporter (`prometheus`, `influxdb`,
`statsd`), it does not care about the sample stream at all: it **ignores every
in-run sample and snapshot** and posts a single **formatted summary** — pass/fail
verdict, p95 latency, error rate, and the threshold results — to a **Slack
incoming webhook** once the run finishes.

It runs in loadr's output pipeline: `start()` validates config and captures the
webhook URL, `on_samples()`/`on_snapshot()` are no-ops, and the message is built
and sent from `finish()`, which receives the final run summary (the same object
the JSON output writes as its `summary` record). The POST is a plain
**HTTPS request over [hyper](https://github.com/hyperium/hyper)** — loadr's own
HTTP stack — so there is no Slack SDK and no extra C dependency, and the plugin
is trivially buildable against the current core.

The contract it uses is documented in
[Developing a plugin](developing.md#outputs).

## Install

`slack-notifier` will ship in the signed [plugin index](installing.md), so once
released you install it by name — no build toolchain required:

```bash
loadr plugin install slack-notifier
loadr plugin info slack-notifier
```

Until then you can build and stage it from source like any native plugin:

```bash
cargo build -p loadr-plugin-slack-notifier --release

mkdir -p dist
cp plugins/loadr-plugin-slack-notifier/plugin.toml dist/
cp target/release/libloadr_plugin_slack_notifier.so dist/   # .dylib on macOS, .dll on Windows
loadr plugin install dist
```

Installing copies `plugin.toml` and the artifact into
`~/.loadr/plugins/slack-notifier/`. The manifest declares it as a native output:

```toml
[plugin]
name = "slack-notifier"
kind = "output"
type = "native"
entry = "libloadr_plugin_slack_notifier.so"
```

## Use it in a test

Add it to the plan's `outputs:` list as a `type: plugin` output and pass the
webhook URL in `config`. Keep the URL out of the plan file with an environment
variable:

```yaml
outputs:
  - type: plugin
    name: slack-notifier
    config:
      webhook_url: ${env.SLACK_WEBHOOK_URL}   # https://hooks.slack.com/services/…

scenarios:
  main:
    executor: constant-vus
    vus: 50
    duration: 5m
    flow:
      - request:
          name: homepage
          url: https://example.com/

thresholds:
  http_req_failed:   [ "rate<0.01" ]
  http_req_duration: [ "p(95)<250ms" ]
```

When the run ends, the plugin renders the summary and posts one message to the
webhook. Because it only acts in `finish()`, it adds no per-request overhead and
does not touch the hot path.

Ad hoc from the CLI, without editing the plan:

```bash
SLACK_WEBHOOK_URL=https://hooks.slack.com/services/… \
  loadr run --output plugin=slack-notifier test.yaml
```

The webhook URL still comes from `config`, so the `outputs:` form (or a
`config:` on the plugin entry) is the usual way to supply it.

## Config reference

Config is the JSON object handed to `start()`.

| Key           | Type    | Default        | Meaning |
|---------------|---------|----------------|---------|
| `webhook_url` | string  | *(required)*   | Slack **incoming webhook** URL (`https://hooks.slack.com/…`). A missing or empty value fails `start()`, so the plan is rejected before the run begins rather than silently dropping the notification. |

The message body is derived from the run summary and includes:

- the **pass/fail** verdict (did every threshold hold?),
- **p95** of `http_req_duration`,
- the **error rate** (`http_req_failed`),
- a line per **threshold** with its result.

## Metrics

The plugin exposes one internal counter so you can confirm delivery from the
run's own metrics:

| Metric                 | Kind    | Meaning |
|------------------------|---------|---------|
| `slack_messages_sent`  | counter | Incremented once per message successfully accepted by the webhook (Slack returns `200 ok`). |

A run that finishes cleanly posts exactly one message, so
`slack_messages_sent` is normally `1`. It stays `0` if the webhook rejects the
request or is unreachable.

## Notes

- **Summary only.** `on_samples()` and `on_snapshot()` are no-ops — pair this
  plugin with a streaming output (`prometheus`, `json`, …) when you also want
  the full time series; `slack-notifier` is purely the end-of-run heads-up.
- **Fire once, at the end.** The message is sent from `finish()`. If the run is
  killed before it completes, no message is posted.
- **Keep the URL secret.** A Slack incoming webhook URL is a credential — pass it
  via `${env.SLACK_WEBHOOK_URL}` (or `${secret.…}`) rather than committing it to
  the plan.
- **Failures don't fail the run.** A webhook error is logged and leaves
  `slack_messages_sent` at `0`; it does not change the run's exit code, which is
  still governed by `thresholds:`.
