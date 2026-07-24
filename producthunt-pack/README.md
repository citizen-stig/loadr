# loadr — Product Hunt launch pack

Everything needed to submit **loadr** to Product Hunt: copy, gallery images, a
thumbnail, and curated demo videos. Copy/paste the text fields, upload the
assets in the order below.

- **Website:** https://loadr.io
- **Demos:** https://loadr.io/demos/
- **Repo:** https://github.com/levantar-ai/loadr
- **Maker:** Levantar

---

## 1. Core fields

**Name**
```
loadr
```

**Tagline** (≤ 60 chars — pick one)
```
Find the breaking point — load testing in one Rust binary
```
Alternatives:
- `The open load tester that finds your breaking point`
- `Load testing with exact percentiles, in one binary`

**Description** (≤ 260 chars)
```
loadr brings together the best ideas from k6, JMeter, Gatling and Locust in one
Rust binary: declarative YAML tests, embedded JavaScript, six protocols,
plugins, a live web UI, distributed runs, and mathematically exact percentiles.
```

**Topics**
```
Developer Tools · GitHub · Open Source (source-available, Elastic-2.0) · Tech
```

**Links**
- Website → https://loadr.io
- GitHub → https://github.com/levantar-ai/loadr

---

## 2. Gallery (upload in this order)

Product Hunt's first gallery slot can be a **video** — use the hero video
below (upload to YouTube/Vimeo and paste the link, or upload the mp4). The rest
are 1270×760 images, ready to upload as-is.

| # | File | Caption (paste under the image) |
|---|------|--------------------------------|
| ▶ | `videos/01-feature-tour.mp4` (hero, 18s) | A 60-second tour: write a test, run it, read the result. |
| 1 | `gallery/01-overview.png` | One binary. Declarative YAML tests, exact percentiles, live threshold checks. |
| 2 | `gallery/02-web-ui.png` | Built-in live web UI (`loadr run --ui`) — author, run and watch tests in the browser. |
| 3 | `gallery/03-timeseries-report.png` | Rich HTML reports with time-series graphs of latency, throughput and errors. |
| 4 | `gallery/04-failure-breakdown.png` | Failure breakdown that groups errors so you see what actually broke. |
| 5 | `gallery/05-ci-gate.png` | Gate CI on your SLOs — loadr exits non-zero when thresholds fail. Drop-in GitHub Action. |
| 6 | `gallery/06-distributed-fleet.png` | Scale out: distribute load across an agent fleet from a single command. |
| 7 | `gallery/07-plugins.png` | Native plugins — Postgres, MySQL, Mongo, Redis, Kafka, RabbitMQ, Elasticsearch. |
| 8 | `gallery/08-flow-control.png` | Flow control: stages, ramps, think time, feeders and throttles for realistic load. |

**Desktop app (Electron GUI)** — captured from the real app on `main`:

| # | File | Caption |
|---|------|---------|
| 9 | `gallery/09-desktop-compose.png` | Desktop app: compose load tests in a forms-first editor — no YAML required. |
| 10 | `gallery/10-desktop-run.png` | Run with the bundled engine and watch it live — req/s, p95, error rate, throughput. |
| 11 | `gallery/11-desktop-plugins.png` | Manage signed protocol plugins (Postgres, Mongo, Redis, Kafka…) from the GUI. |
| 12 | `gallery/12-desktop-split.png` | Split view: forms on one side, live Monaco YAML on the other — they stay in sync. |

**Desktop video:** `videos/10-desktop-app.mp4` (8s) — compose → run → live results in the desktop app.

**Thumbnail / logo:** `thumbnail-240.png` (240×240). Larger logo: `logo-1024.png`.

**Extra videos** you can swap in or add to a YouTube playlist:
`07-quickstart`, `08-convert-k6-jmeter`, `09-postgres-plugin`, plus
`03/04/05/06` mirror the gallery stills in motion.

---

## 3. Maker's first comment (paste as your launch comment)

```
Hey Product Hunt 👋

loadr started as an experiment. While Anthropic's Claude Fable 5 was briefly
available, we gave it one carefully-structured prompt: build a load-testing
tool in Rust that combines the best ideas from four popular tools — k6, JMeter,
Gatling and Locust — with a plugin architecture, embedded scripting, and
distributed execution.

What came back genuinely surprised us. Not a toy or a sketch — a coherent,
architecturally sound foundation. It even generated short demo videos of parts
of it working. So we kept building on it. Today loadr is:

⚡ One Rust binary (Tokio + hyper) — no runtime to install
📝 Declarative YAML tests, with embedded JavaScript when you need real logic
📊 Mathematically exact percentiles (HDR histograms merged across threads — p99.9 is real, not sampled)
🔌 A native plugin system: Postgres, MySQL, Mongo, Redis, Kafka, RabbitMQ, Elasticsearch
🖥️ A live web UI compiled into the binary (loadr run --ui) + a desktop app
🚦 CI-native: gate releases on SLOs, drop-in GitHub Action
🌐 Distributed runs across an agent fleet
🔭 observe: pull your server's Prometheus metrics after a run and overlay them on
   the request timeline — so "p95 spiked" lines up with "the DB hit 90% CPU"

Honest notes: it's source-available under Elastic-2.0 (not OSI open source), and
it's still beta — HTTP/gRPC are solid, the browser-driven path is newer.

The thing we found most interesting wasn't "AI wrote it" — it's how far one
well-structured prompt could compress the gap between idea and a real, working
foundation. Happy to go deep on the QuickJS-per-VU design, the HDR merge, or the
original prompt in the comments.

Try it: https://loadr.io  ·  Demos: https://loadr.io/demos/
```

---

## 4. Pre-launch checklist

- [ ] Pick the tagline variant; confirm topics.
- [ ] Upload hero video to YouTube/Vimeo (or upload `videos/01-feature-tour.mp4`).
- [ ] Upload gallery images 1–8 in order; paste captions.
- [ ] Set thumbnail (`thumbnail-240.png`).
- [ ] Schedule for **12:01am PT** (PH resets daily — early = full day of votes).
- [ ] Line up the maker comment; be available to reply for the first 2–3 hrs.
- [ ] First comment posted the moment it goes live.

## 5. Asset gaps (optional polish)

- **observe / metric-correlation** screenshot (the Prometheus timeline overlay)
  — arguably loadr's most differentiated feature; no demo video exists yet (it
  post-dates the recordings). Needs a run against a live Prometheus target.

Web UI **and** desktop app are both captured (images 1–8 web UI, 9–12 desktop,
plus a desktop video). Ask and I can capture `observe` if you can point loadr at
a Prometheus instance, or re-record the demos to include it.
