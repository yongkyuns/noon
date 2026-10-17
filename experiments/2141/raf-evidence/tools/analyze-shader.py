#!/usr/bin/env python3
"""Verify and summarize the separate historical shader experiment (stdlib only)."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
import statistics
import zipfile

EXPECTED_ZIP = "0a70f6f8623901df4246ba92c9ae81c1a1fb3c3ba758bc87a489a6e19eb84557"
PREFIX = "browser-smoke-artifacts/triangle-profile/renderer/"
REVISIONS = {"baseline": "6c6c0398c97fbbb0b09deb9a6e7485660ffdd5ed",
             "candidate": "3567f45cf651a429ac02d0668b83896120afc46f"}
FROZEN_SHA = "32f2c6ee607bf9670ed47c77dd1f321ace576b5f725a523069bdcad54026bd2e"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    archive_hash = hashlib.sha256(args.archive.read_bytes()).hexdigest()
    require(archive_hash == EXPECTED_ZIP, "historical shader archive SHA256 mismatch")
    pairs = []
    with zipfile.ZipFile(args.archive) as archive:
        report = json.loads(archive.read(PREFIX + "report.json"))
        require(report["browser"] == "151.0.7922.34", "unexpected historical browser")
        for engine, revision in REVISIONS.items():
            require(report["identities"][engine]["sourceRevision"] == revision, "source revision mismatch")
        require(len(report["cases"]) == 8, "unexpected historical case count")
        for name in ("dot", "small-fill", "large-fill", "full"):
            pair = {"name": name, "engines": {}}
            for engine in REVISIONS:
                matches = [case for case in report["cases"] if case["name"] == name and case["engine"] == engine]
                require(len(matches) == 1, "missing or duplicate historical case")
                case = matches[0]
                require(case["outcome"] == "pass" and not case["pageErrors"], "historical case failed")
                png_hash = hashlib.sha256(archive.read(PREFIX + name + "-" + engine + ".png")).hexdigest()
                require(png_hash == case["pngSha256"], "historical screenshot SHA256 mismatch")
                raw = json.loads(archive.read(PREFIX + name + "-" + engine + ".json"))
                require(raw["state"] == "complete", "incomplete historical raw profile")
                samples = raw["report"]["samples"]
                require(len(samples) == 90, "unexpected measured frame count")
                intervals = [b["frameTimestampMs"] - a["frameTimestampMs"] for a, b in zip(samples, samples[1:])]
                mean_interval = statistics.mean(intervals)
                summary = case["summary"]
                require(math.isclose(mean_interval, summary["cadence"]["frameIntervalMs"]["mean"], abs_tol=1e-8),
                        "raw frame intervals do not reproduce historical summary")
                require(summary["setup"]["warmupFrames"] == 24, "unexpected warmup count")
                environment = summary["environment"]
                require(environment["backingResolution"] == [1152, 648], "unexpected backing size")
                require(environment["rendererGpuIdentity"]["deviceType"] == "Cpu", "not the captured software renderer")
                pair["engines"][engine] = {
                    "source_sha256": case["sourceSha256"],
                    "frame_interval_mean_ms": mean_interval,
                    "rust_drive_mean_ms": summary["stages"]["rustDriveMs"]["mean"],
                    "renderer_call_mean_ms": summary["stages"]["lastRendererCallMs"]["mean"],
                    "draw_calls_mean": summary["stages"]["drawCalls"]["mean"],
                    "measured_frames": len(samples), "frame_intervals": len(intervals),
                }
            baseline, candidate = pair["engines"]["baseline"], pair["engines"]["candidate"]
            require(baseline["source_sha256"] == candidate["source_sha256"], "historical pair workload mismatch")
            require(baseline["source_sha256"] != FROZEN_SHA, "unexpected frozen workload conflation")
            pair["source_sha256"] = baseline["source_sha256"]
            pair["baseline_to_candidate_frame_interval_ratio"] = baseline["frame_interval_mean_ms"] / candidate["frame_interval_mean_ms"]
            pair["pixel_comparison"] = next(row for row in report["pixelComparisons"] if row["name"] == name)
            pairs.append(pair)
    result = {"schema": 1, "archive_sha256": archive_hash,
              "purpose": report["purpose"], "browser_version": report["browser"],
              "identities": report["identities"], "profile_harness_sha256": report["profileHarnessSha256"],
              "pairs": pairs,
              "limits": ["This is a different scene and replay cadence experiment from the frozen 10000-ray ordinary playback.",
                         "Forced SwiftShader CPU rendering at 1152x648, with fixed baseline-then-candidate order.",
                         "CPU render return is not GPU completion; stage means and frame intervals overlap.",
                         "The full-scene screenshot differs in one channel by one level; do not label it pixel-exact."]}
    encoded = json.dumps(result, separators=(",", ":")) + "\n"
    if args.output:
        args.output.write_text(encoded)
    print(encoded, end="")


if __name__ == "__main__":
    main()
