#!/usr/bin/env python3
"""Verify hashes, optimizer records, identities, clocks and all timing arithmetic.

Core verification uses Python's standard library. --pixels additionally runs
the numeric screenshot comparison and requires Pillow 12.3.0. No network,
browser, compiler, installed Noon module, or source checkout is used.
"""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--pixels", action="store_true")
    args = parser.parse_args()
    root, output = args.package.resolve(), args.output.resolve()
    require(root not in output.parents and root != output, "replay output must be outside evidence package")
    require(not output.exists() or not any(output.iterdir()), "replay output must be empty")
    manifest = json.loads((root / "manifest.json").read_text())
    for item in manifest["files"]:
        path = root / item["path"]
        require(root in path.resolve().parents, "unsafe manifest path")
        data = path.read_bytes()
        require(len(data) == item["bytes"] and digest(data) == item["sha256"], "manifest mismatch: " + item["path"])
    provenance = json.loads((root / "provenance.json").read_text())
    source = json.loads((root / "source-control.json").read_text())
    sites = json.loads((root / "site-identities.json").read_text())
    proof = json.loads((root / "optimizer-proof.json").read_text())
    require(digest((root / "scene.py").read_bytes()) == provenance["source_sha256"], "scene hash mismatch")
    lock = gzip.decompress((root / provenance["cargo_lock_path"]).read_bytes())
    require(digest(lock) == provenance["cargo_lock_uncompressed_sha256"] == source["cargo_lock_sha256"], "Cargo.lock mismatch")
    require(proof["source_diff_files"] == [source["only_changed_source_file"]], "source comparison mismatch")
    require(digest((root / "audit-bin/wasm-opt").read_bytes()) == proof["wrapper_sha256"], "optimizer wrapper changed")
    expected_args = ["web/pkg/noon_web_bg.wasm", "-o", "web/pkg/noon_web_bg.wasm-opt.wasm", "-O"]
    for arm in proof["arms"]:
        records = [json.loads(line) for line in gzip.decompress((root / "build" / (arm["audit_file"] + ".gz")).read_bytes()).decode().splitlines() if line]
        require(len(records) == 1, "optimizer invocation count mismatch")
        event = records[0]
        require(event["args"] == expected_args and event["real_binary_sha256"] == proof["toolchain"]["binaryen_sha256"], "optimizer invocation mismatch")
        require(event["exit_code"] == 0 and "output_validation_error" not in event and not event["output_existed_before"], "optimizer output not admitted")
        require(event["output"]["sha256"] == arm["selected_sha256"] and event["output"]["bytes"] == arm["selected_bytes"], "selected WASM differs from optimizer output")
        require(all(arm[key] for key in ["selected_equals_optimizer_output", "temporary_absent_after_build", "copied_lock_unchanged", "original_staged_hash_reproduced"]), "recorded build guard did not pass")
        require(arm["runtime"] == sites["arms"][arm["arm"]]["runtime"], "optimizer/runtime identity mismatch")
        require(arm["selected_sha256"] == arm["runtime"]["files"]["wasm"]["sha256"], "selected/runtime WASM mismatch")
    output.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(root / "experiment.json", output / "experiment.json")
    (output / "Cargo.lock").write_bytes(lock)
    experiment = json.loads((output / "experiment.json").read_text())
    require(experiment["identity_gate"] == {"endpoint_counters": {"drawCalls": 154, "instancesDrawn": 10430, "objectCount": 10432}, "passed": True}, "endpoint identities mismatch")
    for capture in provenance["raw_captures"]:
        data = gzip.decompress((root / capture["report_path"]).read_bytes())
        require(len(data) == capture["uncompressed_bytes"] and digest(data) == capture["uncompressed_sha256"], "raw report mismatch")
        report = json.loads(data)
        runtime = sites["arms"][capture["arm"]]["runtime"]
        require(report["sourceSha256"] == provenance["source_sha256"] and report["expectedRevision"] == runtime["sourceRevision"], "capture workload/source identity mismatch")
        require(report["runtime"]["buildId"] == runtime["buildId"] and report["runtime"]["files"] == {key: value["sha256"] for key, value in runtime["files"].items()}, "capture runtime file identities mismatch")
        require(report["browserVersion"] == "151.0.7922.34" and "SwiftShader" in json.dumps(report["browserSystemInfo"]["gpu"]), "browser/GPU identity mismatch")
        require(report["status"] == "collected-unqualified" and not report.get("error") and not any(report[key] for key in ["errors", "requestsFailed", "consoleErrors"]), "capture error")
        require(len(report["runs"]) == 2, "wrong playback count")
        for run in report["runs"]:
            require(run["status"] == "endpoint-observed", "missing endpoint")
            sample = run["samples"][-1]
            require(sample["backend"] == "WebGL2" and sample["backing"] == [620,348] and sample["css"] == [621.671875,349.6875] and sample["dpr"] == 1, "surface identity mismatch")
            m = sample["metrics"]
            require(m["time"] == 25 and not m["needsPresent"] and m["bufferedDeltas"] == 0, "unsettled terminal")
            require({key: m["counters"][key] for key in experiment["identity_gate"]["endpoint_counters"]} == experiment["identity_gate"]["endpoint_counters"], "terminal draw/object identity mismatch")
        folder = output / capture["block"]
        folder.mkdir()
        (folder / "report.json").write_bytes(data)
        subprocess.run([sys.executable, "-B", str(root / "tools/analyze-instrumented.py"), str(folder / "report.json"), "--scene", str(root / "scene.py"), "--output", str(folder / "analysis.json")], check=True, stdout=subprocess.DEVNULL)
    subprocess.run([sys.executable, "-B", str(root / "tools/analyze-comparison.py"), str(output), "--output", str(output / "comparison.json")], check=True, stdout=subprocess.DEVNULL)
    result = json.loads((output / "comparison.json").read_text())
    require(result == json.loads((root / "summaries/comparison.json").read_text()), "timing summary does not reproduce exactly")
    if args.pixels:
        subprocess.run([sys.executable, "-B", str(root / "tools/compare-endpoints.py"), str(root), "--output", str(output / "terminal-pixel-comparison.json")], check=True, stdout=subprocess.DEVNULL)
        require(json.loads((output / "terminal-pixel-comparison.json").read_text()) == json.loads((root / "summaries/terminal-pixel-comparison.json").read_text()), "pixel arithmetic does not reproduce exactly")
    print(json.dumps({"status": "passed", "manifest_files": len(manifest["files"]), "raw_contexts": 4, "playbacks": 8, "source_sha256": provenance["source_sha256"], "optimizer_invocations_verified": 2, "timing_summary_reproduced_exactly": True, "pixel_comparison_reproduced_exactly": args.pixels, "output": str(output)}, separators=(",", ":")))


if __name__ == "__main__":
    main()
