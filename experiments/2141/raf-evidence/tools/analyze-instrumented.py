#!/usr/bin/env python3
"""Analyze instrumented exact-scene runs without double-counting warm profiles.

Requires adjacent analyze-prior.py for the hash-pinned pure geometry decoder.
Only stdlib is used. API/RAF timings overlap wall-clock and each other: do not
add them to obtain a purported total. Profile totals are cumulative; this
script validates and removes prior event prefixes and differences count/sum
totals. Cumulative maxima are never subtracted.
"""

from __future__ import annotations

import argparse
from collections import Counter
import importlib.util
import json
import math
from pathlib import Path
import statistics
from urllib.parse import urlparse


SPEC = importlib.util.spec_from_file_location("prior_forensics", Path(__file__).with_name("analyze-prior.py"))
PRIOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PRIOR)
require = PRIOR.require
close_time = PRIOR.close_time


def distribution(values, include_sum=False):
    if not values:
        return {"count": 0}
    ordered = sorted(values)
    def percentile(fraction):
        # Explicit nearest-rank quantile, avoiding library/version ambiguity.
        return ordered[max(0, math.ceil(fraction * len(ordered)) - 1)]
    result = {"count": len(values), "min_ms": ordered[0], "median_ms": statistics.median(values),
              "p95_ms": percentile(.95), "max_ms": ordered[-1], "mean_ms": statistics.mean(values)}
    if include_sum:
        result["sum_ms"] = math.fsum(values)
    return result


def profile_delta(wrapper, previous, renderer_origin):
    profile = wrapper["profile"]
    require(profile.get("schema") == 1 and profile.get("dropped") == 0,
            "missing or truncated worker profile")
    identity = (urlparse(wrapper["url"]).path, profile["timeOrigin"])
    before = previous.get(identity, {"events": [], "totals": {}})
    prefix = len(before["events"])
    require(profile["events"][:prefix] == before["events"], "cumulative event prefix changed")
    offset = profile["timeOrigin"] - renderer_origin
    events = []
    for event in profile["events"][prefix:]:
        converted = dict(event, renderer_at_ms=event["at"] + offset)
        if "start" in event:
            converted["renderer_start_ms"] = event["start"] + offset
        events.append(converted)
    totals = []
    for name, total in profile["totals"].items():
        old = before["totals"].get(name, {"count": 0, "ms": 0})
        count, elapsed = total["count"] - old["count"], total["ms"] - old["ms"]
        require(count >= 0 and elapsed >= -1e-6, "cumulative API totals regressed")
        if count:
            totals.append({"name": name, "count": count, "sum_ms": max(0, elapsed),
                           "mean_ms": max(0, elapsed) / count})
    previous[identity] = profile
    return {"url": wrapper["url"], "time_origin_ms": profile["timeOrigin"],
            "prior_event_prefix_removed": prefix, "current_event_count": len(events),
            "event_types": dict(Counter(event["type"] for event in events)),
            "api_totals_since_previous_capture": sorted(totals, key=lambda row: row["sum_ms"], reverse=True),
            "events": events}


def select_window(events, lower, upper):
    return [event for event in events if lower <= event["renderer_at_ms"] <= upper]


def window_profile(engine, renderer, lower, upper):
    engine_events = select_window(engine, lower, upper)
    renderer_events = select_window(renderer, lower, upper)
    names = ("driveLiveSegmentFromWallTime", "drainDeltaTransportJson", "completeLiveSegment",
             "liveSegmentWake", "resumeExecutionPlayer", "returnExecutionPlayer")
    result = {"engine_api": {}}
    for method in names:
        values = [event["ms"] for event in engine_events
                  if event["type"] == "api" and event["name"].endswith("." + method)]
        result["engine_api"][method] = distribution(values, include_sum=True)
    fires = [event for event in renderer_events if event["type"] == "raf_fire"]
    requests = [event for event in renderer_events if event["type"] == "raf_request"]
    result["raf_request_count"] = len(requests)
    result["raf_fire_count"] = len(fires)
    result["raf_cancel_count"] = sum(event["type"] == "raf_cancel" for event in renderer_events)
    result["raf_request_to_fire_ms"] = distribution([event["ms"] for event in fires])
    # Keep ordered trace intervals, never infer GPU time from renderer return.
    result["renderer_tick_post_count"] = sum(event["type"] == "port_post" and event.get("kind") == "tick"
                                               for event in renderer_events)
    result["engine_tick_receive_count"] = sum(event["type"] == "engine_receive" and event.get("kind") == "tick"
                                                 for event in engine_events)
    return result


def analyze_run(run, profiles, flights):
    completion = run["completion"]
    session, completion_session = completion["fromSession"], completion["toSession"]
    renderer_origin = completion["clockOriginMs"]
    stage_rows = [stage for stage in run["stageSamples"] if stage["session"] == session]
    stages = sorted({stage["sequence"]: stage for stage in stage_rows}.values(), key=lambda stage: stage["sequence"])
    require(len(stages) == len(stage_rows), "duplicate current-session stage sequence")
    require([stage["sequence"] for stage in stages] == list(range(1, len(stages) + 1)),
            "missing current-session stage sequence")
    require(all(stage["clockOriginMs"] == renderer_origin for stage in stages), "stage clock mismatch")
    require(all(current["time"] + 1e-8 >= previous["time"] for previous, current in zip(stages, stages[1:])),
            "authored stage time regressed")
    engine_profile = next(profile for profile in profiles if urlparse(profile["url"]).path.endswith("/python-worker.js"))
    renderer_profile = next(profile for profile in profiles if urlparse(profile["url"]).path.endswith("/execution-render-worker.js"))
    engine, renderer = engine_profile["events"], renderer_profile["events"]
    presents = [event for event in renderer if event["type"] == "port_post" and event.get("kind") == "execution_presented"]
    initial = next(event for event in presents if event["session"] == session and event["sequence"] == 0)
    final = next(event for event in presents if event["session"] == completion_session and event["sequence"] == 0)
    start_ms, end_ms = initial["renderer_at_ms"], final["renderer_at_ms"]
    require(start_ms < end_ms, "invalid presentation window")
    wakes = [event for event in engine if event["type"] == "api"
             and event["name"].endswith(".liveSegmentWake") and start_ms <= event["renderer_start_ms"] <= end_ms]
    require(bool(wakes), "missing first live clock observation")
    observed_anchor = wakes[0]["renderer_start_ms"]

    def exact(time):
        return [stage for stage in stages if close_time(stage["time"], time)]

    def first(time):
        rows = exact(time)
        require(bool(rows), f"missing exact boundary {time}")
        return rows[0]

    flight_rows = []
    for flight in flights:
        start, end = flight["start_seconds"], flight["end_seconds"]
        boundary, endpoint = exact(start), exact(end)
        require(bool(boundary) and bool(endpoint), "missing flight boundary")
        interior = [stage for stage in stages if start + 1e-8 < stage["time"] < end - 1e-8]
        # The final publication at a boundary is the following segment's initial
        # state. Record multiplicity instead of requiring a universal triple.
        initial_stage = boundary[-1]
        advancing = next(stage for stage in stages if stage["time"] > start + 1e-8)
        require(advancing["time"] <= end + 1e-8, "advancing stage skipped flight endpoint")
        gap = advancing["receivedAtMs"] - initial_stage["presentedAtMs"] - initial_stage["renderToAckMs"]
        require(gap >= -1e-6, "negative initial-to-advance interval")
        lower = initial_stage["presentedAtMs"] + initial_stage["renderToAckMs"]
        upper = advancing["receivedAtMs"]
        api_events = [event for event in engine if event["type"] == "api"
                      and event["name"].endswith(".driveLiveSegmentFromWallTime")
                      and lower <= event["renderer_start_ms"] <= upper]
        fires = [event for event in renderer if event["type"] == "raf_fire" and lower <= event["renderer_at_ms"] <= upper]
        ticks = [event for event in renderer if event["type"] == "port_post" and event.get("kind") == "tick"
                 and lower <= event["renderer_at_ms"] <= upper]
        raf_attribution = None
        if len(ticks) == 1 and len(api_events) == 1:
            preceding_fires = [event for event in renderer if event["type"] == "raf_fire"
                               and event["renderer_at_ms"] <= ticks[0]["renderer_at_ms"]]
            if preceding_fires and preceding_fires[-1]["renderer_at_ms"] >= lower:
                fire = preceding_fires[-1]
                # Clip a RAF request made before the gap to its start. All
                # three terms use the render-worker clock and partition the
                # measured gap exactly; no CPU/GPU or cross-clock subtraction.
                partition = {
                    "before_raf_request_ms": max(0, fire["renderer_start_ms"] - lower),
                    "waiting_for_raf_callback_ms": fire["renderer_at_ms"] - max(lower, fire["renderer_start_ms"]),
                    "after_raf_callback_to_first_advance_receive_ms": upper - fire["renderer_at_ms"],
                }
                require(min(partition.values()) >= -1e-6 and abs(math.fsum(partition.values()) - gap) < 1e-6,
                        "same-renderer-clock RAF partition does not reproduce gap")
                raf_attribution = {"raf_id": fire["id"], "partition_ms": partition,
                                   "unclipped_request_to_fire_ms": fire["ms"],
                                   "tick_posts_in_gap": len(ticks), "drive_calls_in_gap": len(api_events)}
        flight_rows.append({**flight, "interior_presentations": len(interior),
                            "start_boundary_publications": len(boundary), "endpoint_publications": len(endpoint),
                            "first_endpoint_wall_ms": endpoint[0]["presentedAtMs"] - boundary[0]["presentedAtMs"],
                            "post_initial_first_advance_gap_ms": gap,
                            "first_advance_is_endpoint": close_time(advancing["time"], end),
                            "post_initial_raf_attribution": raf_attribution,
                            "drives_inside_post_initial_gap_ms": distribution([event["ms"] for event in api_events], True),
                            "raf_fires_inside_post_initial_gap_ms": distribution([event["ms"] for event in fires])})

    def flight_summary(rows):
        matched = [row for row in rows if row["post_initial_raf_attribution"] is not None]
        partitions = {key: distribution([row["post_initial_raf_attribution"]["partition_ms"][key]
                                        for row in matched], True)
                      for key in (matched[0]["post_initial_raf_attribution"]["partition_ms"] if matched else ())}
        matched_gap_ms = math.fsum(row["post_initial_first_advance_gap_ms"] for row in matched)
        raf_wait_ms = partitions.get("waiting_for_raf_callback_ms", {}).get("sum_ms", 0)
        return {"count": len(rows), "with_interior_presentations": sum(row["interior_presentations"] > 0 for row in rows),
                "without_interior_presentations": sum(row["interior_presentations"] == 0 for row in rows),
                "interior_presentation_count": sum(row["interior_presentations"] for row in rows),
                "interior_count_distribution": dict(Counter(row["interior_presentations"] for row in rows)),
                "start_boundary_publication_count_distribution": dict(Counter(row["start_boundary_publications"] for row in rows)),
                "wall_ms": math.fsum(row["first_endpoint_wall_ms"] for row in rows),
                "authored_seconds": math.fsum(row["duration_seconds"] for row in rows),
                "raf_partition_matched_flights": len(matched),
                "raf_partition_ms": partitions,
                "raf_wait_fraction_of_matched_post_initial_gaps": raf_wait_ms / matched_gap_ms if matched_gap_ms else None,
                "post_initial_first_advance_gap_ms": distribution([row["post_initial_first_advance_gap_ms"] for row in rows], True)}

    phases = []
    named_phases = (("slow_pink", .8, 5.6), ("fast_pink", 5.6, 8.8), ("pose", 8.8, 9.4),
                    ("colored", 9.4, 14.4), ("arc", 14.4, 15), ("wait", 15, 16), ("dense", 16, 23))
    for label, start, end in named_phases:
        before, after = first(start), first(end)
        lower, upper = before["presentedAtMs"], after["presentedAtMs"]
        interval = [stage for stage in stages if before["sequence"] < stage["sequence"] <= after["sequence"]]
        phases.append({"name": label, "start_seconds": start, "end_seconds": end,
                       "authored_seconds": end - start, "wall_ms": upper - lower, "publications": len(interval),
                       "renderer_call_sum_ms": math.fsum(stage["renderMs"] for stage in interval),
                       "delta_apply_sum_ms": math.fsum(stage["applyMs"] for stage in interval),
                       "profile": window_profile(engine, renderer, lower, upper)})
    # Every drive API call is recorded by the injected probe, unlike small API
    # calls omitted by its >=1ms threshold. Check the cumulative-count difference.
    drive_name = "SemanticExecutionPlayer.driveLiveSegmentFromWallTime"
    drive_events = [event for event in engine if event["type"] == "api" and event["name"] == drive_name]
    drive_total = next(row for row in engine_profile["api_totals_since_previous_capture"] if row["name"] == drive_name)
    require(len(drive_events) == drive_total["count"], "drive event stream incomplete")
    require(abs(math.fsum(event["ms"] for event in drive_events) - drive_total["sum_ms"]) < 1e-6,
            "drive events do not reproduce cumulative timing difference")
    return {
        "index": run["index"], "kind": run["kind"], "session": session, "completion_session": completion_session,
        "raw_stage_sample_count": len(run["stageSamples"]), "current_session_stage_sample_count": len(stages),
        "excluded_other_session_stage_count": len(run["stageSamples"]) - len(stages),
        "timing": {"click_to_observed_endpoint_ms": run["elapsedMs"],
                   "first_actual_presentation_renderer_ms": start_ms,
                   "completion_handoff_presentation_renderer_ms": end_ms,
                   "first_actual_presentation_epoch_ms": renderer_origin + start_ms,
                   "completion_handoff_presentation_epoch_ms": renderer_origin + end_ms,
                   "first_actual_presentation_to_handoff_ms": end_ms - start_ms,
                   "first_engine_clock_observation_renderer_ms": observed_anchor,
                   "first_engine_clock_observation_to_handoff_ms": end_ms - observed_anchor,
                   "initial_presentation_to_engine_clock_observation_ms": observed_anchor - start_ms,
                   "click_elapsed_minus_presentation_span_ms": run["elapsedMs"] - (end_ms - start_ms)},
        "fast_flights": flight_summary(flight_rows[4:]),
        "slow_flights": flight_summary(flight_rows[:4]),
        "fast_pink_flights": flight_summary(flight_rows[4:36]),
        "colored_flights": flight_summary(flight_rows[36:]),
        "presentation_window_profile": window_profile(engine, renderer, start_ms, end_ms),
        "all_current_capture_drive_calls_ms": distribution([event["ms"] for event in drive_events], True),
        "worker_profile_deltas": [{key: value for key, value in profile.items() if key != "events"} for profile in profiles],
        "phases": phases, "per_flight": flight_rows,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", nargs="?", type=Path, default=Path("/tmp/noon-profile/evidence-chrome151/report.json"))
    parser.add_argument("--scene", type=Path, default=Path("/tmp/noon-2141-benchmark/scene.py"))
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    report = json.loads(args.report.read_text())
    require(report["sourceSha256"] == PRIOR.EXPECTED_SCENE_SHA256, "report source hash mismatch")
    flights = PRIOR.recover_geometry_and_boundaries(args.scene.read_bytes())
    previous = {}
    results = []
    for run in report["runs"]:
        profiles = [profile_delta(wrapper, previous, run["completion"]["clockOriginMs"])
                    for wrapper in run["workerProfiles"]]
        results.append(analyze_run(run, profiles, flights))
    analysis = PRIOR.rounded({
        "schema": 1, "source_sha256": report["sourceSha256"], "runtime_revision": report["expectedRevision"],
        "runtime": report["runtime"], "browser_version": report["browserVersion"], "report_status": report["status"],
        "input_report": str(args.report.resolve()),
        "notes": [
            "Current-run stage rows are selected by completion.fromSession; warm worker events are cumulative-prefix differenced.",
            "First actual scene presentation and completion are renderer execution_presented posts for the relevant session sequence0; the later sequence1 snapshot may occur after endpoint detection.",
            "The first engine clock observation is the wrapper-call start of liveSegmentWake after initial presentation, not the unrecorded exact Rust anchor value.",
            "Click elapsed minus presentation span includes startup and observer/host timing; it is not a pure interpreter CPU measurement.",
            "Phase and API/RAF windows are selected by event completion time on a shared renderer-relative clock; calls begun just outside a window can end inside it.",
            "RAF request-to-fire durations, engine API time, render/apply time, and whole-scene elapsed overlap. They are not additive CPU/GPU components.",
            "For a post-initial gap with exactly one tick and one drive, the immediately preceding RAF fire is matched; its wait is clipped to the gap and partitioned with before-request/after-fire intervals on the same renderer clock.",
            "The probe records every drive/drain/handoff/wake call but only other API calls taking >=1ms; cumulative API count/sum differences include all calls. Cumulative maxima are never differenced.",
            "No universal zero-interior or triple-publication assumption is imposed. Publication multiplicities and observed interior counts are reported.",
            "Renderer return/presentation posts are not GPU completion or physical scanout. Cross-version/host causal attribution requires a controlled comparison.",
        ], "runs": results,
    })
    if args.output:
        args.output.write_text(json.dumps(analysis, separators=(",", ":")) + "\n")
        summary = {"output": str(args.output.resolve()), "runtime_revision": report["expectedRevision"],
                   "browser_version": report["browserVersion"], "runs": []}
        for run in analysis["runs"]:
            summary["runs"].append({"index": run["index"], "kind": run["kind"],
                "click_elapsed_seconds": run["timing"]["click_to_observed_endpoint_ms"] / 1000,
                "first_actual_presentation_to_handoff_seconds": run["timing"]["first_actual_presentation_to_handoff_ms"] / 1000,
                "fast_flight_wall_seconds": run["fast_flights"]["wall_ms"] / 1000,
                "fast_flights_with_interior": run["fast_flights"]["with_interior_presentations"],
                "fast_interior_presentations": run["fast_flights"]["interior_presentation_count"],
                "fast_post_initial_gap_ms": run["fast_flights"]["post_initial_first_advance_gap_ms"],
                "fast_raf_matched_flights": run["fast_flights"]["raf_partition_matched_flights"],
                "fast_raf_wait_fraction_of_matched_gaps": run["fast_flights"]["raf_wait_fraction_of_matched_post_initial_gaps"],
                "drive_ms": run["all_current_capture_drive_calls_ms"],
                "raf_ms": run["presentation_window_profile"]["raf_request_to_fire_ms"]})
        print(json.dumps(PRIOR.rounded(summary), separators=(",", ":")))
    else:
        print(json.dumps(analysis, separators=(",", ":")))


if __name__ == "__main__":
    main()
