#!/usr/bin/env python3
"""Normalise each tool's native output into one comparable record and render a
table. Every tool ran the same closed-model scenario (N VUs, fixed duration,
GET /json); we extract: total requests, error rate, throughput (req/s) and
latency p50/p95/p99 in milliseconds.

Usage: report.py <results_dir>
Writes <results_dir>/summary.json and prints a Markdown table to stdout.
"""
import csv
import glob
import json
import os
import sys

RESULTS = sys.argv[1] if len(sys.argv) > 1 else "results"


def pct(values, p):
    if not values:
        return None
    s = sorted(values)
    k = (len(s) - 1) * (p / 100.0)
    lo = int(k)
    hi = min(lo + 1, len(s) - 1)
    return s[lo] + (s[hi] - s[lo]) * (k - lo)


def rec(tool, requests, errors, rps, p50, p95, p99):
    return {
        "tool": tool,
        "requests": requests,
        "error_rate": round(errors / requests, 6) if requests else None,
        "rps": round(rps, 1) if rps is not None else None,
        "p50_ms": round(p50, 2) if p50 is not None else None,
        "p95_ms": round(p95, 2) if p95 is not None else None,
        "p99_ms": round(p99, 2) if p99 is not None else None,
    }


def parse_loadr(d):
    f = os.path.join(d, "loadr", "summary.json")
    if not os.path.exists(f):
        return None
    s = json.load(open(f))
    metrics = {m["metric"]: m["agg"] for m in s.get("metrics", [])}
    reqs = next((v for k, v in metrics.items() if k.endswith("_reqs")), {})
    dur = next((v for k, v in metrics.items() if k.endswith("_req_duration")), {})
    failed = metrics.get("http_req_failed", {})
    total = reqs.get("count") or reqs.get("sum") or 0
    errors = round((failed.get("rate") or 0) * total)
    return rec("loadr", total, errors, reqs.get("per_second") or reqs.get("rate"),
               dur.get("med"), dur.get("p95"), dur.get("p99"))


def parse_k6(d, sub="k6", tool="k6"):
    f = os.path.join(d, sub, "summary.json")
    if not os.path.exists(f):
        return None
    m = json.load(open(f))["metrics"]
    reqs = m.get("http_reqs", {})
    dur = m.get("http_req_duration", {})
    failed = m.get("http_req_failed", {})
    total = reqs.get("count", 0)
    # http_req_failed is a Rate: `value` is the failure ratio. (`passes`/`fails`
    # count the rate's true/false samples — `fails` = the OK requests — so they
    # must NOT be read as the error count.)
    errors = round(failed.get("value", failed.get("rate", 0)) * total)
    return rec(tool, total, errors, reqs.get("rate"),
               dur.get("med"), dur.get("p(95)"), dur.get("p(99)"))


def parse_k6_tuned(d):
    return parse_k6(d, sub="k6-tuned", tool="k6-tuned")


def parse_locust(d, sub="locust", tool="locust"):
    f = os.path.join(d, sub, "locust_stats.csv")
    if not os.path.exists(f):
        return None
    row = None
    for r in csv.DictReader(open(f)):
        if r.get("Name") == "Aggregated" or r.get("Type") == "":
            row = r
    row = row or {}

    def g(*names):
        for n in names:
            if n in row and row[n] not in ("", "N/A"):
                return float(row[n])
        return None
    total = int(g("Request Count") or 0)
    errors = int(g("Failure Count") or 0)
    return rec(tool, total, errors, g("Requests/s"),
               g("50%", "Median Response Time"), g("95%"), g("99%"))


def parse_locust_tuned(d):
    return parse_locust(d, sub="locust-tuned", tool="locust-tuned")


def parse_jmeter(d, sub="jmeter", tool="jmeter"):
    f = os.path.join(d, sub, "result.jtl")
    if not os.path.exists(f):
        return None
    elapsed, errors, total = [], 0, 0
    tmin, tmax = None, None
    for r in csv.DictReader(open(f)):
        try:
            e = float(r["elapsed"])
            ts = float(r["timeStamp"])
        except (KeyError, ValueError):
            continue
        elapsed.append(e)
        total += 1
        if r.get("success", "true").lower() != "true":
            errors += 1
        tmin = ts if tmin is None else min(tmin, ts)
        tmax = (ts + e) if tmax is None else max(tmax, ts + e)
    wall = ((tmax - tmin) / 1000.0) if (tmin is not None and tmax) else None
    rps = (total / wall) if wall else None
    return rec(tool, total, errors, rps, pct(elapsed, 50), pct(elapsed, 95), pct(elapsed, 99))


def parse_jmeter_tuned(d):
    return parse_jmeter(d, sub="jmeter-tuned", tool="jmeter-tuned")


def parse_gatling(d):
    hits = glob.glob(os.path.join(d, "gatling", "**", "js", "stats.json"), recursive=True)
    if not hits:
        return None
    st = json.load(open(sorted(hits)[-1]))["stats"]
    total = st["numberOfRequests"]["total"]
    errors = st["numberOfRequests"]["ko"]
    rps = st.get("meanNumberOfRequestsPerSecond", {}).get("total")
    return rec("gatling", total, errors, rps,
               st["percentiles1"]["total"], st["percentiles3"]["total"], st["percentiles4"]["total"])


def main():
    parsers = [parse_loadr, parse_k6, parse_k6_tuned, parse_jmeter, parse_jmeter_tuned,
               parse_gatling, parse_locust, parse_locust_tuned]
    records = [r for r in (p(RESULTS) for p in parsers) if r]
    json.dump(records, open(os.path.join(RESULTS, "summary.json"), "w"), indent=2)

    def cell(v, suffix=""):
        return f"{v}{suffix}" if v is not None else "—"

    print("| Tool | Requests | Error % | Throughput (req/s) | p50 (ms) | p95 (ms) | p99 (ms) |")
    print("|------|---------:|--------:|-------------------:|---------:|---------:|---------:|")
    # Sort by throughput desc so the table reads as a leaderboard.
    for r in sorted(records, key=lambda x: x["rps"] or 0, reverse=True):
        err = f"{r['error_rate'] * 100:.2f}" if r["error_rate"] is not None else "—"
        print(f"| {r['tool']} | {cell(r['requests'])} | {err} | "
              f"{cell(r['rps'])} | {cell(r['p50_ms'])} | {cell(r['p95_ms'])} | {cell(r['p99_ms'])} |")


if __name__ == "__main__":
    main()
