#!/usr/bin/env python3
"""Turn a real loadr demo plan into a walkthrough capture-copy.

Two edits, nothing else — the scenario shape, checks, thresholds and
``${env.BASE_URL}`` are all preserved so the screenshots show the genuine plan:

  1. Relative ``path: data/...`` feeders become absolute. The desktop app runs
     each plan from a throwaway temp dir, so a relative data path would not
     resolve; anchoring it to the plan's own directory keeps feeders working.
  2. Any single ``duration:`` longer than the cap is clamped to the cap. A soak
     or a long ramp can't be screenshotted at full length; clamping lets the
     live run finish on-screen while keeping every ramp's shape intact.
  3. With ``--base-url``, a hardcoded ``base_url:`` is rewritten to the given
     target (plans that use ``${env.BASE_URL}`` are left alone — the env does
     the job). Needed when the backend runs on a non-default port.
  4. With ``--replace old=new`` (repeatable), a literal substring is swapped —
     e.g. retargeting a hardcoded DSN at a remapped local port.

    python3 prep-demo-plan.py <src.yaml> <out.yaml> [--cap 8] [--base-url URL]
                              [--replace old=new ...]
"""
import re
import sys
from pathlib import Path

UNIT_SECONDS = {"ms": 0.001, "s": 1, "m": 60, "h": 3600}


def to_seconds(n: float, unit: str) -> float:
    return n * UNIT_SECONDS[unit]


def prep(
    src: Path,
    out: Path,
    cap_s: int,
    base_url: str | None = None,
    replaces: list[tuple[str, str]] | None = None,
) -> None:
    text = src.read_text()
    plan_dir = src.resolve().parent

    # 1) absolutize relative data feeder paths (path: data/foo.csv, ./data/…)
    def absolutize(m: re.Match) -> str:
        pre, quote, rel = m.group("pre"), m.group("q") or "", m.group("rel")
        if rel.startswith("/") or "${" in rel:
            return m.group(0)
        return f"{pre}{quote}{(plan_dir / rel).resolve()}{quote}"

    text = re.sub(
        r'(?P<pre>\bpath:\s*)(?P<q>["\']?)(?P<rel>(?:\./)?data/[^\s"\']+)(?P=q)',
        absolutize,
        text,
    )

    # 2) clamp long durations to the cap (keeps ramp proportions; only trims)
    def clamp(m: re.Match) -> str:
        pre, num, unit = m.group("pre"), float(m.group("num")), m.group("unit")
        if to_seconds(num, unit) <= cap_s:
            return m.group(0)
        return f"{pre}{cap_s}s"

    text = re.sub(
        r'(?P<pre>\bduration:\s*)(?P<num>\d+(?:\.\d+)?)(?P<unit>ms|s|m|h)\b',
        clamp,
        text,
    )

    # 3) retarget a hardcoded base_url (never a ${env...} one)
    if base_url:
        # value chars stop at , } and quotes so flow-style mappings stay intact
        text = re.sub(
            r'(\bbase_url:\s*)(?!["\']?\$\{)["\']?[^\s,}"\']+["\']?',
            lambda m: f"{m.group(1)}{base_url}",
            text,
        )

    # 4) literal substring swaps (e.g. a hardcoded DSN onto a remapped port)
    for old, new in replaces or []:
        text = text.replace(old, new)

    out.write_text(text)


def main() -> None:
    argv = sys.argv[1:]
    cap, base_url, replaces = 8, None, []
    if "--cap" in argv:
        i = argv.index("--cap")
        cap = int(argv[i + 1])
        del argv[i:i + 2]
    if "--base-url" in argv:
        i = argv.index("--base-url")
        base_url = argv[i + 1]
        del argv[i:i + 2]
    while "--replace" in argv:
        i = argv.index("--replace")
        old, _, new = argv[i + 1].partition("=")
        replaces.append((old, new))
        del argv[i:i + 2]
    src, out = Path(argv[0]), Path(argv[1])
    prep(src, out, cap, base_url, replaces)
    print(f"prepped {src.name} -> {out} (cap {cap}s{', base_url ' + base_url if base_url else ''})")


if __name__ == "__main__":
    main()
