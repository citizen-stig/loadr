# Record-pass fixtures

The exact `/tmp/loadr-demo/*.yaml` plans the plugin videos were recorded with —
every one verified with a real `loadr run` (exit 0) against real local backends
before the camera rolled:

- `mock-sink.py` on 127.0.0.1:9999 (webhook/Slack/Datadog/OAuth/OTLP sink)
- go-httpbin on :8090 (`docker run -p 8090:8080 mccutchen/go-httpbin`)
- rig containers: redis:7, postgres:16, nats, mosquitto, cassandra:5, minio, vault

To re-record: stage these into `/tmp/loadr-demo/`, bring the rigs up, then run
the `plugin-<slug>.tape` files with `LOADR_BIN_DIR` pointing at a release binary.

Honest caveats live as comments inside the fixtures (e.g. core does not yet
start service plugins' endpoints — those demos show the equivalent flow and say
so on screen).
