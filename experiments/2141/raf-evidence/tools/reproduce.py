#!/usr/bin/env python3
"""Verify all preserved evidence and regenerate analyses offline (Python stdlib)."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
from pathlib import Path, PurePosixPath
import subprocess
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def extract_zip(source, destination):
    with zipfile.ZipFile(source) as archive:
        for member in archive.infolist():
            path = PurePosixPath(member.filename)
            require(not path.is_absolute() and ".." not in path.parts and "\\" not in member.filename, "unsafe ZIP path")
            target = destination.joinpath(*path.parts)
            if member.is_dir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(archive.read(member))


def run(script, arguments):
    process = subprocess.run([sys.executable, "-B", str(ROOT / "tools" / script), *map(str, arguments)],
                             check=True, text=True, capture_output=True)
    return json.loads(process.stdout)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="New or empty output directory outside this package")
    args = parser.parse_args()
    output = args.output.resolve()
    require(ROOT != output and ROOT not in output.parents, "output must be outside evidence package")
    require(not output.exists() or (output.is_dir() and not any(output.iterdir())), "output must be empty")
    manifest = json.loads((ROOT / "manifest.json").read_text())
    for entry in manifest["files"]:
        path = PurePosixPath(entry["path"])
        require(not path.is_absolute() and ".." not in path.parts, "unsafe manifest path")
        data = ROOT.joinpath(*path.parts).read_bytes()
        require(len(data) == entry["bytes"] and hashlib.sha256(data).hexdigest() == entry["sha256"],
                "package integrity mismatch: " + entry["path"])
    output.mkdir(parents=True, exist_ok=True)
    extract_zip(ROOT / "raw/prior-frozen-vector.zip", output / "prior")
    report = gzip.decompress((ROOT / "raw/local-chromium151-report.json.gz").read_bytes())
    provenance = json.loads((ROOT / "provenance.json").read_text())
    require(hashlib.sha256(report).hexdigest() == provenance["current_capture"]["uncompressed_sha256"], "raw report SHA256 mismatch")
    (output / "current-report.json").write_bytes(report)
    prior = run("analyze-prior.py", [output / "prior", "--output", output / "prior-analysis.json"])
    current = run("analyze-instrumented.py", [output / "current-report.json", "--scene", ROOT / "workload/scene.py",
                                             "--output", output / "current-analysis.json"])
    shader = run("analyze-shader.py", [ROOT / "raw/historical-shader.zip", "--output", output / "shader-analysis.json"])
    for result in (prior, current):
        result.pop("output", None)
    result = {"schema": 1, "package_files_verified": len(manifest["files"]),
              "prior": prior, "current": current,
              "historical_shader": {"browser_version": shader["browser_version"], "pairs": shader["pairs"]}}
    expected_path = ROOT / "summaries/findings.json"
    if expected_path.exists():
        expected = json.loads(expected_path.read_text())
        # Manifest grows by one after the first findings file is produced.
        expected.pop("package_files_verified", None)
        actual = dict(result)
        actual.pop("package_files_verified", None)
        require(actual == expected, "replayed findings differ from preserved expected findings")
    (output / "findings.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, separators=(",", ":")))


if __name__ == "__main__":
    main()
