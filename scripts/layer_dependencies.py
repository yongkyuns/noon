#!/usr/bin/env python3
"""Check declared Cargo package edges, including inactive targets and features."""
from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys

# Normal, build and dev dependencies obey the same direction. A test-only or
# optional upward edge still couples an engine owner to a later layer/host.
FORBIDDEN = {
    "noon-core": {"noon-compile", "noon-runtime", "noon-render-wgpu", "noon-native", "noon-web", "noon"},
    "noon-compile": {"noon-runtime", "noon-render-wgpu", "noon-native", "noon-web", "noon"},
    "noon-runtime": {"noon-render-wgpu", "noon-native", "noon-web", "noon"},
    "noon-render-wgpu": {"noon-native", "noon-web", "noon"},
    "noon": {"noon-native", "noon-web"},
}


def required_string(record: dict, key: str) -> str:
    value = record.get(key)
    if not isinstance(value, str) or not value:
        raise ValueError(f"metadata requires a nonempty {key!r}")
    return value


def violations(metadata: dict, root: Path) -> list[str]:
    """Use package declarations, never resolve.nodes (which omits inactive edges)."""
    if not isinstance(metadata, dict) or metadata.get("version") != 1:
        raise ValueError("expected Cargo metadata format version 1")
    packages = metadata.get("packages")
    members = metadata.get("workspace_members")
    if not isinstance(packages, list) or not isinstance(members, list) or not all(isinstance(x, str) for x in members):
        raise ValueError("metadata requires packages and workspace_members arrays")
    if Path(required_string(metadata, "workspace_root")).resolve() != root.resolve():
        raise ValueError("metadata workspace_root does not match the checked repository")
    by_manifest: dict[Path, dict] = {}
    for package in packages:
        if not isinstance(package, dict):
            raise ValueError("invalid package metadata")
        manifest = Path(required_string(package, "manifest_path")).resolve()
        if manifest in by_manifest:
            raise ValueError(f"duplicate package manifest: {manifest}")
        by_manifest[manifest] = package

    found = []
    for owner, forbidden in FORBIDDEN.items():
        manifest = (root / "crates" / owner / "Cargo.toml").resolve()
        package = by_manifest.get(manifest)
        if package is None or package.get("name") != owner or package.get("id") not in members:
            raise ValueError(f"missing or misidentified workspace package: {manifest}")
        dependencies = package.get("dependencies")
        if not isinstance(dependencies, list):
            raise ValueError(f"missing dependencies array for {owner}")
        for dep in dependencies:
            if not isinstance(dep, dict):
                raise ValueError(f"invalid dependency declaration for {owner}")
            name = required_string(dep, "name")
            # These fields are part of metadata v1. Missing/unknown data must
            # fail rather than accidentally exempt a declaration.
            if "kind" not in dep or dep["kind"] not in (None, "build", "dev"):
                raise ValueError(f"invalid dependency kind for {owner} -> {name}")
            if type(dep.get("optional")) is not bool:
                raise ValueError(f"invalid optional flag for {owner} -> {name}")
            for key in ("rename", "target"):
                if key not in dep or (dep[key] is not None and not isinstance(dep[key], str)):
                    raise ValueError(f"invalid {key} for {owner} -> {name}")
            if name in forbidden:
                details = [dep["kind"] or "normal"]
                if dep["rename"]:
                    details.append(f"alias={dep['rename']}")
                if dep["optional"]:
                    details.append("optional")
                if dep["target"]:
                    details.append(f"target={dep['target']}")
                found.append(f"{manifest}: {owner} must not depend on {name} ({', '.join(details)})")
    return found


def check(root: Path) -> int:
    # --no-deps reads declarations without activating/resolving the graph. In
    # particular, do not add --filter-platform or inspect only default features.
    result = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--offline",
         "--manifest-path", str(root / "Cargo.toml")],
        cwd=root, text=True, capture_output=True, check=False,
    )
    if result.returncode:
        raise ValueError(f"cargo metadata failed (exit {result.returncode}):\n{result.stderr.strip()}")
    found = violations(json.loads(result.stdout), root)
    for message in found:
        print("layer dependency ratchet: " + message, file=sys.stderr)
    if found:
        print("Keep dependency arrows down the engine stack; see #953, #960 and #961.", file=sys.stderr)
        return 1
    print("architecture layer dependency ratchet passed (normal/build/dev, all declared targets/features)")
    return 0


def main() -> int:
    root = Path(os.environ.get("NOON_ROOT", Path(__file__).resolve().parents[1])).resolve()
    try:
        return check(root)
    except (OSError, ValueError) as error:
        print(f"layer dependency ratchet: cannot check dependencies: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
