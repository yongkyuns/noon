#!/usr/bin/env python3
"""Enforce package dependency direction without resolving/building dependencies.

Policy covers every declared normal, build and dev edge, including optional and
inactive target edges. Dev dependencies are not an architectural escape hatch.
Cargo's package.dependencies records normalize aliases and workspace inheritance;
resolve.nodes describes enabled features instead and must not be used here.
"""

import json
from pathlib import Path
import subprocess
import sys


# Existing #960/#961 policy; this change fixes discovery, not layer ownership.
FORBIDDEN = {
    "noon-core": (
        "noon-compile", "noon-runtime", "noon-render-wgpu", "noon-native", "noon-web", "noon",
    ),
    "noon-compile": ("noon-runtime", "noon-render-wgpu", "noon-native", "noon-web", "noon"),
    "noon-runtime": ("noon-render-wgpu", "noon-native", "noon-web", "noon"),
    "noon-render-wgpu": ("noon-native", "noon-web", "noon"),
    "noon": ("noon-native", "noon-web"),
}


class InputError(ValueError):
    """The check could not obtain complete, usable workspace metadata."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise InputError(message)


def nonempty_string(value: object) -> bool:
    return isinstance(value, str) and bool(value)


def inspect_metadata(metadata: object, root: Path) -> list[str]:
    """Validate the fields we consume, then report all forbidden package edges."""
    require(isinstance(metadata, dict), "metadata must be a JSON object")
    require(type(metadata.get("version")) is int and metadata["version"] == 1,
            "expected cargo metadata format version 1")
    workspace_root = metadata.get("workspace_root")
    require(nonempty_string(workspace_root), "missing workspace_root")
    require(Path(workspace_root).resolve() == root.resolve(),
            "metadata belongs to a different workspace")
    members = metadata.get("workspace_members")
    require(isinstance(members, list) and bool(members)
            and all(nonempty_string(member) for member in members),
            "missing or malformed workspace_members")
    require(len(set(members)) == len(members), "duplicate workspace member IDs")
    packages = metadata.get("packages")
    require(isinstance(packages, list), "missing or malformed packages")
    by_name = {}
    ids = set()
    for package in packages:
        require(isinstance(package, dict), "malformed package record")
        name, identity = package.get("name"), package.get("id")
        require(nonempty_string(name) and nonempty_string(identity),
                "package is missing a name or ID")
        require(identity not in ids and name not in by_name, "duplicate package record")
        ids.add(identity)
        by_name[name] = package
    require(ids == set(members), "package records do not cover exactly the workspace members")
    require(set(FORBIDDEN) <= set(by_name),
            "missing policy packages: " + ", ".join(sorted(set(FORBIDDEN) - set(by_name))))

    violations = []
    for name, forbidden in FORBIDDEN.items():
        package = by_name[name]
        manifest = package.get("manifest_path")
        require(nonempty_string(manifest), f"{name}: missing manifest_path")
        dependencies = package.get("dependencies")
        require(isinstance(dependencies, list), f"{name}: missing dependencies")
        for dependency in dependencies:
            require(isinstance(dependency, dict), f"{name}: malformed dependency record")
            require(nonempty_string(dependency.get("name")), f"{name}: missing dependency name")
            require("kind" in dependency and dependency["kind"] in (None, "normal", "build", "dev"),
                    f"{name}: missing or unknown dependency kind")
            require(type(dependency.get("optional")) is bool, f"{name}: malformed optional flag")
            for field in ("rename", "target"):
                require(field in dependency and (dependency[field] is None
                        or nonempty_string(dependency[field])), f"{name}: malformed {field}")
            # `name` is the package identity; `rename` is only its local spelling.
            target = dependency["name"]
            if target in forbidden:
                details = [dependency["kind"] or "normal"]
                if dependency["rename"]:
                    details.append(f"alias={dependency['rename']}")
                if dependency["optional"]:
                    details.append("optional")
                if dependency["target"]:
                    details.append(f"target={dependency['target']}")
                violations.append(
                    f"{manifest}: {name} must not depend on {target} ({', '.join(details)})"
                )
    return violations


def main(argv: list[str]) -> int:
    if len(argv) != 1:
        print("usage: layer_dependency_ratchet.py WORKSPACE_ROOT", file=sys.stderr)
        return 2
    try:
        root = Path(argv[0]).resolve()
        manifest = root / "Cargo.toml"
        # An unreadable/missing manifest is not an empty dependency graph.
        manifest.read_text(encoding="utf-8")
        result = subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1", "--offline",
             "--manifest-path", str(manifest)],
            cwd=root, capture_output=True, text=True, encoding="utf-8", timeout=60,
        )
        if result.returncode:
            raise InputError(f"cargo metadata failed (exit {result.returncode}):\n{result.stderr.strip()}")
        violations = inspect_metadata(json.loads(result.stdout), root)
    except (OSError, ValueError, subprocess.TimeoutExpired) as error:
        print(f"layer dependency ratchet: cannot check workspace: {error}", file=sys.stderr)
        return 2
    if violations:
        for violation in violations:
            print(f"layer dependency ratchet: {violation}", file=sys.stderr)
        print("\nKeep dependency ownership one-way; see docs/architecture.md, #960 and #961 A6.12.",
              file=sys.stderr)
        return 1
    print("architecture layer dependency ratchet passed (all normal/build/dev declarations)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
