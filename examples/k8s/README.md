# Distributed load testing on Kubernetes

Run a loadr fleet on Kubernetes — the loadr take on Artillery's
`k8s-testing-with-kubectl-artillery`. A controller coordinates N agent pods;
percentiles are merged centrally from HDR histograms (never averaged).

## Topology
- **controller** (`controller.yaml`) — one pod running `loadr controller`.
  Exposes `:7625` (agents join here) and `:6464` (run submission API).
- **agents** (`agents.yaml`) — a Deployment of agent pods that join the
  controller. Scale with `kubectl scale deployment/loadr-agents --replicas=N`.
- **run** (`run-job.yaml`) — a Job that submits `perf.yaml` (mounted from a
  ConfigMap) to the controller; the load is partitioned across all agents.

## Deploy
```bash
kubectl apply -f controller.yaml
kubectl apply -f agents.yaml
kubectl scale deployment/loadr-agents --replicas=10   # size the fleet
kubectl apply -f run-job.yaml                          # kick off the run
kubectl logs -f job/loadr-run                          # watch the merged summary
```
The Job's exit code reflects thresholds (0 pass / 99 breach), so it doubles as a
CI gate in cluster.

## Image
These manifests use `ghcr.io/levantar-ai/loadr:v1`. If you don't have access,
build your own with the included `Dockerfile` and push to your registry, then
replace the `image:` fields.
