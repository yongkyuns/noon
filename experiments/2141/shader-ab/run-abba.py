#!/usr/bin/env python3
"""Run four sequential shader-control capture blocks, failing closed on identity.

Each block starts a fresh Chromium process and performs two ordinary editor
executions (fresh browser, then fresh scene on the same page). The probe and
timed capture loop are unchanged from the prior instrumented experiment;
the harness additionally records CDP GPU identity after both timed runs.
No other browser or compiler should run during this capture window.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent
SOURCE_SHA = "32f2c6ee607bf9670ed47c77dd1f321ace576b5f725a523069bdcad54026bd2e"
BUILD_TOOLS = {"cargo", "rustc", "rust-lld", "wasm-opt", "wasm-pack", "ninja", "cc1", "cc1plus"}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def idle_snapshot():
    active = []
    for path in Path("/proc").glob("[0-9]*/comm"):
        try:
            name = path.read_text().strip()
            if name in BUILD_TOOLS:
                active.append({"pid": int(path.parent.name), "name": name})
        except (OSError, ValueError):
            pass
    require(not active, "compiler processes active: " + json.dumps(active))
    return {"epoch_seconds": time.time(), "load_average": os.getloadavg(), "known_build_processes": active}


def save(report, output):
    output.write_text(json.dumps(report, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--chrome", type=Path, required=True)
    parser.add_argument("--node", default="node")
    parser.add_argument("--output", type=Path, default=ROOT / "evidence")
    args = parser.parse_args()
    identities = json.loads((ROOT / "site-identities.json").read_text())
    require(sha(ROOT / "scene.py") == SOURCE_SHA, "frozen scene SHA256 mismatch")
    chrome_version = subprocess.check_output([str(args.chrome), "--version"], text=True).strip()
    require("151.0.7922.34" in chrome_version, "unexpected Chromium version")
    output = args.output.resolve()
    require(not output.exists() or not any(output.iterdir()), "capture output must be empty")
    output.mkdir(parents=True, exist_ok=True)
    manifest = {"schema": 1, "experiment": "same-host frozen-scene shader-only ABBA",
                "source_sha256": SOURCE_SHA, "chrome_version_command": chrome_version,
                "order": ["new", "old", "old", "new"], "runs_per_block": 2,
                "status": "incomplete", "blocks": [], "identity_gate": None}
    manifest_path = output / "experiment.json"
    try:
        expected_counters = None
        for index, arm in enumerate(manifest["order"]):
            identity = identities["arms"][arm]
            site = ROOT / "sites" / arm
            observed = json.loads((site / "runtime-build-identity.json").read_text())
            require(observed == identity["runtime"], "runtime identity changed")
            for role, entry in observed["files"].items():
                require(sha(site / entry["path"]) == entry["sha256"], "runtime file changed: " + role)
            for name, digest in identity["instrumented_host_sha256"].items():
                require(sha(site / name) == digest, "probe/host changed: " + name)
            folder = output / f"block-{index}-{arm}"
            block = {"index": index, "arm": arm, "path": folder.name, "runtime_revision": observed["sourceRevision"],
                     "runtime_build_id": observed["buildId"], "before": idle_snapshot(), "status": "running"}
            manifest["blocks"].append(block)
            save(manifest, manifest_path)
            print(json.dumps({"event": "block_start", "index": index, "arm": arm, "revision": observed["sourceRevision"]}), flush=True)
            environment = dict(os.environ, NOON_BASELINE_SITE=str(site), NOON_BASELINE_SCENE=str(ROOT / "scene.py"),
                               NOON_BASELINE_OUTPUT=str(folder), NOON_BASELINE_REVISION=observed["sourceRevision"],
                               NOON_BASELINE_RUNS="2", NOON_PROFILE_CHROME=str(args.chrome.resolve()))
            subprocess.run([args.node, str(ROOT / "harness/frozen-scene-profile.mjs")], env=environment, check=True)
            report = json.loads((folder / "report.json").read_text())
            require(report["status"] == "collected-unqualified" and not report.get("error"), "capture failed")
            require(report["browserVersion"] == "151.0.7922.34", "capture browser mismatch")
            require(report["sourceSha256"] == SOURCE_SHA, "capture scene mismatch")
            require(report["expectedRevision"] == observed["sourceRevision"] and report["runtime"]["buildId"] == observed["buildId"], "capture runtime mismatch")
            gpu = report["browserSystemInfo"]["gpu"]
            require("SwiftShader" in json.dumps(gpu), "captured GPU identity does not establish SwiftShader")
            block["gpu_devices"] = gpu["devices"]
            require(not report["errors"] and not report["requestsFailed"] and not report["consoleErrors"], "capture environment error")
            require(len(report["runs"]) == 2, "unexpected playback count")
            terminals = []
            for run in report["runs"]:
                require(run["status"] == "endpoint-observed", "endpoint was not observed")
                sample = run["samples"][-1]
                require(sample["backend"] == "WebGL2" and sample["backing"] == [620, 348] and sample["dpr"] == 1, "render environment mismatch")
                metric = sample["metrics"]
                require(metric["time"] == 25 and not metric["needsPresent"] and metric["bufferedDeltas"] == 0, "endpoint not settled")
                counters = {key: metric["counters"][key] for key in ("drawCalls", "instancesDrawn", "objectCount")}
                require(counters["objectCount"] == 10432 and counters["instancesDrawn"] == 10430, "frozen endpoint content mismatch")
                require(0 < counters["drawCalls"] < 500, "packed draw-call path not active")
                if expected_counters is None:
                    expected_counters = counters
                require(counters == expected_counters, "shader arms/runs disagree on endpoint draw or object identities")
                require(len(run["workerProfiles"]) == 2 and all(profile["profile"].get("dropped") == 0 for profile in run["workerProfiles"]), "worker profile missing or truncated")
                terminals.append({"index": run["index"], "kind": run["kind"], "elapsed_ms": run["elapsedMs"], "counters": counters})
            subprocess.run([sys.executable, "-B", str(ROOT / "tools/analyze-instrumented.py"), str(folder / "report.json"),
                            "--scene", str(ROOT / "scene.py"), "--output", str(folder / "analysis.json")], check=True,
                           stdout=(folder / "analysis-stdout.json").open("w"))
            block.update(status="complete", after=idle_snapshot(), runs=terminals)
            manifest["identity_gate"] = {"endpoint_counters": expected_counters, "passed": True}
            save(manifest, manifest_path)
            print(json.dumps({"event": "block_complete", "index": index, "arm": arm, "runs": terminals}), flush=True)
        manifest["status"] = "complete"
    except BaseException as error:
        manifest["error"] = repr(error)
        raise
    finally:
        save(manifest, manifest_path)


if __name__ == "__main__":
    main()
