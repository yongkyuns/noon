#!/usr/bin/env python3
"""Check compact Fourier motion/trace sources and their mathematical/cost baseline.

This executes only the preserved source's pure NumPy preparation functions.
It does not emulate Noon, execute construct(), or qualify a runtime/renderer.
Use --oracle to include immutable motion keys for real browser comparisons.
"""

import argparse
import ast
import hashlib
import json
from pathlib import Path
import sys

import numpy as np


ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests" / "fixtures" / "fourier_audio"
ORIGINAL_SHA256 = "7a09cda6039efdc9901a3241b0c514f634996187a0ea9b1f316e34dbbd1feed9"
COMPACT_SHA256 = "8289df0a003130531e4ec14076dd58b4c6de83b35de9e2b9ec5581d61872d254"
CAPTURE_SHA256 = "5ee5435db5f8b551228c272a421f2161913d91edcb720e24bd469315a43c40df"
EXPECTED_INTERVALS = (64, 128, 146)
EXPECTED_BOUNDS = (0.007911181851180863, 0.008604716558672504, 0.00979857890999025)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def function(tree, name):
    return next(node for node in tree.body
                if isinstance(node, ast.FunctionDef) and node.name == name)


def outside_motion_helper(source, tree):
    node = function(tree, "native_poses")
    lines = source.splitlines(keepends=True)
    return "".join(lines[:node.lineno - 1] + lines[node.end_lineno:])


def outside_trace_block(source, marker):
    start = source.index(marker)
    end = source.index("    return VGroup(*objects), trace, animations, len(phase) - 1, error_bound", start)
    return source[:start] + source[end:]


def source_contract():
    original_bytes = (FIXTURES / "fourier_epicycles.py").read_bytes()
    compact_bytes = (FIXTURES / "fourier_epicycles_sampled.py").read_bytes()
    phase_reveal_bytes = (FIXTURES / "fourier_epicycles_phase_reveal.py").read_bytes()
    capture_bytes = (FIXTURES / "fourier_model_capture.json").read_bytes()
    assert digest(original_bytes) == ORIGINAL_SHA256, "original Fourier bytes changed"
    assert digest(compact_bytes) == COMPACT_SHA256, "Step 4 compact Fourier baseline bytes changed"
    assert digest(capture_bytes) == CAPTURE_SHA256, "recorded model capture changed"
    original, compact = original_bytes.decode(), compact_bytes.decode()
    old_tree, new_tree = ast.parse(original), ast.parse(compact)
    phase_reveal = phase_reveal_bytes.decode()
    phase_tree = ast.parse(phase_reveal)
    assert outside_motion_helper(original, old_tree) == outside_motion_helper(compact, new_tree), (
        "compact source may change only native_poses; preserve FFT, keys, trace, and choreography"
    )
    assert outside_trace_block(compact, "    # Create uses uniform curve-count progress.") == outside_trace_block(
        phase_reveal, "    # Reveal the retained adaptive chords in the same elapsed phase as the pen."
    ), "phase reveal may change only the trace block; preserve sampled motion, FFT, keys, and choreography"
    phase_stage = function(phase_tree, "prepare_stage")
    phase_calls = [node for node in ast.walk(phase_stage) if isinstance(node, ast.Call)]
    reveal_calls = [node for node in phase_calls
                    if isinstance(node.func, ast.Name) and node.func.id == "PhaseReveal"]
    assert len(reveal_calls) == 1
    assert not any(isinstance(node.func, ast.Name) and node.func.id == "Create" for node in phase_calls)
    trace_points_calls = [node for node in phase_calls
                          if isinstance(node.func, ast.Attribute) and node.func.attr == "set_points_as_corners"
                          and isinstance(node.func.value, ast.Name) and node.func.value.id == "trace"]
    assert len(trace_points_calls) == 1 and len(trace_points_calls[0].args) == 1
    old_helper, new_helper = function(old_tree, "native_poses"), function(new_tree, "native_poses")
    old_calls = [node for node in ast.walk(old_helper) if isinstance(node, ast.Call)]
    new_calls = [node for node in ast.walk(new_helper) if isinstance(node, ast.Call)]
    assert sum(isinstance(node.func, ast.Attribute) and node.func.attr == "copy"
               for node in old_calls) == 1
    assert sum(isinstance(node.func, ast.Name) and node.func.id == "Transform"
               for node in old_calls) == 1
    assert sum(isinstance(node.func, ast.Name) and node.func.id == "SampledMotion"
               for node in new_calls) == 1
    assert not any(isinstance(node, (ast.For, ast.While)) for node in ast.walk(new_helper)), (
        "compact authoring must not rebuild an interval animation tree"
    )
    assert not any((isinstance(node.func, ast.Attribute) and node.func.attr == "copy") or
                   (isinstance(node.func, ast.Name) and node.func.id in {"Transform", "Succession"})
                   for node in new_calls)
    capture = json.loads(capture_bytes)
    for name, expected in capture["provenance"]["unchanged_helper_source_sha256"].items():
        assert digest(ast.get_source_segment(original, function(old_tree, name)).encode()) == expected
    constants = {}
    for node in old_tree.body:
        if isinstance(node, ast.Assign) and len(node.targets) == 1 and isinstance(node.targets[0], ast.Name):
            constants[node.targets[0].id] = ast.literal_eval(node.value)
    names = {"resample_outline", "fourier_model", "prepare_positions", "xy"}
    helpers = [node for node in old_tree.body if isinstance(node, ast.FunctionDef) and node.name in names]
    namespace = {"np": np, **constants}
    exec(compile(ast.Module(body=helpers, type_ignores=[]), "preserved_fourier_helpers", "exec"), namespace)
    returns = [node for node in new_helper.body if isinstance(node, ast.Return)]
    assert len(returns) == 1 and isinstance(returns[0].value, ast.Call)
    namespace["sampled_arguments"] = returns[0].value
    namespace["phase_reveal_arguments"] = reveal_calls[0]
    namespace["phase_trace_points"] = trace_points_calls[0].args[0]
    return namespace, capture, {
        "original_sha256": digest(original_bytes),
        "compact_sha256": digest(compact_bytes),
        "phase_reveal_sha256": digest(phase_reveal_bytes),
        "model_capture_sha256": digest(capture_bytes),
        "changed_function": "native_poses",
        "duration_seconds": 34.65,
        "trace_grid": "unchanged uniform curve-count compensation",
        "phase_reveal_changed_block": "prepare_stage trace construction only",
        "phase_reveal_parameterization": "elapsed phase mapped to existing retained drawable-curve count",
    }


def interpolate(phases, keys, values):
    return np.stack([
        np.interp(phases, keys, column.real) + 1j * np.interp(phases, keys, column.imag)
        for column in values.T
    ], axis=1)


def authored_sampled_arguments(api, keys, positions, turns):
    """Evaluate the fixture's actual pure argument expressions, never Noon."""
    call = api["sampled_arguments"]
    namespace = {**api, "phase": keys, "positions": positions, "turns": turns, "linear": "linear"}

    def evaluate(node):
        return eval(compile(ast.Expression(node), "sampled_motion_arguments", "eval"), namespace)

    assert len(call.args) == 3
    keywords = {keyword.arg: evaluate(keyword.value) for keyword in call.keywords}
    return evaluate(call.args[1]), evaluate(call.args[2]), keywords


def authored_phase_reveal_arguments(api, keys, endpoint):
    """Evaluate the new fixture's actual trace/key expressions, never Noon."""
    call = api["phase_reveal_arguments"]
    namespace = {**api, "phase": keys, "endpoint": endpoint, "linear": "linear"}

    def evaluate(node):
        return eval(compile(ast.Expression(node), "phase_reveal_arguments", "eval"), namespace)

    assert len(call.args) == 2 and isinstance(call.args[0], ast.Name) and call.args[0].id == "trace"
    keywords = {keyword.arg: evaluate(keyword.value) for keyword in call.keywords}
    return evaluate(call.args[1]), evaluate(api["phase_trace_points"]), keywords


def check(include_oracle=False):
    api, capture, identity = source_contract()
    model = capture["model"]
    frequencies = np.asarray(model["frequencies"], dtype=int)
    coefficients = np.array([complex(*value) for value in model["coefficients"]])
    offset = complex(*model["offset"])
    assert len(frequencies) == len(coefficients) == 60
    assert float(np.abs(coefficients).min()) > 1e-7, "coefficient omission changes the structural baseline"
    report = {
        "status": "passed",
        "scope": "source identity, source construction counts, and mathematical oracle; no runtime or performance claim",
        "python_version": sys.version.split()[0],
        "numpy_version": np.__version__,
        **identity,
        "capture_provenance": capture["provenance"],
        "mathematical_probes": {
            "off_grid_phase_count": 8192,
            "off_grid_phase_formula": "(arange(8192) + 0.371) / 8192",
            "phase_reveal_additions": "all adaptive keys and +/-1e-7 neighborhoods, then reverse order",
            "unmapped_control": "ordinary curve-count reveal over adaptive chords without a phase map",
        },
        "stages": [],
    }
    # Off-grid phases exercise entire chords, not only endpoints or midpoints.
    phases = (np.arange(8192) + 0.371) / 8192
    for stage, terms in enumerate(api["TERM_COUNTS"]):
        count = terms - 1
        keys, chain, bound = api["prepare_positions"](frequencies, coefficients, offset, count)
        intervals = len(keys) - 1
        assert intervals == EXPECTED_INTERVALS[stage]
        assert abs(bound - EXPECTED_BOUNDS[stage]) <= 2e-14
        assert keys[0] == 0 and keys[-1] == 1 and np.all(np.diff(keys) > 0)
        np.testing.assert_array_equal(chain[0], chain[-1])
        true_vectors = np.exp(2j * np.pi * np.outer(phases, frequencies[:count])) * coefficients[:count]
        true_chain = offset + np.concatenate((np.zeros((len(phases), 1)), np.cumsum(true_vectors, axis=1)), axis=1)
        sampled_chain = interpolate(phases, keys, chain)
        interpolation_error = float(np.max(np.abs(sampled_chain - true_chain)))
        assert interpolation_error <= bound + 1e-12 <= api["MAX_POINT_ERROR"] + 1e-12

        # The new implicit phase-zero endpoint is the real activation capture.
        # In this unchanged fixture every target begins at chain[0]. Each keyed
        # stream therefore specifies exactly the same chords as the old targets.
        old_positions = np.concatenate((chain[:, 1:count],
                                        (chain[:, :-1] + chain[:, 1:]) / 2,
                                        chain[:, -1:], chain[:, -1:]), axis=1)
        turns_by_stream = np.concatenate((np.zeros(count - 1, dtype=int), frequencies[:count], [0, 0]))
        for positions, turns in zip(old_positions.T, turns_by_stream):
            authored_keys, authored_positions, options = authored_sampled_arguments(api, keys, positions, int(turns))
            np.testing.assert_array_equal(authored_keys, keys[1:])
            np.testing.assert_array_equal(authored_positions, np.stack((positions[1:].real, positions[1:].imag), axis=1))
            assert options["run_time"] == api["SECONDS_PER_DRAWING"] and options["rate_func"] == "linear"
            if turns:
                np.testing.assert_array_equal(options["angles"], 2 * np.pi * turns * keys[1:])
            else:
                assert options["angles"] is None
        angle_keys = 2 * np.pi * keys[:, None] * frequencies[:count]
        sampled_angles = np.stack([np.interp(phases, keys, column) for column in angle_keys.T], axis=1)
        analytic_angles = 2 * np.pi * phases[:, None] * frequencies[:count]
        angle_error = float(np.max(np.abs(sampled_angles - analytic_angles)))
        assert angle_error <= 1e-12, "signed angles must not wrap at complete turns"

        trace_intervals = int(round(1 / float(np.min(np.diff(keys)))))
        trace_keys = np.linspace(0, 1, trace_intervals + 1)
        trace_points = interpolate(trace_keys, keys, chain[:, -1:])
        trace_at_phase = interpolate(phases, trace_keys, trace_points)[:, 0]
        trace_pen_gap = float(np.max(np.abs(trace_at_phase - sampled_chain[:, -1])))
        assert trace_pen_gap <= 1e-12, "retain the existing phase/curve-count compensation"

        # The new fixture retains the original adaptive chords. Its phase map
        # changes only how the existing curve-count reveal selects that path.
        # Include exact keys and both neighborhoods in reverse order as well as
        # the unchanged 8192 off-grid samples; this is a source/math oracle only.
        authored_trace_keys, authored_trace_points, trace_options = authored_phase_reveal_arguments(
            api, keys, chain[:, -1])
        np.testing.assert_array_equal(authored_trace_keys, keys)
        np.testing.assert_array_equal(authored_trace_points,
                                      np.stack((chain[:, -1].real, chain[:, -1].imag), axis=1))
        assert trace_options == {"run_time": api["SECONDS_PER_DRAWING"], "rate_func": "linear"}
        trace_probe_phases = np.concatenate((phases, keys, np.clip(keys - 1e-7, 0, 1),
                                            np.clip(keys + 1e-7, 0, 1)))[::-1]
        curve_keys = np.arange(len(keys)) / intervals
        mapped_reveal = np.interp(trace_probe_phases, keys, curve_keys)
        phase_trace = interpolate(mapped_reveal, curve_keys, chain[:, -1:])[:, 0]
        phase_pen = interpolate(trace_probe_phases, keys, chain[:, -1:])[:, 0]
        expanded_trace = interpolate(trace_probe_phases, trace_keys, trace_points)[:, 0]
        phase_trace_pen_gap = float(np.max(np.abs(phase_trace - phase_pen)))
        phase_trace_baseline_gap = float(np.max(np.abs(phase_trace - expanded_trace)))
        assert phase_trace_pen_gap <= 1e-12 and phase_trace_baseline_gap <= 1e-12
        np.testing.assert_array_equal(np.interp(keys, keys, curve_keys), curve_keys)
        naive_trace = interpolate(phases, curve_keys, chain[:, -1:])[:, 0]
        naive_trace_pen_gap = float(np.max(np.abs(naive_trace - sampled_chain[:, -1])))
        if terms == 61:
            assert naive_trace_pen_gap > 0.5, "the nonuniform stage must detect omitted phase mapping"
        streams = 2 * count + 1
        copies = streams * intervals
        row = {
            "terms": terms,
            "active_coefficients": count,
            "motion_intervals": intervals,
            "phase_keys": len(keys),
            "motion_streams": streams,
            "rig_objects": 2 * count + 2,
            "original_motion_target_copies": copies,
            "original_motion_transform_leaves": copies,
            "original_motion_succession_nodes": streams,
            "compact_motion_target_copies": 0,
            "compact_motion_sampled_leaves": streams,
            "compact_explicit_position_samples": copies,
            "compact_explicit_angle_samples": count * intervals,
            "trace_intervals": trace_intervals,
            "trace_vertices": trace_intervals + 1,
            "phase_reveal_trace_intervals": intervals,
            "phase_reveal_trace_vertices": len(keys),
            "phase_reveal_map_entries": len(keys),
            "removed_collinear_trace_vertices": trace_intervals - intervals,
            "phase_reveal_probe_count": len(trace_probe_phases),
            "certified_position_error_scene_units": bound,
            "off_grid_max_joint_position_error_scene_units": interpolation_error,
            "off_grid_max_angle_interpolation_error_radians": angle_error,
            "off_grid_max_trace_pen_gap_scene_units": trace_pen_gap,
            "phase_reveal_max_trace_pen_gap_scene_units": phase_trace_pen_gap,
            "phase_reveal_max_trace_baseline_gap_scene_units": phase_trace_baseline_gap,
            "unmapped_adaptive_create_max_trace_pen_gap_scene_units": naive_trace_pen_gap,
        }
        if include_oracle:
            row["oracle"] = {
                "keys": keys.tolist(),
                "chain": np.stack((chain.real, chain.imag), axis=2).tolist(),
                "frequencies": frequencies[:count].tolist(),
                "coefficients": model["coefficients"][:count],
                "draw_start_seconds": (1.4, 12.0, 22.6)[stage],
                "draw_duration_seconds": api["SECONDS_PER_DRAWING"],
            }
        report["stages"].append(row)
    summed = ("rig_objects", "motion_streams", "original_motion_target_copies",
              "original_motion_transform_leaves", "original_motion_succession_nodes",
              "compact_motion_target_copies", "compact_motion_sampled_leaves",
              "compact_explicit_position_samples", "compact_explicit_angle_samples",
              "trace_intervals", "trace_vertices", "phase_reveal_trace_intervals",
              "phase_reveal_trace_vertices", "phase_reveal_map_entries", "removed_collinear_trace_vertices")
    report["totals"] = {key: sum(stage[key] for stage in report["stages"]) for key in summed}
    assert report["totals"]["original_motion_target_copies"] == 23_746
    assert report["totals"]["compact_motion_sampled_leaves"] == 175
    assert report["totals"]["trace_vertices"] == 451
    assert report["totals"]["phase_reveal_trace_vertices"] == 341
    assert report["totals"]["removed_collinear_trace_vertices"] == 110
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--oracle", action="store_true", help="include motion keys for real-runtime observations")
    args = parser.parse_args()
    print(json.dumps(check(args.oracle), indent=2, allow_nan=False))
