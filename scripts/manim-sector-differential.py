#!/usr/bin/env python3
"""Compare shared Rust sector geometry against pinned ManimCE semantics.

This probe deliberately bypasses the still-in-progress Python adapter.  It compares
renderer-independent bounds from the existing shared Rust/WASM sector bridge with
ManimCE 0.21.0, so frontend work can reuse an already-qualified geometry oracle.
"""

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
    return {
        "center": [float(center[0]), float(center[1])],
        "width": float(obj.width),
        "height": float(obj.height),
    }


def manim_observations():
    annular_default = manim.AnnularSector()
    annular_signed_offset = manim.AnnularSector(
        inner_radius=0.5,
        outer_radius=2.25,
        angle=-math.pi / 3,
        start_angle=math.pi / 4,
        num_components=9,
        arc_center=[1.25, -0.75, 0.0],
    )
    sector_offset = manim.Sector(
        radius=2.0,
        angle=math.pi / 2,
        start_angle=-math.pi / 4,
        num_components=9,
        arc_center=[-1.5, 0.75, 0.0],
    )
    annulus_offset = manim.Annulus(inner_radius=0.5, outer_radius=1.75).shift(
        [0.8, -1.1, 0.0]
    )
    return {
        "annular_default": observe(annular_default),
        "annular_signed_offset": observe(annular_signed_offset),
        "sector_offset": observe(sector_offset),
        "annulus_offset": observe(annulus_offset),
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
            "manim_sector_oracle",
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
    compare("sector", noon, reference, failures)
    if failures:
        print("[FAIL] shared Rust sector geometry diverges from ManimCE")
        for failure in failures:
            print(f"  {failure}")
        raise SystemExit(1)

    print(
        f"[PASS] {len(reference)} shared Rust sector fixtures match "
        f"ManimCE {manim.__version__} within {TOLERANCE:g}"
    )


if __name__ == "__main__":
    main()
