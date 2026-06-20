#!/usr/bin/env bash
# Run a simple, fair benchmark of loadr vs k6, JMeter, Gatling and Locust
# against the same Dockerized target, one tool at a time, and print a comparison
# table. Each tool runs the identical closed-model scenario:
#   BENCH_VUS concurrent users, BENCH_DURATION, GET /json.
#
#   ./run.sh                 # all tools, defaults (50 VUs, 30s)
#   TOOLS="loadr k6" ./run.sh
#   BENCH_VUS=100 BENCH_DURATION=60s BENCH_DURATION_S=60 ./run.sh
#
# Requires: docker (host networking → Linux), and a loadr binary ($LOADR_BIN,
# else ../target/{release,debug}/loadr, else `cargo build`).
set -uo pipefail
cd "$(dirname "$0")"
ROOT="$(pwd)"

# ---- config ---------------------------------------------------------------
VUS="${BENCH_VUS:-50}"
DURATION="${BENCH_DURATION:-30s}"
DURATION_S="${BENCH_DURATION_S:-30}"
HOST_URL="http://localhost:18080"
URL="$HOST_URL/json"
TOOLS="${TOOLS:-${*:-loadr k6 jmeter gatling locust}}"

K6_IMAGE="${K6_IMAGE:-grafana/k6:0.54.0}"
LOCUST_IMAGE="${LOCUST_IMAGE:-locustio/locust:2.31.8}"
JMETER_IMAGE="${JMETER_IMAGE:-justb4/jmeter:5.5}"
MAVEN_IMAGE="${MAVEN_IMAGE:-maven:3.9-eclipse-temurin-21}"

RESULTS="$ROOT/results"
rm -rf "$RESULTS"; mkdir -p "$RESULTS"/{loadr,k6,k6-tuned,locust,locust-tuned,jmeter,jmeter-tuned,gatling}
# Run tool containers as the host user so files written to the mounted result
# dirs are owned by us (and writable / cleanable next run).
DUSER="$(id -u):$(id -g)"
mkdir -p "$ROOT/.cache-m2"

say() { printf '\n\033[1;31m▶ %s\033[0m\n' "$*"; }

# ---- target ---------------------------------------------------------------
say "Building & starting the target"
docker compose up -d --build || { echo "target failed to start"; exit 1; }
trap 'docker compose down >/dev/null 2>&1 || true' EXIT

printf 'waiting for target'
for _ in $(seq 1 60); do
  curl -sf "$HOST_URL/healthz" >/dev/null 2>&1 && { echo " ready"; break; }
  printf '.'; sleep 1
done
curl -sf "$HOST_URL/healthz" >/dev/null || { echo " target never became ready"; exit 1; }
# brief warmup so the first tool isn't penalised by a cold target
for _ in $(seq 1 200); do curl -s "$URL" >/dev/null 2>&1; done

# ---- render scenario templates -------------------------------------------
export BENCH_URL="$URL" BENCH_VUS="$VUS" BENCH_DURATION="$DURATION"
SUBST='${BENCH_URL} ${BENCH_VUS} ${BENCH_DURATION}'
envsubst "$SUBST" < scenarios/loadr/plan.yaml.tmpl > "$RESULTS/loadr/plan.yaml"
envsubst "$SUBST" < scenarios/k6/script.js.tmpl       > "$RESULTS/k6/script.js"
envsubst "$SUBST" < scenarios/k6/script-tuned.js.tmpl > "$RESULTS/k6-tuned/script.js"
cp scenarios/locust/locustfile.py        "$RESULTS/locust/"
cp scenarios/locust/locustfile-tuned.py  "$RESULTS/locust-tuned/"
cp scenarios/jmeter/plan.jmx             "$RESULTS/jmeter/"
cp scenarios/jmeter/plan.jmx scenarios/jmeter/user.properties "$RESULTS/jmeter-tuned/"

ran=()
fail=()
runtool() { # name + command; record success/failure, never abort the suite
  local name="$1"; shift
  say "Running $name ($VUS VUs, $DURATION)"
  if "$@"; then ran+=("$name"); else echo "✗ $name failed"; fail+=("$name"); fi
}

# ---- loadr (the binary under test, on the host) ---------------------------
resolve_loadr() {
  if [ -n "${LOADR_BIN:-}" ] && [ -x "$LOADR_BIN" ]; then echo "$LOADR_BIN"; return; fi
  for p in "$ROOT/../target/release/loadr" "$ROOT/../target/debug/loadr"; do
    [ -x "$p" ] && { echo "$p"; return; }
  done
  return 1
}
run_loadr() {
  local bin
  if ! bin="$(resolve_loadr)"; then
    say "Building loadr (release)"; ( cd "$ROOT/.." && cargo build --release -p loadr-cli ) || return 1
    bin="$ROOT/../target/release/loadr"
  fi
  "$bin" run "$RESULTS/loadr/plan.yaml" --summary-export "$RESULTS/loadr/summary.json"
}

run_k6() {
  docker run --rm --network host --user "$DUSER" -v "$RESULTS/k6:/work" -w /work "$K6_IMAGE" \
    run --vus "$VUS" --duration "$DURATION" --summary-export=summary.json script.js
}

# Tuned k6: discard bodies + minimal tags (in the script), no thresholds, no
# usage report. (--compatibility-mode=base is a no-op on k6 ≥0.53, so dropped.)
run_k6_tuned() {
  docker run --rm --network host --user "$DUSER" -v "$RESULTS/k6-tuned:/work" -w /work "$K6_IMAGE" \
    run --vus "$VUS" --duration "$DURATION" --no-thresholds --no-usage-report \
    --summary-export=summary.json script.js
}

run_locust() {
  docker run --rm --network host --user "$DUSER" -e HOME=/tmp -v "$RESULTS/locust:/work" -w /work "$LOCUST_IMAGE" \
    -f locustfile.py --headless -u "$VUS" -r "$VUS" -t "$DURATION" \
    --host "$HOST_URL" --csv locust --only-summary
}

# Tuned Locust: FastHttpUser + one worker process per core (--processes -1).
run_locust_tuned() {
  docker run --rm --network host --user "$DUSER" -e HOME=/tmp -v "$RESULTS/locust-tuned:/work" -w /work "$LOCUST_IMAGE" \
    -f locustfile-tuned.py --headless --processes -1 -u "$VUS" -r "$VUS" -t "$DURATION" \
    --host "$HOST_URL" --csv locust --only-summary --loglevel WARNING
}

run_jmeter() {
  docker run --rm --network host --user "$DUSER" -v "$RESULTS/jmeter:/work" -w /work "$JMETER_IMAGE" \
    -n -t plan.jmx -l result.jtl \
    -Jthreads="$VUS" -Jrampup=0 -Jduration="$DURATION_S" \
    -Jhost=localhost -Jport=18080 -Jpath=/json
}

# Tuned JMeter: connection reuse across iterations + trimmed JTL (user.properties)
# and a fixed 2G heap (JVM_XMS/JVM_XMX, in MB, read by the image entrypoint).
run_jmeter_tuned() {
  docker run --rm --network host --user "$DUSER" -e JVM_XMS=2048 -e JVM_XMX=2048 \
    -v "$RESULTS/jmeter-tuned:/work" -w /work "$JMETER_IMAGE" \
    -n -t plan.jmx -q user.properties -l result.jtl \
    -Jthreads="$VUS" -Jrampup=0 -Jduration="$DURATION_S" \
    -Jhost=localhost -Jport=18080 -Jpath=/json
}

run_gatling() {
  # The gatling-maven-plugin always writes to target/gatling; copy that report
  # out to the mounted /results afterwards. Bench params come via env (cleaner
  # than nested shell quoting). Results to a top-level /results mount, not nested
  # under target/ (a bind there would be root-owned and break compilation).
  docker run --rm --network host --user "$DUSER" -e HOME=/tmp \
    -e G_URL="$HOST_URL" -e G_USERS="$VUS" -e G_DUR="$DURATION_S" \
    -v "$ROOT/scenarios/gatling:/proj" -v "$RESULTS/gatling:/results" \
    -v "$ROOT/.cache-m2:/m2" -w /proj --entrypoint bash "$MAVEN_IMAGE" -c '
      mvn -q -B -Dmaven.repo.local=/m2 gatling:test \
        -Dgatling.simulationClass=bench.BenchSimulation \
        -Dbench.url="$G_URL" -Dbench.users="$G_USERS" -Dbench.duration="$G_DUR" \
      && cp -r target/gatling/. /results/'
}

for t in $TOOLS; do
  case "$t" in
    loadr)         runtool loadr         run_loadr ;;
    k6)            runtool k6            run_k6 ;;
    k6-tuned)      runtool k6-tuned      run_k6_tuned ;;
    locust)        runtool locust        run_locust ;;
    locust-tuned)  runtool locust-tuned  run_locust_tuned ;;
    jmeter)        runtool jmeter        run_jmeter ;;
    jmeter-tuned)  runtool jmeter-tuned  run_jmeter_tuned ;;
    gatling)       runtool gatling       run_gatling ;;
    *) echo "unknown tool: $t" ;;
  esac
done

# ---- report ---------------------------------------------------------------
say "Results ($VUS VUs · $DURATION · GET /json)"
TABLE="$(python3 "$ROOT/lib/report.py" "$RESULTS")"
echo "$TABLE" | tee "$RESULTS/summary.md"
echo
echo "ran: ${ran[*]:-none}${fail:+   failed: ${fail[*]}}"
[ ${#fail[@]} -eq 0 ]
