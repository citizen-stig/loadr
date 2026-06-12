# Outputs

Outputs stream metrics out of a run — raw samples and/or one-second
aggregates. Configure any number:

```yaml
outputs:
  - { type: json, path: results.jsonl }             # newline-delimited JSON
  - { type: csv, path: samples.csv }
  - type: prometheus
    listen: 127.0.0.1:9091                          # scrape endpoint (GET /metrics)
    remote_write_url: http://prom:9090/api/v1/write # and/or push
    interval: 5s
  - type: influxdb
    url: http://influxdb:8086
    database: loadr                                  # bucket (v2) / db (v1)
    token: ${env.INFLUX_TOKEN}
    organization: my-org
  - type: otlp
    endpoint: http://otel-collector:4317
    protocol: grpc                                   # grpc | http
    headers: { x-tenant: load }
  - { type: statsd, address: 127.0.0.1:8125, prefix: loadr. }
  - { type: plugin, name: my-exporter, config: { mode: fast } }
```

Or ad hoc from the CLI: `loadr run --output json=results.jsonl test.yaml`.

| Output | Granularity | Notes |
|---|---|---|
| `json` | every sample + snapshots + final summary | one JSON object per line (`type` field discriminates) |
| `csv` | every sample | `timestamp_ms,metric,kind,value,tags` |
| `prometheus` | 1 s aggregates | metrics prefixed `loadr_`; trends as quantile gauges; counters as `_total` |
| `influxdb` | interval aggregates | line protocol, v1 and v2 APIs |
| `otlp` | interval aggregates | OpenTelemetry metrics over gRPC or HTTP/protobuf |
| `statsd` | every sample | DogStatsD-style tags |
| `plugin` | both | any installed output plugin |

The Grafana dashboard in
`deploy/grafana/dashboards/`
is pre-built against the Prometheus naming; `docker compose -f
deploy/docker-compose.yml up` gives you the full Prometheus + Grafana stack.

For end-of-run results in CI, prefer `--summary-export results.json` +
`loadr report results.json -o report.html`.
