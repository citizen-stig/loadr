#!/usr/bin/env python3
"""Capture-copy an examples/ plan against the local demo target.

Same philosophy as prep-demo-plan.py (clamp durations, absolutize data paths,
never touch scenario shapes/checks/thresholds) plus scheme-aware retargeting:
every placeholder host (api.example.com, shop.example.com, httpbin.org, ...)
is pointed at the local listener that speaks its protocol.

    python3 prep-example-plan.py <src> <out> --http 9801 --ws 9802 \
        --tcp 9803 --udp 9804 [--cap 6] [--pg localhost:5433] [--redis localhost:6380] \
        [--mysql localhost:3307] [--mongo localhost:27018]
"""
import re
import sys
from pathlib import Path

UNIT_SECONDS = {"ms": 0.001, "s": 1, "m": 60, "h": 3600}


def main() -> None:
    argv = sys.argv[1:]

    def opt(name, default=None):
        return argv[argv.index(name) + 1] if name in argv else default

    src, out = Path(argv[0]), Path(argv[1])
    cap_s = int(opt("--cap", 6))
    http, ws = opt("--http", "9801"), opt("--ws", "9802")
    tcp, udp = opt("--tcp", "9803"), opt("--udp", "9804")

    text = src.read_text()
    plan_dir = src.resolve().parent

    # Apply a regex substitution to the CODE part of each line only, so header
    # comments (which quote example URLs) stay pristine in the YAML view.
    def code_sub(pattern, repl, whole: str) -> str:
        out_lines = []
        for line in whole.splitlines(keepends=True):
            stripped = line.lstrip()
            if stripped.startswith("#"):
                out_lines.append(line)
                continue
            m = re.search(r"\s#", line)
            code, comment = (line[: m.start()], line[m.start():]) if m else (line, "")
            out_lines.append(re.sub(pattern, repl, code) + comment)
        return "".join(out_lines)

    # --- retarget by scheme -------------------------------------------------
    # http(s)/ws(s)/sse(s)/tcp/udp placeholder hosts -> local listeners.
    text = code_sub(r"https?://[a-z0-9.-]*\b(example\.com|httpbin\.org|example\.org)\b(:\d+)?",
                  f"http://127.0.0.1:{http}", text)
    text = code_sub(r"wss?://[a-z0-9.-]*example\.com(:\d+)?", f"ws://127.0.0.1:{ws}", text)
    text = code_sub(r"sses?://[a-z0-9.-]*example\.com(:\d+)?", f"sse://127.0.0.1:{http}", text)
    text = code_sub(r"tcp://[a-z0-9.-]*example\.com(:\d+)?", f"tcp://127.0.0.1:{tcp}", text)
    text = code_sub(r"udp://[a-z0-9.-]*example\.com(:\d+)?", f"udp://127.0.0.1:{udp}", text)
    # output sinks referenced by bare service names (influxdb:8086 etc.)
    text = code_sub(r"https?://influxdb:\d+", f"http://127.0.0.1:{http}", text)
    # datastores -> the real local containers, keeping credentials/db from the flag
    for flag, scheme in (("--pg", "postgres"), ("--mysql", "mysql"), ("--mongo", "mongodb"), ("--redis", "redis")):
        target = opt(flag)
        if target:
            text = code_sub(scheme + r"://[^\s\"']+",
                          lambda m, t=target: rewrite_dsn(m.group(0), t), text)
    grpc = opt("--grpc")
    if grpc:
        text = code_sub(r"grpc://[a-z0-9.-]*example\.com(:\d+)?", f"grpc://{grpc}", text)
    for flag, scheme in (("--kafka", "kafka"), ("--es", "elasticsearch")):
        target = opt(flag)
        if target:
            text = code_sub(scheme + r"://[a-z0-9.-]+(:\d+)?", f"{scheme}://{target}", text)
    amqp = opt("--amqp")
    if amqp:
        text = code_sub(r"(amqp://(?:[^@/\s]+@)?)[a-z0-9.-]+(:\d+)?", lambda m: f"{m.group(1)}{amqp}", text)

    # --- absolutize data/ and protos/ paths ----------------------------------
    def absolutize(m: re.Match) -> str:
        pre, quote, rel = m.group("pre"), m.group("q") or "", m.group("rel")
        if rel.startswith("/") or "${" in rel:
            return m.group(0)
        return f"{pre}{quote}{(plan_dir / rel).resolve()}{quote}"

    text = re.sub(
        r'(?P<pre>(?:\bpath:|\bfile:|-|\[)\s*)(?P<q>["\']?)(?P<rel>(?:\./)?(?:data|protos|payloads|scripts)/[^\s,\]"\']+)(?P=q)',
        absolutize, text)

    # --- clamp durations ------------------------------------------------------
    def clamp(m: re.Match) -> str:
        pre, num, unit = m.group("pre"), float(m.group("num")), m.group("unit")
        if num * UNIT_SECONDS[unit] <= cap_s:
            return m.group(0)
        return f"{pre}{cap_s}s"

    text = re.sub(r'(?P<pre>\b(?:duration|session_duration|gracefulStop|graceful_stop|start_time|start_after):\s*)'
                  r'(?P<num>\d+(?:\.\d+)?)(?P<unit>ms|s|m|h)\b', clamp, text)
    # iteration-based executors: cap total iterations so runs end promptly
    text = re.sub(r'(\biterations:\s*)(\d+)',
                  lambda m: f"{m.group(1)}{min(int(m.group(2)), 50)}", text)
    # cap open-model arrival rates: the local capture target is a small Python
    # server, not the production service the SLO numbers were written for
    text = code_sub(r'(\b(?:rate|start_rate|target):\s*)(\d{3,})',
                    lambda m: f"{m.group(1)}60", text)

    # generic literal swaps (--replace old=new), code lines only
    a2 = list(argv)
    while "--replace" in a2:
        i = a2.index("--replace")
        old_lit, _, new_lit = a2[i + 1].partition("=")
        text = code_sub(re.escape(old_lit), new_lit.replace("\\", "\\\\"), text)
        del a2[i:i + 2]

    # --- supply values for free-standing template variables (--var k=v) ------
    # Some examples reference ${token}-style variables that a surrounding doc
    # explains; give the capture copy a concrete value via a variables: block.
    var_pairs = []
    a = list(argv)
    while "--var" in a:
        i = a.index("--var")
        k, _, v = a[i + 1].partition("=")
        var_pairs.append((k, v))
        del a[i:i + 2]
    if var_pairs:
        if re.search(r"^variables:", text, flags=re.M) is None:
            text += "\nvariables:\n"
        for k, v in var_pairs:
            text = re.sub(r"^variables:\n", f"variables:\n  {k}: {v}\n", text, count=1, flags=re.M)

    out.write_text(text)
    print(f"prepped {src.name} -> {out}")


def rewrite_dsn(dsn: str, hostport: str) -> str:
    """Swap host:port inside scheme://[user:pass@]host[:port][/rest]."""
    return re.sub(r"(://(?:[^@/\s]+@)?)[^/:\s]+(:\d+)?", lambda m: f"{m.group(1)}{hostport}", dsn, count=1)


if __name__ == "__main__":
    main()
