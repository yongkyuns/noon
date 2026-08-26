#!/usr/bin/env python3
"""Focused #76 fixtures using the shared Manim differential normalizers/comparator."""

from __future__ import annotations

import importlib.util
import math
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE_PATH = ROOT / "scripts" / "manim-differential.py"
spec = importlib.util.spec_from_file_location("noon_manim_differential", BASE_PATH)
if spec is None or spec.loader is None:
    raise RuntimeError("unable to load shared Manim differential harness")
base = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = base
spec.loader.exec_module(base)

import _manim_phase_b  # noqa: E402,F401 - match browser compatibility stack
import _manim_geometry  # noqa: E402,F401 - installs Dot/Ellipse public surface

noon = base.noon
manim = base.manim


def dot_probe(module):
    default = module.Dot()
    shifted = module.Dot(point=2 * module.LEFT + 0.75 * module.UP, radius=0.18)
    return {
        "default": base._object_observation(default),
        "shifted": base._object_observation(shifted),
    }


def ellipse_probe(module):
    default = module.Ellipse()
    transformed = (
        module.Ellipse(width=4.0, height=1.5)
        .rotate(math.pi / 6)
        .shift(1.25 * module.RIGHT + 0.5 * module.DOWN)
    )
    return {
        "default": base._object_observation(default),
        "transformed": base._object_observation(transformed),
    }


fixtures = [
    base.Fixture("dot_geometry", lambda: dot_probe(noon), lambda: dot_probe(manim)),
    base.Fixture(
        "ellipse_geometry", lambda: ellipse_probe(noon), lambda: ellipse_probe(manim)
    ),
]

failures = 0
for fixture in fixtures:
    noon_value = fixture.noon_probe()
    manim_value = fixture.manim_probe()
    differences = base._compare(noon_value, manim_value, fixture.tolerance)
    if differences:
        failures += 1
        print(f"[FAIL] {fixture.name}")
        for difference in differences:
            print(f"  {difference}")
    else:
        print(f"[PASS] {fixture.name}")

print(
    f"\n{len(fixtures) - failures}/{len(fixtures)} Dot/Ellipse fixtures match "
    f"ManimCE {manim.__version__}"
)
raise SystemExit(1 if failures else 0)
