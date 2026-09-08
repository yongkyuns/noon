#!/usr/bin/env python3
"""Enforce Phase A's declared package dependency directions (#961 / #1272 R3).

Requires Python 3.10+ and Cargo. No compilation, dependency download or feature
resolution is needed: `cargo metadata --no-deps --offline` normalizes manifests.
Inspect packages[].dependencies, NOT resolve.nodes: the latter can hide disabled
optional and other-target edges. `name` is the package; `rename` is only an alias.

Policy: normal, build and dev declarations obey the same forbidden directions.
Tests needing an upper layer belong in that layer or an integration consumer, not
in a lower layer's dev-dependencies. Optional/target conditions are not exemptions.
This preserves the existing direct-edge policy; it is not a transitive graph or
feature-isolation qualification. Native/WASM feature checks remain necessary.

Exit status: 0 = checked/pass, 1 = forbidden edge, 2 = could not check.
"""

import json
import os
from pathlib import Path
import subprocess
import sys
from typing import Any


FORBIDDEN = {
    "noon-core": {"noon-compile", "noon-runtime", "noon-render-wgpu", "noon-native", "noon-web", "noon"},
    "noon-compile": {"noon-runtime", "noon-render-wgpu", "noon-native", "noon-web", "noon"},
    "noon-runtime": {"noon-render-wgpu", "noon-native", "noon-web", "noon"},
    "noon-render-wgpu": {"noon-native", "noon-web", "noon"},
    "noon": {"noon-native", "noon-web"},
}
PREFIX = "layer dependency ratchet:"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def nonempty_string(value: Any) -> bool:
    return isinstance(value, str) and bool(value)


def violations(metadata: Any, root: Path) -> list[str]:
    """Validate the metadata fields we consume, then report every forbidden edge."""
    require(isinstance(metadata, dict), "metadata must be an object")
    require(type(metadata.get("version")) is int and metadata["version"] == 1,
            "unsupported or missing metadata format version")
    require(nonempty_string(metadata.get("workspace_root")), "missing metadata workspace_root")
    require(Path(metadata["workspace_root"]).resolve() == root,
            "metadata workspace_root does not match NOON_ROOT")
    members = metadata.get("workspace_members")
    require(isinstance(members, list) and bool(members)
            and all(nonempty_string(member) for member in members),
            "metadata workspace_members must be a nonempty array of package IDs")
    require(len(set(members)) == len(members), "duplicate metadata workspace member")
    packages = metadata.get("packages")
    require(isinstance(packages, list) and bool(packages), "metadata packages must be a nonempty array")
    by_id = {}
    for package in packages:
        require(isinstance(package, dict) and nonempty_string(package.get("id")),
                "metadata package must have an ID")
        require(package["id"] not in by_id, "duplicate metadata package ID")
        require(nonempty_string(package.get("name")) and nonempty_string(package.get("manifest_path")),
                "metadata package must have a name and manifest_path")
        by_id[package["id"]] = package
    require(all(member in by_id for member in members), "metadata is missing a workspace member package")

    failures = []
    for name, forbidden in FORBIDDEN.items():
        expected = (root / "crates" / name / "Cargo.toml").resolve()
        matches = [by_id[member] for member in members
                   if Path(by_id[member]["manifest_path"]).resolve() == expected]
        require(len(matches) == 1 and matches[0]["name"] == name,
                f"missing or mismatched workspace package {name} at {expected}")
        dependencies = matches[0].get("dependencies")
        require(isinstance(dependencies, list), f"{name}: metadata dependencies must be an array")
        for dependency in dependencies:
            require(isinstance(dependency, dict) and nonempty_string(dependency.get("name")),
                    f"{name}: metadata dependency must have a package name")
            require("kind" in dependency and dependency["kind"] in (None, "build", "dev"),
                    f"{name}: unsupported or missing dependency kind")
            require("optional" in dependency and type(dependency["optional"]) is bool,
                    f"{name}: missing or invalid dependency optional flag")
            for field in ("rename", "target"):
                require(field in dependency and (dependency[field] is None or nonempty_string(dependency[field])),
                        f"{name}: missing or invalid dependency {field}")
            target = dependency["name"]
            if target in forbidden:
                alias = dependency["rename"] or target
                kind = dependency["kind"] or "normal"
                platform = dependency["target"] or "all targets"
                optional = ", optional" if dependency["optional"] else ""
                failures.append(
                    f"{name} must not depend on {target} "
                    f"(alias={alias}, kind={kind}, target={platform}{optional}; {expected})"
                )
    return failures


def main() -> int:
    try:
        root = Path(os.environ.get("NOON_ROOT", Path(__file__).resolve().parent.parent)).resolve(strict=True)
        # Do not let an incomplete checkout, unreadable input, or unexpected
        # workspace membership turn into a successful empty scan.
        for manifest in [root / "Cargo.toml", *(root / "crates" / name / "Cargo.toml" for name in FORBIDDEN)]:
            with manifest.open("rb") as source:
                source.read(1)
        result = subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1", "--offline",
             "--manifest-path", str(root / "Cargo.toml")],
            cwd=root, capture_output=True, text=True, check=False,
        )
        if result.returncode != 0:
            raise ValueError(f"cargo metadata failed (exit {result.returncode}):\n{result.stderr.strip()}")
        failures = violations(json.loads(result.stdout), root)
    except (OSError, ValueError, RuntimeError) as error:
        print(f"{PREFIX} could not check: {error}", file=sys.stderr)
        return 2

    if failures:
        for failure in failures:
            print(f"{PREFIX} {failure}", file=sys.stderr)
        print("\nPhase A layer dependency direction was violated. Move shared data/behavior "
              "to its owning lower layer instead of adding an upward dependency. "
              "See #953, #960 A5 and #961 A6.12.", file=sys.stderr)
        return 1
    print("architecture layer dependency ratchet passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
