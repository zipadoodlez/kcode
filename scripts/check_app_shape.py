#!/usr/bin/env python3
"""Ratchet the shape of `App` while it is being re-cored.

`App` (`crates/kcode-tui/src/tui/app.rs`) is the largest single cost in the
tree: ~310 fields in one struct and 57 separate `impl App` blocks spread over 53
files. The re-core (`plans/app-shape.md`, "Re-core `App`") turns it into a coordinator
holding named sub-structs, so the field count, the `impl App` block count, and
the `use super::*` glob count must all fall.

This script measures those three numbers and refuses to let any of them grow.
It tracks no per-group detail on purpose: the re-core's own "done when" is
monotonicity, and a metric that moves for the wrong reason is less harmful than
a metric nobody trusts.

Policy:
- Each metric may only stay flat or fall.
- A metric that falls fails until the baseline records it, so the counts can only
  tighten. Otherwise an unrecorded improvement leaves the looser old number in
  force and lets it regrow unnoticed.
- `--update` re-baselines after an intentional change (a stage of the re-core
  that adds fields to a sub-struct and removes them from `App` is net-zero here,
  re-baseline then; a stage that is not net-zero is a regression).

The field count is measured from `pub struct App`'s body. Nested sub-struct
definitions elsewhere in the file, and fields already grouped into a sub-struct,
still count once each as declared fields.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
BASELINE_FILE = REPO_ROOT / "scripts" / "app_shape_budget.json"
TUI_SRC = REPO_ROOT / "crates" / "kcode-tui" / "src"
APP_FILE = TUI_SRC / "tui" / "app.rs"

FIELD_START = re.compile(r"^\s*(?:pub(?:\s*\([^)]*\))?\s+)?[A-Za-z_][A-Za-z0-9_]*\s*:")
IMPL_APP = re.compile(r"^\s*impl\s+App\b")
SUPER_GLOB = re.compile(r"^\s*use\s+super::\s*\*\s*;")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--update", action="store_true", help="refresh the baseline")
    return parser.parse_args()


def count_app_fields(path: Path) -> int:
    """Count top-level fields in `pub struct App`'s body.

    A field ends at the first line whose brace depth is back to the body's and
    which ends in a comma, so multi-line generic/enum types stay one field.
    Line comments and attribute lines are ignored; `pub`, `pub(crate)` and
    `pub(super)` prefixes are stripped before matching the field name.
    """
    lines = path.read_text(encoding="utf-8").splitlines()
    start = None
    for index, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith(("pub struct App {", "struct App {")):
            start = index
            break
    if start is None:
        raise SystemExit(f"error: `struct App` not found in {path.relative_to(REPO_ROOT)}")

    depth = 0
    in_field = False
    fields = 0
    for index in range(start, len(lines)):
        raw = lines[index]
        code = raw.split("//", 1)[0] if "//" in raw else raw
        stripped = raw.strip()
        if not in_field and (stripped.startswith("//") or stripped.startswith("#")):
            continue
        depth_at_start = depth
        depth += code.count("{") - code.count("}")
        if not in_field:
            if depth_at_start == 1 and FIELD_START.match(code):
                in_field = True
                if depth == 1 and code.rstrip().endswith(","):
                    fields += 1
                    in_field = False
        elif depth == 1 and code.rstrip().endswith(","):
            fields += 1
            in_field = False
        if depth == 0 and index > start:
            break
    return fields


def count_matches(root: Path, pattern: re.Pattern[str]) -> int:
    total = 0
    for path in sorted(root.rglob("*.rs")):
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        for line in text.splitlines():
            if line.lstrip().startswith("//"):
                continue
            if pattern.match(line):
                total += 1
    return total


def collect_metrics() -> dict[str, int]:
    return {
        "app_fields": count_app_fields(APP_FILE),
        "impl_app_blocks": count_matches(TUI_SRC, IMPL_APP),
        "super_glob_imports": count_matches(TUI_SRC, SUPER_GLOB),
    }


def load_baseline() -> dict[str, int] | None:
    if not BASELINE_FILE.exists():
        return None
    try:
        data = json.loads(BASELINE_FILE.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    metrics = data.get("metrics") if isinstance(data, dict) else None
    if not isinstance(metrics, dict) or any(not isinstance(v, int) for v in metrics.values()):
        return None
    return metrics


def write_baseline(metrics: dict[str, int]) -> None:
    payload = {"version": 1, "metrics": metrics}
    BASELINE_FILE.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    args = parse_args()
    metrics = collect_metrics()

    if args.update:
        write_baseline(metrics)
        summary = ", ".join(f"{k}={v}" for k, v in metrics.items())
        print(f"app-shape baseline updated: {summary}")
        return 0

    baseline = load_baseline()
    if baseline is None:
        print(
            f"error: missing or invalid baseline {BASELINE_FILE.relative_to(REPO_ROOT)}; "
            "run with --update to create it",
            file=sys.stderr,
        )
        return 1

    regressions: list[str] = []
    improvements: list[str] = []
    for key, current in metrics.items():
        allowed = baseline.get(key)
        if allowed is None:
            regressions.append(f"{key} has no baseline entry (current={current})")
        elif current > allowed:
            regressions.append(f"{key} grew: {allowed} -> {current}")
        elif current < allowed:
            improvements.append(f"{key} shrank: {allowed} -> {current}")

    if regressions:
        print(
            "App shape regressed. The re-core may only shrink these numbers:",
            file=sys.stderr,
        )
        for entry in regressions:
            print(f"  - {entry}", file=sys.stderr)
        print(
            "If the growth is intentional, run scripts/check_app_shape.py --update "
            "in the same commit.",
            file=sys.stderr,
        )
        return 1

    summary = ", ".join(f"{k}={v}" for k, v in metrics.items())
    if improvements:
        print(
            "App shape improved, but the baseline was not updated. The ratchet "
            "only tightens: record the improvement in this commit with "
            "`scripts/check_app_shape.py --update`, or the looser old number "
            "stays in force:",
            file=sys.stderr,
        )
        for entry in improvements:
            print(f"  - {entry}", file=sys.stderr)
        return 1

    print(f"App shape OK ({summary})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
