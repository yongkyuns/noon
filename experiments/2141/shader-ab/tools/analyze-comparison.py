#!/usr/bin/env python3
"""Aggregate the four ABBA blocks descriptively, preserving clock denominators."""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
import statistics


def require(condition, message):
    if not condition:
        raise ValueError(message)


def describe(values):
    return {"n": len(values), "values": values, "min": min(values),
            "median": statistics.median(values), "max": max(values)}


def rounded(value):
    if isinstance(value, float):
        return round(value, 6)
    if isinstance(value, list):
        return [rounded(item) for item in value]
    if isinstance(value, dict):
        return {key: rounded(item) for key, item in value.items()}
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("evidence", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    experiment = json.loads((args.evidence / "experiment.json").read_text())
    require(experiment["status"] == "complete" and experiment["identity_gate"]["passed"], "experiment incomplete")
    require(experiment["order"] == ["new", "old", "old", "new"], "not the planned ABBA order")
    require(len(experiment["blocks"]) == 4, "unexpected block count")
    rows = []
    for block in experiment["blocks"]:
        require(block["status"] == "complete", "block incomplete")
        analysis = json.loads((args.evidence / block["path"] / "analysis.json").read_text())
        require(analysis["source_sha256"] == experiment["source_sha256"], "workload identity mismatch")
        require(analysis["runtime_revision"] == block["runtime_revision"] and analysis["runtime"]["buildId"] == block["runtime_build_id"], "runtime identity mismatch")
        require(len(analysis["runs"]) == 2, "unexpected within-context run count")
        for run in analysis["runs"]:
            fast = run["fast_flights"]
            require(fast["count"] == 80, "unexpected fast-flight count")
            flights = run["per_flight"][4:]
            matched = [flight for flight in flights if flight["post_initial_raf_attribution"] is not None]
            matched_gap = math.fsum(flight["post_initial_first_advance_gap_ms"] for flight in matched)
            components = {key: math.fsum(flight["post_initial_raf_attribution"]["partition_ms"][key] for flight in matched)
                          for key in ("before_raf_request_ms", "waiting_for_raf_callback_ms", "after_raf_callback_to_first_advance_receive_ms")}
            require(abs(math.fsum(components.values()) - matched_gap) < 1e-5, "aggregate partition mismatch")
            wait_fraction = components["waiting_for_raf_callback_ms"] / matched_gap
            require(abs(wait_fraction - fast["raf_wait_fraction_of_matched_post_initial_gaps"]) < 1e-8, "fraction denominator mismatch")
            drive = run["presentation_window_profile"]["engine_api"]["driveLiveSegmentFromWallTime"]
            rows.append({"block": block["index"], "arm": block["arm"], "within_context_run": run["index"],
                         "condition": "fresh_browser_first_run" if run["index"] == 0 else "same_page_repeat",
                         "click_to_endpoint_ms": run["timing"]["click_to_observed_endpoint_ms"],
                         "presentation_to_handoff_ms": run["timing"]["first_actual_presentation_to_handoff_ms"],
                         "observed_engine_clock_to_handoff_ms": run["timing"]["first_engine_clock_observation_to_handoff_ms"],
                         "fast_flight_wall_ms": fast["wall_ms"],
                         "fast_flights_with_interior": fast["with_interior_presentations"],
                         "fast_interior_publications": fast["interior_presentation_count"],
                         "fast_all_post_initial_gaps_ms": fast["post_initial_first_advance_gap_ms"]["sum_ms"],
                         "raf_matched_fast_flights": len(matched), "raf_matched_gap_ms": matched_gap,
                         "raf_wait_ms": components["waiting_for_raf_callback_ms"],
                         "raf_wait_fraction_of_matched_gaps": wait_fraction,
                         "raf_partition_ms": components,
                         "drive_in_presentation_window_count": drive["count"],
                         "drive_in_presentation_window_ms": drive.get("sum_ms", 0),
                         "drive_in_fast_gaps_ms": math.fsum(flight["drives_inside_post_initial_gap_ms"].get("sum_ms", 0) for flight in flights),
                         "all_capture_drive_ms": run["all_current_capture_drive_calls_ms"]["sum_ms"],
                         "all_capture_drive_count": run["all_current_capture_drive_calls_ms"]["count"],
                         "raf_request_to_fire_ms": run["presentation_window_profile"]["raf_request_to_fire_ms"],
                         "excluded_other_session_stages": run["excluded_other_session_stage_count"],
                         "warm_prefix_events_removed": [profile["prior_event_prefix_removed"] for profile in run["worker_profile_deltas"]]})
    summaries = []
    metrics = ["click_to_endpoint_ms", "presentation_to_handoff_ms", "fast_flight_wall_ms",
               "fast_flights_with_interior", "fast_interior_publications", "raf_matched_fast_flights",
               "raf_matched_gap_ms", "raf_wait_ms", "raf_wait_fraction_of_matched_gaps",
               "drive_in_presentation_window_ms", "drive_in_fast_gaps_ms"]
    for condition in ("fresh_browser_first_run", "same_page_repeat"):
        for arm in ("new", "old"):
            selected = [row for row in rows if row["condition"] == condition and row["arm"] == arm]
            require(len(selected) == 2, "expected exactly two contexts per arm/condition")
            summaries.append({"arm": arm, "condition": condition, "contexts": [row["block"] for row in selected],
                              "metrics": {key: describe([row[key] for row in selected]) for key in metrics}})
    ratios = []
    for condition in ("fresh_browser_first_run", "same_page_repeat"):
        new = next(row for row in summaries if row["condition"] == condition and row["arm"] == "new")
        old = next(row for row in summaries if row["condition"] == condition and row["arm"] == "old")
        ratios.append({"condition": condition, "statistic": "old/new ratio of descriptive medians; n=2 per condition/arm",
                       "ratios": {key: old["metrics"][key]["median"] / new["metrics"][key]["median"]
                                  for key in ("click_to_endpoint_ms", "presentation_to_handoff_ms", "fast_flight_wall_ms", "raf_wait_ms")}})
    result = rounded({"schema": 1, "source_sha256": experiment["source_sha256"], "order": experiment["order"],
                      "contexts_per_arm": 2, "playbacks_per_arm": 4,
                      "endpoint_identity": experiment["identity_gate"], "runs": rows,
                      "condition_summaries": summaries, "descriptive_median_ratios": ratios,
                      "limitations": ["Two fresh-browser contexts and four playbacks per arm; descriptive observations, not confidence estimates.",
                                     "The raw cold label means fresh-browser first run, not cold OS, tool, download, or shader caches.",
                                     "RAF fractions divide clipped RAF waiting by matched post-initial gaps, not total playback or fast-phase wall time.",
                                     "Renderer acknowledgement is CPU-side publication, not GPU completion or physical scanout.",
                                     "API time, RAF waits, and scene elapsed overlap; only explicit validated interval partitions are additive.",
                                     "Same host, exact scene, forced SwiftShader WebGL2, and ABBA order; no physical-GPU or universal user-device claim."]})
    encoded = json.dumps(result, separators=(",", ":")) + "\n"
    if args.output:
        args.output.write_text(encoded)
    print(encoded, end="")


if __name__ == "__main__":
    main()
