#!/usr/bin/env python3
"""Check crate dependency boundaries for type crates.

The `kcode-*-types` crates are data contracts. One rule: a type crate may
depend on other type crates and on external crates, but not on any other
workspace crate. A DTO that pulls in runtime, provider, UI, or storage
behaviour stops being a data contract and becomes the backdoor dependency that
everything reaches through.

This replaced an allow-list plus a 17-entry deny-list, whose deny-list had
already drifted (it named two crates that no longer exist) and which missed
every internal crate nobody had thought to add.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

def cargo_metadata() -> dict:
    result = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    )
    return json.loads(result.stdout)


def is_type_crate(name: str) -> bool:
    return name.startswith("kcode-") and name.endswith("-types")


def main() -> int:
    metadata = cargo_metadata()
    package_by_id = {package["id"]: package for package in metadata["packages"]}
    workspace_ids = set(metadata["workspace_members"])
    workspace_names = {
        package_by_id[package_id]["name"] for package_id in workspace_ids if package_id in package_by_id
    }

    errors: list[str] = []

    for package_id in sorted(workspace_ids, key=lambda pid: package_by_id[pid]["name"]):
        package = package_by_id[package_id]
        name = package["name"]
        if not is_type_crate(name):
            continue

        for dep in package.get("dependencies", []):
            dep_name = dep["name"]
            if dep_name not in workspace_names or is_type_crate(dep_name):
                continue
            errors.append(f"{name} must not depend on non-type workspace crate {dep_name}")

    for error in errors:
        print(f"error: {error}", file=sys.stderr)

    if errors:
        print(f"dependency boundary check failed: {len(errors)} error(s)", file=sys.stderr)
        return 1

    print("dependency boundary check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
