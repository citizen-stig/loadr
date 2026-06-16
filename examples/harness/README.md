# Example harness

Run every example in `examples/` end-to-end against **real backend services**,
and print a pass/fail table.

```bash
scripts/run-examples.sh           # bring the stack up, run all 23, tear down
scripts/run-examples.sh --keep    # leave the services running afterwards
LOADR=/path/to/loadr scripts/run-examples.sh
```

## What it runs against

`docker-compose.yml` brings up real services — real products where one exists,
real protocol stacks otherwise:

| Service | Image / build | Protocols | Used by |
|---------|---------------|-----------|---------|
| `redis`   | `redis:7-alpine` (real Redis) | RESP | 30-redis (via loadr-plugin-redis) |
| `httpbin` | `mccutchen/go-httpbin` (real HTTP) | HTTP/1.1+2 | most HTTP examples |
| `greeter` | `./greeter` — real `grpcio` + reflection | gRPC | 10-grpc |
| `echo`    | `./echo` — real `websockets` + asyncio sockets + HTTP SSE | WS / SSE / TCP / UDP | 09, 12, 18 |

`15-distributed` is run against a real `loadr controller` + two real
`loadr agent` processes the script starts and stops.

The runner repoints each example's hosts at these services, shortens the
durations to a few seconds, and supplies harmless values for the secrets the
examples reference (`GRPC_API_KEY`, `GRAPHQL_TOKEN`, `INFLUX_TOKEN`,
`EXAMPLE_API_KEY`).

## Reading the table

- **PASS** — exit 0: ran and every threshold/check passed.
- **RAN\*** — exit 99: executed end-to-end, but a threshold or a content
  assertion failed. Usually because the assertion expects *that application's*
  data shape (e.g. a JSONPath into a specific response) that a generic server
  doesn't return, or a long-run threshold (e.g. `count>100`) under the
  shortened duration.
- **ERR(n)** — couldn't run; a real problem to fix.

The examples are realistic templates pointed at placeholder hosts; this harness
proves loadr's engine and every protocol handler work against real servers.
