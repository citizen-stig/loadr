# Exit codes

| Code | Meaning | Notes |
|---|---|---|
| `0` | success | run completed, every threshold passed |
| `1` | error | invalid test definition, I/O failure, connection to controller failed, ... |
| `99` | thresholds failed | run completed (or was aborted by `abort_on_fail`) |
| `130` | interrupted | second Ctrl-C (the first triggers a graceful stop with summary) |

CI example:

```yaml
- name: Load test gate
  run: loadr run -e ci --summary-export results.json --junit junit.xml perf/checkout.yaml
  # job fails automatically on exit 99

- name: Publish report
  if: always()
  run: loadr report results.json -o report.html
```

For a turnkey setup, use the first-party
[GitHub Action and JUnit report](../ci/github-actions.md) instead — it installs
loadr, runs the plan, and surfaces thresholds in the PR's test panel.
