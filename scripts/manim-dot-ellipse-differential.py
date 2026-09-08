#!/usr/bin/env python3
"""Compare shared Rust Dot/Ellipse semantics using the Manim differential helpers."""

from __future__ import annotations

import importlib.util
import json
import math
import os
import subprocess
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

manim = base.manim


def noon_observations():
    subprocess.run(
        ["cargo", "build", "--quiet", "--workspace", "--all-features", "--example", "manim_dot_ellipse_oracle"],
        cwd=ROOT, check=True,
    )
    binary = ROOT / os.environ.get("CARGO_TARGET_DIR", "target") / "debug/examples/manim_dot_ellipse_oracle"
    result = subprocess.run([str(binary)], cwd=ROOT, check=True, capture_output=True, text=True)
    return json.loads(result.stdout)


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


if manim.__version__ != base.PINNED_MANIM_VERSION:
    raise SystemExit(
        f"expected ManimCE {base.PINNED_MANIM_VERSION}, got {manim.__version__}"
    )

noon = noon_observations()
fixtures = [
    base.Fixture("dot_geometry", lambda: noon["dot_geometry"], lambda: dot_probe(manim)),
    base.Fixture(
        "ellipse_geometry", lambda: noon["ellipse_geometry"], lambda: ellipse_probe(manim)
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
