#!/usr/bin/env python3
"""Check lightweight crate dependency boundaries.

Type crates should remain data-contract crates. This guard intentionally starts
small: it blocks direct dependencies from any `kcode-*-types` crate to root or
runtime-heavy internal crates. It allows external dependencies for now, while
making internal domain leaks visible and easy to extend.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# Internal crates that are allowed as dependencies of type crates.
# Keep this list narrow. Add a crate only if it is itself a data-contract crate.
ALLOWED_INTERNAL_TYPE_DEPS = {
    "kcode-message-types",
}

# Internal crates that type crates must not depend on directly. Most are runtime,
# provider, UI, storage, or root behavior crates. `kcode-core` is intentionally
# blocked so it does not become the backdoor catch-all dependency for DTO crates.
FORBIDDEN_INTERNAL_DEPS = {
    "kcode",
    "kcode-agent-runtime",
    "kcode-core",
    "kcode-embedding",
    "kcode-pdf",
    "kcode-plan",
    "kcode-provider-core",
    "kcode-provider-gemini",
    "kcode-provider-metadata",
    "kcode-provider-openrouter",
    "kcode-protocol",
    "kcode-terminal-launch",
    "kcode-tui-core",
    "kcode-tui-markdown",
    "kcode-tui-mermaid",
    "kcode-tui-render",
    "kcode-tui-workspace",
}


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
    warnings: list[str] = []

    for package_id in sorted(workspace_ids, key=lambda pid: package_by_id[pid]["name"]):
        package = package_by_id[package_id]
        name = package["name"]
        if not is_type_crate(name):
            continue

        for dep in package.get("dependencies", []):
            dep_name = dep["name"]
            if dep_name not in workspace_names:
                continue
            if dep_name in ALLOWED_INTERNAL_TYPE_DEPS:
                continue
            if is_type_crate(dep_name):
                continue
            if dep_name in FORBIDDEN_INTERNAL_DEPS:
                errors.append(f"{name} must not depend on runtime/internal crate {dep_name}")
            else:
                warnings.append(
                    f"{name} depends on internal non-type crate {dep_name}; review boundary policy"
                )

    for warning in warnings:
        print(f"warning: {warning}", file=sys.stderr)
    for error in errors:
        print(f"error: {error}", file=sys.stderr)

    if errors:
        print(f"dependency boundary check failed: {len(errors)} error(s)", file=sys.stderr)
        return 1

    print("dependency boundary check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
