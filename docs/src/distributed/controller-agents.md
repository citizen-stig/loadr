# Controller & agents

## The coordination protocol

Controller and agents speak `loadr.coordination.v1` — a single bidirectional
gRPC stream per agent:

```text
agent ──▶ Register{agent_id, name, protocol_version, cores, labels}
      ◀── Registered{controller_id}
      ◀── Assignment{run_id, plan_yaml, partition i/n, data files}
      ◀── Start{run_id, start_unix_ms}          # synchronized barrier
      ──▶ MetricsBatch{run_id, delta}           # every second
      ──▶ Heartbeat{active_vus, run_state}      # every 2 seconds
      ◀── Control{stop|kill|pause|resume|scale}
      ──▶ RunEvent{started|finished|failed, summary}
```

The protocol is versioned; an agent with an incompatible
`protocol_version` is rejected at registration.

## TLS / mTLS

```bash
loadr controller --bind 0.0.0.0:7625 \
  --tls-cert server.pem --tls-key server-key.pem \
  --tls-client-ca clients-ca.pem          # require client certs (mTLS)

loadr agent --join ctrl:7625 \
  --tls-ca ca.pem \
  --tls-cert agent.pem --tls-key agent-key.pem
```

Without flags the channel is plaintext — fine on a private network, not on
the internet.

## Failure handling

- **Heartbeats** every 2 s; an agent silent past the liveness window
  (default 6 s) is marked unhealthy.
- **Reconnection**: agents reconnect with jittered exponential backoff and
  re-register, resuming their identity.
- **Agent loss during a run** is policy-driven per submission:
  - `continue` (default) — remaining agents keep their share; the lost
    agent's portion of the load simply stops (the summary notes the
    reduced fleet).
  - `abort` — the controller stops the run everywhere.

## Data files

CSV files, JS modules, proto files and body files referenced by the test are
shipped inside the assignment and materialized in the agent's working
directory. Paths are sanitized — anything containing `..` or absolute paths
is rejected.

## Operating notes

- Agents are stateless; scale them with your orchestrator
  (`kubectl scale deploy/loadr-agent --replicas=20`).
- One controller handles many sequential/concurrent runs; each run records
  its agent set at submission time.
- The web UI on the controller shows the fleet (health, VUs, labels,
  last heartbeat) and every run's live metrics.
