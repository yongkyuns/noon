#!/usr/bin/env python3
"""Reproduce renderer-publication forensics for the frozen 10,000-ray artifact.

Standard library only. Does not import Noon or change the scene/repository.
All interval partitions use one renderer worker's performance clock. They do
not identify CPU ownership or measure GPU duration/physical presentation.
"""

from __future__ import annotations

import argparse
import ast
import bisect
from collections import Counter
import hashlib
import json
import math
from pathlib import Path
import statistics


EXPECTED_SCENE_SHA256 = "32f2c6ee607bf9670ed47c77dd1f321ace576b5f725a523069bdcad54026bd2e"
RUN_NAMES = ("baseline-1", "baseline-4", "candidate-2", "candidate-3")
TIME_TOLERANCE_SECONDS = 1e-8
INTERVAL_TOLERANCE_MS = 1e-6


def require(condition, message):
    if not condition:
        raise ValueError(message)


def close_time(a, b):
    return math.isclose(a, b, abs_tol=TIME_TOLERANCE_SECONDS, rel_tol=0)


def read_json(path):
    return json.loads(path.read_text())


def rounded(value):
    if isinstance(value, float):
        require(math.isfinite(value), "nonfinite output value")
        return round(value, 9)
    if isinstance(value, dict):
        return {key: rounded(item) for key, item in value.items()}
    if isinstance(value, (tuple, list)):
        return [rounded(item) for item in value]
    return value


def stats(values):
    require(bool(values), "cannot summarize an empty interval set")
    return {
        "count": len(values), "sum": math.fsum(values),
        "min": min(values), "median": statistics.median(values),
        "mean": statistics.mean(values), "max": max(values),
    }


def recover_geometry_and_boundaries(scene_bytes):
    actual_hash = hashlib.sha256(scene_bytes).hexdigest()
    require(actual_hash == EXPECTED_SCENE_SHA256,
            f"frozen scene hash mismatch: expected {EXPECTED_SCENE_SHA256}, got {actual_hash}")
    # Only execute the hash-pinned pure geometry helpers and required numeric
    # assignments. Exclude imports, all authoring functions and the scene class.
    functions = {"xy", "cross", "difference", "trace_ray", "flight_durations", "pink_flight_durations"}
    names = {"ART_WIDTH", "ART_HEIGHT", "ART_SCALE", "TRIANGLE", "LAUNCH_POINT",
             "LAUNCH_ANGLE_DEGREES", "BOUNCES", "FLIGHT_SECONDS", "SLOW_BOUNCES",
             "SLOW_FLIGHT_SECONDS", "COLOR_BOUNCES", "COLOR_FLIGHT_SECONDS",
             "DENSE_BOUNCES", "EXPOSURE_COUNTS", "EXPOSURE_SECONDS"}
    retained = []
    for node in ast.parse(scene_bytes.decode("utf-8")).body:
        if isinstance(node, ast.FunctionDef) and node.name in functions:
            retained.append(node)
        elif isinstance(node, ast.Assign):
            assigned = {child.id for target in node.targets for child in ast.walk(target)
                        if isinstance(child, ast.Name)}
            if assigned and assigned <= names:
                retained.append(node)
    namespace = {"math": math}
    exec(compile(ast.Module(body=retained, type_ignores=[]), "hash-pinned-geometry", "exec"), namespace)
    require(namespace["BOUNCES"] == 36 and namespace["COLOR_BOUNCES"] == 48
            and namespace["DENSE_BOUNCES"] == 10000 and namespace["SLOW_BOUNCES"] == 4,
            "unexpected frozen-scene workload")
    points, _ = namespace["trace_ray"](
        namespace["TRIANGLE"], namespace["LAUNCH_POINT"], namespace["LAUNCH_ANGLE_DEGREES"], 84)
    pink = namespace["pink_flight_durations"](points[:37], namespace["FLIGHT_SECONDS"])
    color = namespace["flight_durations"](points[36:], namespace["COLOR_FLIGHT_SECONDS"])
    flights = []
    authored_time = 0.8  # Hash-pinned opening self.play(run_time=0.8).
    for index, duration in enumerate(pink + color):
        if index == 36:
            authored_time += 0.6  # Hash-pinned pose transition between sections.
        start = authored_time
        authored_time += duration
        flights.append({"bounce": index + 1, "start_seconds": start,
                        "end_seconds": authored_time, "duration_seconds": duration})
    require(close_time(flights[3]["end_seconds"], 5.6), "slow phase boundary mismatch")
    require(close_time(flights[35]["end_seconds"], 8.8), "pink phase boundary mismatch")
    require(close_time(flights[36]["start_seconds"], 9.4), "color start mismatch")
    require(close_time(authored_time, 14.4), "flight endpoint mismatch")
    return flights


def analyze_run(root, name, flights):
    report = read_json(root / name / "report.json")
    require(report["sourceSha256"] == EXPECTED_SCENE_SHA256, f"{name}: report scene hash mismatch")
    warm = read_json(root / name / "run-1.json")
    cold = read_json(root / name / "run-0.json")
    require(warm["kind"] == "fresh-scene-same-page", f"{name}: unexpected warm run kind")
    require(cold["stageSamples"] == [], f"{name}: cold run stage availability changed")
    samples = warm["stageSamples"]
    require(bool(samples), f"{name}: missing warm stage samples")
    session = samples[0]["session"]
    stages = [sample for sample in samples if sample["session"] == session]
    require([sample["sequence"] for sample in stages] == list(range(1, len(stages) + 1)),
            f"{name}: missing or reordered publication sequence")
    require(len({(sample["session"], sample["sequence"]) for sample in samples}) == len(samples),
            f"{name}: duplicate publication identities")
    origin = stages[0]["clockOriginMs"]
    required_numbers = ("time", "receivedAtMs", "presentedAtMs", "applyMs", "receiveToPresentMs",
                        "renderMs", "ackPostMs", "renderToAckMs", "clockOriginMs")
    for stage in stages:
        require(all(isinstance(stage[key], (int, float)) and math.isfinite(stage[key])
                    for key in required_numbers), f"{name}: invalid stage numeric field")
        require(stage["clockOriginMs"] == origin, f"{name}: renderer origin changed")
        require(abs(stage["presentedAtMs"] - stage["receivedAtMs"] - stage["receiveToPresentMs"])
                <= INTERVAL_TOLERANCE_MS, f"{name}: receive-to-present field mismatch")
    for previous, current in zip(stages, stages[1:]):
        require(current["time"] + TIME_TOLERANCE_SECONDS >= previous["time"],
                f"{name}: authored time regressed")
        require(current["receivedAtMs"] >= previous["presentedAtMs"] - INTERVAL_TOLERANCE_MS,
                f"{name}: renderer publication intervals overlap")

    def exact(time):
        return [stage for stage in stages if close_time(stage["time"], time)]

    def first(time):
        found = exact(time)
        require(bool(found), f"{name}: missing exact authored boundary {time}")
        return found[0]

    phases = []
    boundaries = (0.8, 5.6, 8.8, 9.4, 14.4, 15, 16, 17, 18, 19, 20, 21, 22, 23)
    for start, end in zip(boundaries, boundaries[1:]):
        previous, current = first(start), first(end)
        interval = [stage for stage in stages if previous["sequence"] < stage["sequence"] <= current["sequence"]]
        phases.append({"start_seconds": start, "end_seconds": end, "authored_seconds": end - start,
                       "wall_seconds": (current["presentedAtMs"] - previous["presentedAtMs"]) / 1000,
                       "publications": len(interval),
                       "renderer_call_sum_ms": math.fsum(stage["renderMs"] for stage in interval),
                       "delta_apply_sum_ms": math.fsum(stage["applyMs"] for stage in interval)})

    all_metrics = [sample["metrics"] for sample in warm["samples"]
                   if sample.get("metrics") and sample["metrics"]["session"] == session]
    unique_metrics = {metric["metricReply"]: metric for metric in all_metrics}
    metrics = sorted(unique_metrics.values(), key=lambda metric: metric["rendererAt"])
    require(all(metric["clockOriginMs"] == origin for metric in metrics), f"{name}: metric clock mismatch")
    metric_times = [metric["rendererAt"] for metric in metrics]
    fast = []
    metrics_inside_gaps = []
    for flight in flights[4:]:
        start, end = flight["start_seconds"], flight["end_seconds"]
        interior = [stage for stage in stages
                    if start + TIME_TOLERANCE_SECONDS < stage["time"] < end - TIME_TOLERANCE_SECONDS]
        prior_boundary, endpoint = exact(start), exact(end)
        require(len(interior) == 0, f"{name}: bounce {flight['bounce']} has an interior presentation")
        require(len(prior_boundary) == 3 and len(endpoint) == 3,
                f"{name}: bounce {flight['bounce']} boundary does not have three publications")
        ready, cleanup, initial = prior_boundary
        advancing = endpoint[0]
        require(advancing["sequence"] == initial["sequence"] + 1,
                f"{name}: an unexpected publication occurs inside fast flight")
        ack_end = lambda stage: stage["presentedAtMs"] + stage["renderToAckMs"]
        partition = {
            "previous_endpoint_ack_tail_ms": ready["renderToAckMs"],
            "endpoint_ack_to_cleanup_receive_ms": cleanup["receivedAtMs"] - ack_end(ready),
            "cleanup_receive_to_present_ms": cleanup["receiveToPresentMs"],
            "cleanup_ack_tail_ms": cleanup["renderToAckMs"],
            "cleanup_ack_to_next_initial_receive_ms": initial["receivedAtMs"] - ack_end(cleanup),
            "initial_receive_to_present_ms": initial["receiveToPresentMs"],
            "initial_ack_tail_ms": initial["renderToAckMs"],
            "next_initial_ack_to_first_advance_receive_ms": advancing["receivedAtMs"] - ack_end(initial),
            "advancing_receive_to_present_ms": advancing["receiveToPresentMs"],
        }
        wall = advancing["presentedAtMs"] - ready["presentedAtMs"]
        require(min(partition.values()) >= -INTERVAL_TOLERANCE_MS, f"{name}: negative partition interval")
        require(abs(math.fsum(partition.values()) - wall) <= INTERVAL_TOLERANCE_MS,
                f"{name}: partition does not equal endpoint-to-endpoint wall interval")
        gap_start, gap_end = ack_end(initial), advancing["receivedAtMs"]
        within = metrics[bisect.bisect_right(metric_times, gap_start):bisect.bisect_left(metric_times, gap_end)]
        metrics_inside_gaps.extend(within)
        fast.append({**flight, "interior_presentations": len(interior), "endpoint_publications": len(endpoint),
                     "previous_endpoint_sequence": ready["sequence"], "initial_sequence": initial["sequence"],
                     "next_endpoint_sequence": advancing["sequence"], "wall_ms": wall,
                     "partition_ms": partition, "metric_replies_during_post_initial_gap": len(within)})

    partitions = {key: stats([flight["partition_ms"][key] for flight in fast])
                  for key in fast[0]["partition_ms"]}
    wall_ms = math.fsum(flight["wall_ms"] for flight in fast)
    post_start_ms = partitions["next_initial_ack_to_first_advance_receive_ms"]["sum"]
    reply_intervals = [current["rendererAt"] - previous["rendererAt"]
                       for previous, current in zip(metrics_inside_gaps, metrics_inside_gaps[1:])
                       if current["metricReply"] == previous["metricReply"] + 1]
    gap_metrics = {
        "fresh_reply_count": len({metric["metricReply"] for metric in metrics_inside_gaps}),
        "replies_per_gap": stats([flight["metric_replies_during_post_initial_gap"] for flight in fast]),
        "consecutive_reply_interval_ms": stats(reply_intervals),
        "needs_present_true_count": sum(bool(metric["needsPresent"]) for metric in metrics_inside_gaps),
        "max_buffered_deltas": max(metric["bufferedDeltas"] for metric in metrics_inside_gaps),
    }
    return {
        "name": name, "source_revision": report["expectedRevision"], "browser_version": report["browserVersion"],
        "report_status": report["status"], "kind": warm["kind"], "warm_elapsed_ms": warm["elapsedMs"],
        "cold_stage_sample_count": len(cold["stageSamples"]), "warm_stage_sample_count": len(samples),
        "stage_sessions": dict(Counter(stage["session"] for stage in samples)),
        "analyzed_session": session, "clock_origin_ms": origin,
        "fast_flights": {
            "count": len(fast), "authored_seconds": math.fsum(flight["duration_seconds"] for flight in fast),
            "zero_interior_count": sum(flight["interior_presentations"] == 0 for flight in fast),
            "three_endpoint_publications_count": sum(flight["endpoint_publications"] == 3 for flight in fast),
            "wall_ms": wall_ms, "post_initial_gap_fraction": post_start_ms / wall_ms,
            "partitions_ms": partitions, "metrics_during_post_initial_gaps": gap_metrics,
        },
        "phases": phases, "per_fast_flight": fast,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artifact_dir", nargs="?", type=Path, default=Path("/tmp/noon-2141-benchmark"))
    parser.add_argument("--output", type=Path, help="Write complete compact JSON here; print only a summary to stdout")
    args = parser.parse_args()
    flights = recover_geometry_and_boundaries((args.artifact_dir / "scene.py").read_bytes())
    runs = [analyze_run(args.artifact_dir, name, flights) for name in RUN_NAMES]
    report = rounded({
        "schema": 1, "source_sha256": EXPECTED_SCENE_SHA256,
        "artifact_directory": str(args.artifact_dir.resolve()),
        "method": {
            "fast_flights": "Exact geometry-derived bounces 5-84; first prior endpoint presentation to first next endpoint presentation.",
            "three_publications_mapping": "Source interpretation: endpoint-ready, completeLiveSegment cleanup, startContinuation initialization.",
            "partition_clock": "All partition timestamps use the same render-worker performance clock; ack means renderer ack-post completion.",
            "render_apply_semantics": "Phase render and apply sums are subsets of receive-to-present intervals, not additional wall-time components.",
            "gpu_limit": "Renderer return is not GPU completion or physical display; post-initial gaps do not distinguish RAF/GPU scheduling from engine drive.",
            "qualification": "Existing collected-unqualified artifact; cold runs have no stage samples; no workload or runtime modifications.",
        },
        "authored_flights": flights, "runs": runs,
    })
    if args.output:
        args.output.write_text(json.dumps(report, separators=(",", ":")) + "\n")
        summary = {"source_sha256": EXPECTED_SCENE_SHA256, "output": str(args.output.resolve()), "runs": []}
        for run in report["runs"]:
            fast = run["fast_flights"]
            parts = fast["partitions_ms"]
            gap = parts["next_initial_ack_to_first_advance_receive_ms"]
            authoring = parts["cleanup_ack_to_next_initial_receive_ms"]
            summary["runs"].append({
                "name": run["name"], "fast_flights": fast["count"],
                "zero_interior": fast["zero_interior_count"],
                "three_endpoint_publications": fast["three_endpoint_publications_count"],
                "wall_seconds": fast["wall_ms"] / 1000,
                "post_initial_gap_seconds": gap["sum"] / 1000,
                "post_initial_gap_median_ms": gap["median"],
                "post_initial_gap_fraction": fast["post_initial_gap_fraction"],
                "cleanup_ack_to_next_initial_total_ms": authoring["sum"],
                "fresh_metric_replies_inside_gaps": fast["metrics_during_post_initial_gaps"]["fresh_reply_count"],
            })
        print(json.dumps(rounded(summary), separators=(",", ":")))
    else:
        print(json.dumps(report, separators=(",", ":")))


if __name__ == "__main__":
    main()
