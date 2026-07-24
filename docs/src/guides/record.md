# Recording a session (loadr record)

You don't have to hand-write a test, and you don't have to export a HAR from
devtools either. `loadr record` starts a capturing proxy: point a browser, an
app, or `curl` at it, do the journey you want to load-test, and on `Ctrl-C` it
emits a ready-to-run scenario — with dynamic values (tokens, CSRF, ids)
**auto-correlated** by the same engine behind [`loadr convert har`](../migration/from-har.md).

## Quick start

```console
$ loadr record -o checkout.yaml
loadr record  recording proxy on 127.0.0.1:8888
  Point your client at it, e.g.:
    export HTTP_PROXY=http://127.0.0.1:8888 HTTPS_PROXY=http://127.0.0.1:8888
  Ctrl-C to stop and emit the scenario.
```

In another terminal, drive your journey through the proxy:

```console
$ curl -x http://127.0.0.1:8888 -X POST https://api.example.com/login -d '{"user":"alice"}'
$ curl -x http://127.0.0.1:8888 https://api.example.com/profile
```

Back in the first terminal, press `Ctrl-C`. loadr writes `checkout.yaml` and
tells you what it correlated:

```text
record: stopping — 2 transaction(s) captured
note: [request #1] auto-correlated `token` ($.token from the response) into 1 later request(s) — review it
record: wrote checkout.yaml
```

## Recording HTTPS

The proxy terminates TLS so it can see the plaintext (a man-in-the-middle on
**your own** traffic, on localhost). Trust its CA once:

```console
$ loadr record --trust
loadr record CA certificate:
  ~/.config/loadr/record-ca-cert.pem
  ...per-OS install instructions...
```

The CA is generated on first use and stored under `~/.config/loadr`
(`$XDG_CONFIG_HOME/loadr`). A fresh leaf certificate is minted per host on
demand and cached for the session. Remove the CA from your trust store when
you're done if you prefer — a new one is minted on demand next time.

## What you get

The emitted plan is a normal loadr scenario, so you can run it immediately and
then shape it into a real load test:

```yaml
scenarios:
  recorded:
    executor: constant-vus      # a safe default — set real load next
    vus: 1
    duration: 1m
    flow:
    - request:
        name: POST /login
        method: POST
        url: /login
        body: '{"user":"alice"}'
        extract:
        - type: jsonpath
          name: token
          expression: $.token        # ← auto-correlated
    - request:
        name: GET /profile
        method: GET
        url: /profile
        headers:
          authorization: Bearer ${token}   # ← substituted
```

Static assets (images, CSS, fonts) are dropped automatically so the scenario
stays focused on the API calls that matter.

## Options

| Flag | Meaning |
|------|---------|
| `-l, --listen <addr>` | Proxy listen address (default `127.0.0.1:8888`) |
| `-o, --output <path>` | Write the result here (default: stdout) |
| `--har` | Emit the raw HAR document instead of a scenario |
| `--trust` | Print the CA location + trust instructions and exit |
| `--ca-dir <dir>` | Override where the recorder CA is stored |

## Next steps

- Set a real `executor`, `vus`/`rate` and `duration` (see
  [Scenarios & executors](../yaml/scenarios-executors.md)).
- **Review the correlations** — the heuristic is good but not infallible.
- Add [thresholds](../yaml/thresholds.md) so the run has a pass/fail gate.
