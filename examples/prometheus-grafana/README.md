# Prometheus + Grafana dashboards

Watch a loadr run live in Grafana — the loadr take on Artillery's
`prometheus-grafana-dashboards`.

## Files
- `loadr.yaml` — a test that exposes a Prometheus scrape endpoint (`:9091`).
- `docker-compose.yml` — Prometheus + Grafana (anonymous admin).
- `prometheus.yml` — scrape config pointing at loadr on the host.
- `grafana-dashboard.json` — a starter dashboard (req/s, avg duration, VUs, check rate).

## Run it
```bash
cd examples/prometheus-grafana
docker compose up -d                 # Prometheus :9090, Grafana :3000
loadr run loadr.yaml                 # exposes metrics on :9091, scraped every 5s
```
Open Grafana at http://localhost:3000 → **Dashboards → Import** →
upload `grafana-dashboard.json` → pick the Prometheus datasource
(http://prometheus:9090).

## Metrics loadr exposes
`loadr_http_reqs_total`, `loadr_http_req_duration_milliseconds` (`_sum`/`_count`),
`loadr_checks_rate`, `loadr_checks_passes_total`, `loadr_vus`, and your custom
metrics as `loadr_m_<name>` (see `../39-custom-metrics.yaml`).

## Push instead of scrape
Prefer remote-write (e.g. to Grafana Cloud / Mimir)? Swap the output:
```yaml
outputs:
  - { type: prometheus, remote_write_url: "https://<prom>/api/v1/write" }
```

## Going further: `observe`
Scraping shows loadr's own numbers. loadr's `observe` feature goes the other way
— it pulls your **server's** Prometheus metrics after a run and overlays them on
the request timeline, and can fail the run on server-side thresholds (CPU,
saturation). See the docs `observe` chapter — it's the piece most load testers
don't have.
