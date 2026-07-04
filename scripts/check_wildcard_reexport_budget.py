#!/usr/bin/env python3
"""Enforce a ratcheting cross-crate wildcard re-export budget.

Counts production Rust occurrences of `pub use jcode_<crate>::*;` (cross-crate
wildcard re-exports). These re-exports erase crate boundaries: every symbol in
the re-exported crate becomes reachable through the re-exporting crate, so new
code silently couples across layers and edits to the re-exported crate rebuild
the whole spine.

Module-internal wildcards (`pub use self::...`, `pub use super::...`,
`pub use crate::...`, `pub use <module>::*` within a crate) are allowed; only
`jcode_*` cross-crate wildcards are budgeted.

Policy:
- Existing files may not increase their count.
- New production files may not introduce cross-crate wildcard re-exports.
- Total count may not increase.
- `--update` refreshes the baseline after intentional cleanup.

The long-term goal is to drive this budget to zero as the migration-era
re-export spine (base -> app-core -> tui -> root) is dismantled. See
docs/CRATE_OWNERSHIP_BOUNDARIES.md.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parent.parent
BASELINE_FILE = REPO_ROOT / "scripts" / "wildcard_reexport_budget.json"
SCAN_ROOTS = (REPO_ROOT / "src", REPO_ROOT / "crates")
PATTERN = re.compile(r"^\s*pub\s+use\s+(?:::)?jcode_[a-z0-9_]+(?:::[A-Za-z0-9_]+)*::\*\s*;")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--update", action="store_true", help="refresh the baseline")
    return parser.parse_args()


def production_rust_files() -> list[Path]:
    files: list[Path] = []
    for root in SCAN_ROOTS:
        if not root.exists():
            continue
        for path in sorted(root.rglob("*.rs")):
            rel = path.relative_to(REPO_ROOT).as_posix()
            if "/target/" in f"/{rel}/":
                continue
            files.append(path)
    return files


def count_wildcards(path: Path) -> int:
    count = 0
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return 0
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("//"):
            continue
        if PATTERN.match(line):
            count += 1
    return count


def collect_counts() -> dict[str, int]:
    counts: dict[str, int] = {}
    for path in production_rust_files():
        count = count_wildcards(path)
        if count > 0:
            rel = path.relative_to(REPO_ROOT).as_posix()
            counts[rel] = count
    return counts


def load_baseline() -> dict[str, Any] | None:
    if not BASELINE_FILE.exists():
        return None
    try:
        return json.loads(BASELINE_FILE.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None


def write_baseline(counts: dict[str, int]) -> None:
    payload = {
        "total": sum(counts.values()),
        "files": dict(sorted(counts.items())),
    }
    BASELINE_FILE.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    args = parse_args()
    counts = collect_counts()
    total = sum(counts.values())

    if args.update:
        write_baseline(counts)
        print(f"wildcard re-export baseline updated: total={total} files={len(counts)}")
        return 0

    baseline = load_baseline()
    if baseline is None:
        print(
            "error: missing or invalid baseline file "
            f"{BASELINE_FILE.relative_to(REPO_ROOT)}; run with --update to create it",
            file=sys.stderr,
        )
        return 1

    baseline_files: dict[str, int] = baseline.get("files", {})
    baseline_total: int = baseline.get("total", sum(baseline_files.values()))
    errors: list[str] = []

    for rel, count in sorted(counts.items()):
        allowed = baseline_files.get(rel)
        if allowed is None:
            errors.append(
                f"{rel}: new file introduces {count} cross-crate wildcard re-export(s); "
                "use explicit `pub use crate::path::{Item, ...}` instead"
            )
        elif count > allowed:
            errors.append(
                f"{rel}: cross-crate wildcard re-exports increased from {allowed} to {count}"
            )

    if total > baseline_total:
        errors.append(
            f"total cross-crate wildcard re-exports increased from {baseline_total} to {total}"
        )

    if errors:
        for error in errors:
            print(f"error: {error}", file=sys.stderr)
        print(
            "wildcard re-export budget check failed. These re-exports erase crate "
            "boundaries; prefer explicit item re-exports. If a removal made the "
            "baseline stale, run scripts/check_wildcard_reexport_budget.py --update",
            file=sys.stderr,
        )
        return 1

    if total < baseline_total or any(
        counts.get(rel, 0) < allowed for rel, allowed in baseline_files.items()
    ):
        print(
            f"wildcard re-export budget check passed (total={total}, baseline={baseline_total}); "
            "consider running --update to ratchet the baseline down"
        )
    else:
        print(f"wildcard re-export budget check passed (total={total})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
