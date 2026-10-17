#!/usr/bin/env python3
"""Reconstruct the captured web host plus exact CI runtime in a new directory.

Uses local git objects and an already downloaded artifact only; no checkout
changes, rebuilds, network, or production source edits. Hash checks pin the
two probe hooks and preserve every compiled runtime identity check.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import tarfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def safe_parts(name):
    path = PurePosixPath(name)
    require(not path.is_absolute() and ".." not in path.parts and "\\" not in name, "unsafe archive path")
    return path.parts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--checkout", type=Path, required=True, help="Existing Noon clone containing the pinned web host commit")
    parser.add_argument("--runtime-zip", type=Path, required=True, help="Original artifact 12083686108 ZIP")
    parser.add_argument("--site", type=Path, required=True, help="New or empty output directory")
    args = parser.parse_args()
    provenance = json.loads((ROOT / "provenance.json").read_text())["current_capture"]
    require(digest(args.runtime_zip.read_bytes()) == provenance["runtime_artifact"]["archive_sha256"], "runtime ZIP SHA256 mismatch")
    site = args.site.resolve()
    require(not site.exists() or (site.is_dir() and not any(site.iterdir())), "site output must be empty")
    # Validate everything that needs git before creating the output directory.
    host = subprocess.check_output(["git", "-C", str(args.checkout.resolve()), "archive",
                                    provenance["web_host_revision"], "web"])
    site.mkdir(parents=True, exist_ok=True)
    host_count = 0
    with tarfile.open(fileobj=io.BytesIO(host)) as archive:
        for member in archive:
            parts = safe_parts(member.name)
            require(parts and parts[0] == "web", "unexpected host archive root")
            if member.isdir():
                continue
            require(member.isfile(), "host archive contains non-regular file")
            target = site.joinpath(*parts[1:])
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(archive.extractfile(member).read())
            host_count += 1
    require(host_count == provenance["web_host_audit"]["tracked_files_compared"], "host file count mismatch")
    overlay_count = 0
    with zipfile.ZipFile(args.runtime_zip) as archive:
        for member in archive.infolist():
            parts = safe_parts(member.filename)
            if member.is_dir():
                continue
            target = site.joinpath(*parts)
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(archive.read(member))
            overlay_count += 1
    require(overlay_count == 8, "runtime artifact member count mismatch")
    probe = ROOT / "harness/profile-probe.js"
    require(digest(probe.read_bytes()) == provenance["probe"]["sha256"], "probe SHA256 mismatch")
    shutil.copyfile(probe, site / "profile-probe.js")
    for name, expected in provenance["instrumented_host_files"].items():
        path = site / name
        original = path.read_bytes()
        require(digest(original) == expected["original_sha256"], "host original SHA256 mismatch: " + name)
        instrumented = b'import "./profile-probe.js";\n' + original
        if name == "semantic-engine-endpoint.js":
            boundary = b'    renderPort.addEventListener("message", ({ data: message }) => {\n'
            hook = b'      globalThis.__noonProfileMark("engine_receive", {kind:message?.type,sequence:message?.sequence,cadence:message?.cadence});\n'
            require(instrumented.count(boundary) == 1, "engine receive insertion boundary is not unique")
            instrumented = instrumented.replace(boundary, boundary + hook)
        require(digest(instrumented) == expected["captured_sha256"], "instrumented host SHA256 mismatch: " + name)
        path.write_bytes(instrumented)
    identity = json.loads((site / "runtime-build-identity.json").read_text())
    require(identity["sourceRevision"] == provenance["runtime"]["revision"], "runtime source revision mismatch")
    require(identity["buildId"] == provenance["runtime"]["buildId"], "runtime build ID mismatch")
    for key in ("worker", "wasm", "glue", "verifier"):
        entry = identity["files"][key]
        path = site.joinpath(*safe_parts(entry["path"]))
        require(digest(path.read_bytes()) == entry["sha256"] == provenance["runtime"]["files"][key], "runtime hash mismatch: " + key)
    print(json.dumps({"site": str(site), "host_revision": provenance["web_host_revision"],
                      "runtime_revision": identity["sourceRevision"], "build_id": identity["buildId"],
                      "host_files": host_count, "runtime_overlay_files": overlay_count,
                      "instrumented_host_files": list(provenance["instrumented_host_files"]),
                      "compiled_runtime_identity_verified": True}, separators=(",", ":")))


if __name__ == "__main__":
    main()
