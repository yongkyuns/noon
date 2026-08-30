#!/usr/bin/env python3
"""Compare shared Rust Elbow geometry against pinned ManimCE semantics."""

from __future__ import annotations

import json
import math
import subprocess
from pathlib import Path

import manim

ROOT = Path(__file__).resolve().parents[1]
PINNED_MANIM_VERSION = "0.21.0"
TOLERANCE = 2e-5


def observe(obj):
    center = obj.get_center()
    start = obj.get_start()
    end = obj.get_end()
    return {
        "center": [float(center[0]), float(center[1])],
        "start": [float(start[0]), float(start[1])],
        "end": [float(end[0]), float(end[1])],
        "width": float(obj.width),
        "height": float(obj.height),
    }


def manim_observations():
    return {
        "default": observe(manim.Elbow()),
        "rotated_wide": observe(manim.Elbow(width=2.0, angle=5.0 * math.pi / 4.0)),
        "zero_width": observe(manim.Elbow(width=0.0, angle=math.pi / 3.0)),
        "negative_width": observe(manim.Elbow(width=-0.5, angle=-math.pi / 6.0)),
    }


def noon_observations():
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "noon-web",
            "--example",
            "manim_elbow_oracle",
        ],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def compare(path, actual, expected, failures):
    if isinstance(expected, dict):
        if set(actual) != set(expected):
            failures.append(
                f"{path}: keys differ: Noon={sorted(actual)}, Manim={sorted(expected)}"
            )
            return
        for key in expected:
            compare(f"{path}.{key}", actual[key], expected[key], failures)
        return

    if isinstance(expected, list):
        if len(actual) != len(expected):
            failures.append(
                f"{path}: length differs: Noon={len(actual)}, Manim={len(expected)}"
            )
            return
        for index, (actual_item, expected_item) in enumerate(zip(actual, expected)):
            compare(f"{path}[{index}]", actual_item, expected_item, failures)
        return

    if not math.isclose(float(actual), float(expected), rel_tol=0.0, abs_tol=TOLERANCE):
        failures.append(f"{path}: Noon={actual!r}, Manim={expected!r}")


def main():
    if manim.__version__ != PINNED_MANIM_VERSION:
        raise SystemExit(
            f"expected ManimCE {PINNED_MANIM_VERSION}, got {manim.__version__}"
        )

    noon = noon_observations()
    reference = manim_observations()
    failures = []
    compare("elbow", noon, reference, failures)
    if failures:
        print("[FAIL] shared Rust Elbow geometry diverges from ManimCE")
        for failure in failures:
            print(f"  {failure}")
        raise SystemExit(1)

    print(
        f"[PASS] {len(reference)} shared Rust Elbow fixtures match "
        f"ManimCE {manim.__version__} within {TOLERANCE:g}"
    )


if __name__ == "__main__":
    main()
