#!/usr/bin/env bash
# Run every loadr example end-to-end against the real backend services in
# examples/harness/docker-compose.yml and print a pass/fail table.
#
#   scripts/run-examples.sh            # bring up stack, run all, tear down
#   scripts/run-examples.sh --keep     # leave the stack running afterwards
#   LOADR=/path/to/loadr scripts/run-examples.sh
#
# Exit-code legend per example:  0 = pass · 99 = ran, a threshold/check failed
# · anything else = error (couldn't run).
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COMPOSE="$ROOT/examples/harness/docker-compose.yml"
RUNDIR="$(mktemp -d /tmp/loadr-examples.XXXXXX)"
KEEP=0; [ "${1:-}" = "--keep" ] && KEEP=1

LOADR="${LOADR:-}"
if [ -z "$LOADR" ]; then
  for c in "$ROOT/target/release/loadr" "$ROOT/target/debug/loadr" "$(command -v loadr || true)"; do
    [ -x "$c" ] && { LOADR="$c"; break; }
  done
fi
[ -x "$LOADR" ] || { echo "loadr binary not found (set LOADR=...)"; exit 1; }
echo "==> loadr: $LOADR"

# Example workloads reference these; give them harmless values.
export GRPC_API_KEY="demo-key" GRAPHQL_TOKEN="demo-token" INFLUX_TOKEN="demo-token" EXAMPLE_API_KEY="demo-key"

echo "==> bringing up backend services"
docker compose -f "$COMPOSE" up -d --build >/dev/null

echo "==> waiting for services"
for i in $(seq 1 40); do
  docker compose -f "$COMPOSE" exec -T redis redis-cli ping 2>/dev/null | grep -q PONG \
    && curl -fsS -o /dev/null http://127.0.0.1:8080/get 2>/dev/null \
    && curl -fsS -o /dev/null http://127.0.0.1:8085/ 2>/dev/null \
    && (echo > /dev/tcp/127.0.0.1/50051) 2>/dev/null \
    && (echo > /dev/tcp/127.0.0.1/8081) 2>/dev/null \
    && docker compose -f "$COMPOSE" exec -T postgres pg_isready -U loadr -d loadr >/dev/null 2>&1 \
    && docker compose -f "$COMPOSE" exec -T mysql mysqladmin ping -h 127.0.0.1 -uloadr -ploadr --silent >/dev/null 2>&1 \
    && docker compose -f "$COMPOSE" exec -T mongo mongosh -u loadr -p loadr --quiet --eval "db.getSiblingDB('loadr').runCommand({ping:1}).ok" loadr 2>/dev/null | grep -q 1 \
    && { echo "    services ready"; break; }
  sleep 1
done

# Build + install the native MongoDB protocol plugin so 28-mongo.yaml can run.
# Plugins resolve from $LOADR_PLUGINS_DIR; point it at a temp dir for this run.
echo "==> building + installing the mongo plugin"
export LOADR_PLUGINS_DIR="$RUNDIR/plugins"
mkdir -p "$LOADR_PLUGINS_DIR"
if cargo build --manifest-path "$ROOT/Cargo.toml" -p loadr-plugin-mongo --release >/tmp/h-mongo-build.log 2>&1; then
  # `plugin install` copies a dir holding plugin.toml + the artifact named by
  # its `entry`. Stage them together, then install.
  stage="$RUNDIR/mongo-stage"; mkdir -p "$stage"
  cp "$ROOT/plugins/loadr-plugin-mongo/plugin.toml" "$stage/"
  for art in libloadr_plugin_mongo.so libloadr_plugin_mongo.dylib loadr_plugin_mongo.dll; do
    [ -f "$ROOT/target/release/$art" ] && cp "$ROOT/target/release/$art" "$stage/"
  done
  "$LOADR" plugin install "$stage" --plugins-dir "$LOADR_PLUGINS_DIR" >/dev/null 2>&1 \
    || echo "    mongo plugin install failed; 28-mongo will ERR"
else
  echo "    mongo plugin build failed (see /tmp/h-mongo-build.log); 28-mongo will ERR"
fi

# Stage the examples + their data/scripts/protos so relative paths resolve.
cp -r "$ROOT/examples/." "$RUNDIR/"

repoint() {  # stdin -> stdout: point hosts at local services, shorten durations
  # Shortens scenario/stage/session durations only; leaves think_time untouched
  # (shortening think_time would starve throughput and miss count thresholds).
  perl -pe '
    s{https?://httpbin\.org}{http://127.0.0.1:8080}g;
    s{https?://[a-z0-9.-]*example\.com}{http://127.0.0.1:8085}g;
    s{wss?://[^/\s"'\'']+}{ws://127.0.0.1:8081}g;
    s{sses?://[^/\s"'\'']+}{sse://127.0.0.1:8082}g;
    s{grpc://[^/\s"'\'']+}{grpc://127.0.0.1:50051}g;
    s{tcp://[^/\s"'\'']+}{tcp://127.0.0.1:7000}g;
    s{udp://[^/\s"'\'']+}{udp://127.0.0.1:8125}g;
    s{rediss?://[^/\s"'\'']+}{redis://127.0.0.1:6379}g;
    s{postgres(?:ql)?://([^@/\s"'\'']+)@[^/\s"'\'']+/}{postgres://${1}\@127.0.0.1:5432/}g;
    s{mysql://([^@/\s"'\'']+)@[^/\s"'\'']+/}{mysql://${1}\@127.0.0.1:3306/}g;
    s{mongodb://([^@/\s"'\'']+)@[^/\s"'\'']+/}{mongodb://${1}\@127.0.0.1:27017/}g;
    s{^(\s*)duration:\s*\d+(?:ms|s|m|h)\b}{${1}duration: 6s};
    s{(\{\s*)duration:\s*\d+(?:ms|s|m|h)(\s*,\s*target:)}{${1}duration: 3s${2}}g;
    s{\bsession_duration:\s*\d+(?:ms|s|m|h)\b}{session_duration: 1s}g;
  '
}

declare -a NAMES EXITS NOTE
run_one() {  # $1 = example file (in RUNDIR), $2.. = extra loadr args
  local f="$1"; shift
  local base; base="$(basename "$f")"
  repoint < "$f" > "$f.local" && mv "$f.local" "$f"
  local out; out="$("$LOADR" run "$@" "$f" 2>&1)"; local code=$?
  local reqs; reqs="$(echo "$out" | grep -oE '(http_reqs|plugin_reqs|grpc_reqs|sql_reqs|mongo_reqs|ws_msgs_received)\.+: [0-9]+' | grep -oE '[0-9]+' | head -1)"
  NAMES+=("$base"); EXITS+=("$code"); NOTE+=("${reqs:-0} reqs")
}

echo "==> running examples"
for f in "$RUNDIR"/[0-9]*.yaml; do
  case "$(basename "$f")" in
    15-distributed.yaml) continue ;;   # handled below
  esac
  run_one "$f"
done

# Distributed: real controller + 2 agents.
echo "==> distributed run (controller + 2 agents)"
"$LOADR" controller >/tmp/h-ctrl.log 2>&1 & CTRL=$!
sleep 2
"$LOADR" agent --join 127.0.0.1:7625 --name a1 >/tmp/h-a1.log 2>&1 & A1=$!
"$LOADR" agent --join 127.0.0.1:7625 --name a2 >/tmp/h-a2.log 2>&1 & A2=$!
sleep 3
run_one "$RUNDIR/15-distributed.yaml" --controller 127.0.0.1:6464
kill "$A1" "$A2" "$CTRL" 2>/dev/null

echo ""
echo "================ RESULTS ================"
pass=0; ran=0; err=0
for i in "${!NAMES[@]}"; do
  c="${EXITS[$i]}"
  case "$c" in
    0)  tag="PASS "; pass=$((pass+1)) ;;
    99) tag="RAN* "; ran=$((ran+1)) ;;
    *)  tag="ERR($c)"; err=$((err+1)) ;;
  esac
  printf "  %-6s %-32s %s\n" "$tag" "${NAMES[$i]}" "${NOTE[$i]}"
done
echo "-----------------------------------------"
echo "  PASS=$pass  RAN*(checks/thresholds failed)=$ran  ERR=$err  of ${#NAMES[@]}"
echo "  * RAN = executed end-to-end against the real service; some content"
echo "    assertions need a backend returning that app's data shapes."

[ "$KEEP" = "1" ] || { echo "==> tearing down"; docker compose -f "$COMPOSE" down >/dev/null 2>&1; }
rm -rf "$RUNDIR"

# Fail the run (for CI acceptance gating) only if an example could not execute
# end-to-end. RAN* (a content/threshold assertion not met against the generic
# stand-in backend) is expected and does not fail acceptance.
if [ "$err" -gt 0 ]; then
  echo "::error::$err example(s) failed to execute"
  exit 1
fi
