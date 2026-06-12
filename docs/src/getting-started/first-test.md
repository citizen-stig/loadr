# Your first test

Create `first.yaml`:

```yaml
name: first-test
defaults:
  http:
    base_url: https://httpbin.org
    timeout: 10s

scenarios:
  smoke:
    executor: constant-vus
    vus: 5
    duration: 30s
    flow:
      - request:
          name: get anything
          url: /anything?hello=loadr
          checks:
            - { type: status, equals: 200 }
            - { type: jsonpath, name: echoed arg, expression: "$.args.hello", equals: loadr }
      - think_time: { type: uniform, min: 500ms, max: 1500ms }

thresholds:
  http_req_duration: [ "p(95)<2000" ]
  checks: [ "rate>0.99" ]
```

Validate it first — loadr's validator reports precise positions and suggests
fixes for typos:

```console
$ loadr validate first.yaml
✓ first.yaml is valid (1 scenario, 1 request)
```

Run it:

```console
$ loadr run first.yaml

  first-test — 1 scenario(s), 30.0s

  checks.....................: 100.00% — ✓ 214 ✗ 0
    ✓ status is 200 (107 / 107)
    ✓ echoed arg (107 / 107)
  http_req_duration..........: avg=312.44ms min=287.12ms med=305.81ms max=512.20ms p(90)=341ms p(95)=367ms p(99)=489ms
  http_reqs..................: 107 (3/s)
  iterations.................: 107 (3/s)
  vus........................: value=5 min=5 max=5

  thresholds:
    ✓ http_req_duration: p(95)<2000 (observed: 367.21)
    ✓ checks: rate>0.99 (observed: 1.00)
```

The exit code is `0` when all thresholds pass and `99` when any fail — wire it
straight into CI.

## What just happened

- `constant-vus` kept exactly 5 virtual users iterating for 30 seconds — a
  *closed* load model (new iterations start only when the previous finishes).
- Each iteration ran the `flow`: one HTTP request, two checks, then a random
  pause between 500 ms and 1.5 s.
- Checks record pass/fail into the `checks` metric **without** failing the
  request (use `assert:` for failures). The threshold over `checks` is what
  gates the run.

## Next steps

- Watch it live: `loadr run --ui first.yaml` then open `http://127.0.0.1:6464`.
- Save machine-readable results: `loadr run --summary-export results.json first.yaml`.
- Browse `examples/`
  — 15 runnable tests covering every feature.
