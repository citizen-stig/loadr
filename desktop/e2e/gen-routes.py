#!/usr/bin/env python3
"""Synthesize a mock target's routes from a loadr plan's own contract.

Walks every request step in a plan and derives, per (method, path), the
response that satisfies the step's checks/asserts and extracts: the status
code, JSONPath fields with their expected values, substrings the body must
contain, response headers, and HTML/XML shapes for css/xpath/boundary
extractors. serve-target.py then serves exactly that.

    python3 gen-routes.py <plan.yaml> [...more plans] -o routes.json

The point: the demo target isn't hand-written per demo — it is *derived from
the plan*, so the walkthrough's green verdict is the plan's own gates passing
against a server that honours its documented contract.
"""
import json
import re
import sys
from pathlib import Path

import yaml


def as_list(v):
    return v if isinstance(v, list) else [] if v is None else [v]


def set_jsonpath(obj: dict, expr: str, value):
    """Set a simple JSONPath ($.a.b, $.items[0].id) into a dict skeleton."""
    parts = re.findall(r"\.([A-Za-z_][\w-]*)|\[(\d+)\]|\['([^']+)'\]", expr)
    cur = obj
    flat = [p for group in parts for p in [next(x for x in group if x != "")] ]
    for i, key in enumerate(flat):
        last = i == len(flat) - 1
        idx = int(key) if key.isdigit() else None
        if idx is not None:
            if not isinstance(cur, list):
                return  # parent wasn't a list; give up on this path
            while len(cur) <= idx:
                cur.append({})
            if last:
                cur[idx] = value
            else:
                if not isinstance(cur[idx], (dict, list)):
                    cur[idx] = {}
                cur = cur[idx]
        else:
            if last:
                if isinstance(cur, dict):
                    cur[key] = value
            else:
                nxt_is_idx = i + 1 < len(flat) and flat[i + 1].isdigit()
                if key not in cur or not isinstance(cur.get(key), (dict, list)):
                    cur[key] = [] if nxt_is_idx else {}
                cur = cur[key]


def norm_path(url: str) -> str:
    """Strip host + template segments: /orders/${id} -> /orders/*"""
    path = re.sub(r"^[a-z+]+://[^/]+", "", str(url)) or "/"
    path = path.split("?")[0]
    return re.sub(r"\$\{[^}]*\}[^/]*", "*", path) or "/"


def walk_steps(node, out):
    if isinstance(node, dict):
        for k, v in node.items():
            if k == "request" and isinstance(v, dict):
                out.append(v)
            else:
                walk_steps(v, out)
    elif isinstance(node, list):
        for item in node:
            walk_steps(item, out)


def sample_for(check: dict):
    """A value satisfying a jsonpath check that isn't a plain equals."""
    if "equals" in check:
        return check["equals"]
    if "less" in check or "less_than" in check:
        return max(0, float(check.get("less", check.get("less_than", 1))) - 1)
    if "greater" in check or "greater_than" in check:
        return float(check.get("greater", check.get("greater_than", 0))) + 1
    if "matches" in check:
        # best effort: a literal that matches common patterns, else the regex text
        pat = str(check["matches"]).strip("^$")
        return "AB-12345" if "\\d" in pat or "[0-9]" in pat else pat
    return "demo-value"


def build_route(req: dict) -> dict:
    r = {
        "method": str(req.get("method", "GET")).upper(),
        "path": norm_path(req.get("url", "/")),
        "status": 200,
        "json": {},
        "contains": [],
        "headers": {},
        "html_inputs": {},   # name -> value  (css input[name=x] extractors)
        "boundaries": [],    # (left, right)
        "xml": False,
    }
    checks = as_list(req.get("checks")) + as_list(req.get("assert")) + as_list(req.get("asserts"))
    for c in checks:
        if not isinstance(c, dict):
            continue
        t = c.get("type")
        if t == "status":
            if "equals" in c:
                r["status"] = int(c["equals"])
            elif "in" in c and as_list(c["in"]):
                r["status"] = int(as_list(c["in"])[0])
        elif t == "jsonpath":
            set_jsonpath(r["json"], str(c.get("expression", "$.ok")), sample_for(c))
        elif t in ("body", "body_contains"):
            v = c.get("contains") or c.get("value") or c.get("equals")
            if v:
                r["contains"].append(str(v))
        elif t == "header":
            r["headers"][str(c.get("name") or c.get("header", "X-Demo"))] = str(
                c.get("equals") or c.get("contains") or "demo"
            )
        elif t == "xpath":
            r["xml"] = True
            for name in re.findall(r"local-name\(\)\s*=\s*'([^']+)'", str(c.get("expression", ""))):
                r.setdefault("xml_elems", []).append(name)
    for e in as_list(req.get("extract")) + as_list(req.get("extracts")):
        if not isinstance(e, dict):
            continue
        t = e.get("type")
        if "chain" in e:
            check = e.get("check") or {}
            if "jsonpath" in e:
                val = as_list(check.get("one_of"))[:1] or [e.get("default", "demo")]
                set_jsonpath(r["json"], str(e["jsonpath"]), val[0])
            elif "jmespath" in e and "items" in str(e["jmespath"]):
                r["json"].setdefault("items", [{"name": "alpha", "price": 9}, {"name": "beta", "price": 19}])
                r["json"].setdefault("count", 2)
            continue
        if t == "xpath":
            r["xml"] = True
            for name in re.findall(r"local-name\(\)\s*=\s*'([^']+)'", str(e.get("expression", ""))):
                r.setdefault("xml_elems", []).append(name)
            continue
        if t == "jsonpath":
            expr = str(e.get("expression", "$.value"))
            # don't clobber a value a check already pinned
            probe = {}
            set_jsonpath(probe, expr, None)
            set_jsonpath(r["json"], expr, e.get("name", "demo") + "-123") if not _has(r["json"], expr) else None
        elif t == "css":
            m = re.search(r"\[name=['\"]?([\w-]+)", str(e.get("expression", "")))
            r["html_inputs"][m.group(1) if m else e.get("name", "field")] = f"{e.get('name','field')}-token-42"
        elif t == "boundary":
            r["boundaries"].append([str(e.get("left", "<<")), str(e.get("right", ">>"))])
        elif t == "header":
            r["headers"].setdefault(str(e.get("header", "X-Request-Id")), "req-abc-123")
        elif t == "regex":
            pat = str(e.get("expression", ""))
            lit = re.sub(r"\((?!\?)[^)]*\)", "GRP-77", pat)
            lit = re.sub(r"[\\^$*+?.|\[\]{}()]", "", lit)
            if lit:
                r["contains"].append(lit)
    return r


def _has(obj, expr) -> bool:
    keys = re.findall(r"\.([A-Za-z_][\w-]*)", expr)
    cur = obj
    for k in keys:
        if not isinstance(cur, dict) or k not in cur:
            return False
        cur = cur[k]
    return True


def merge(a: dict, b: dict) -> dict:
    """Two steps hit the same method+path: union their contracts."""
    a["json"].update({**b["json"], **a["json"]})
    a["contains"] = sorted(set(a["contains"]) | set(b["contains"]))
    a["headers"].update(b["headers"])
    a["html_inputs"].update(b["html_inputs"])
    a["boundaries"] += [x for x in b["boundaries"] if x not in a["boundaries"]]
    a["xml"] = a["xml"] or b["xml"]
    return a


def main():
    argv = sys.argv[1:]
    out_path = Path(argv[argv.index("-o") + 1]) if "-o" in argv else Path("routes.json")
    plans = [a for a in argv if a.endswith((".yaml", ".yml"))]
    routes = {}
    for p in plans:
        docs = [d for d in yaml.safe_load_all(Path(p).read_text())
                if isinstance(d, dict) and "scenarios" in d]
        if not docs:
            continue
        doc = docs[0]
        reqs = []
        walk_steps(doc.get("scenarios", {}), reqs)
        for req in reqs:
            url = str(req.get("url", ""))
            if url.startswith(("ws://", "wss://", "postgres://", "mysql://", "mongodb://", "redis://", "grpc")):
                continue  # not HTTP — other listeners handle these
            if url.startswith(("sse://", "sses://")):
                key = f"SSE {norm_path(url)}"
                routes[key] = {"method": "GET", "path": norm_path(url), "kind": "sse"}
                continue
            r = build_route(req)
            key = f"{r['method']} {r['path']}"
            routes[key] = merge(routes[key], r) if key in routes else r
    out_path.write_text(json.dumps(list(routes.values()), indent=1))
    print(f"routes: {len(routes)} -> {out_path}")


if __name__ == "__main__":
    main()
