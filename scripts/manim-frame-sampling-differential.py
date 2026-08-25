#!/usr/bin/env python3
"""Compare Noon's canonical frame timestamps with pinned ManimCE v0.21.0."""

from __future__ import annotations

import math
import sys
from pathlib import Path
from types import SimpleNamespace

REPO_ROOT = Path(__file__).resolve().parents[1]
WEB_PYTHON = REPO_ROOT / "web" / "python"
if str(WEB_PYTHON) not in sys.path:
    sys.path.insert(0, str(WEB_PYTHON))

from _manim_frame_sampling import frame_times, logical_endpoint  # noqa: E402

try:
    import manim  # noqa: E402
except ImportError as exc:  # pragma: no cover - CI installs the pinned reference
    raise SystemExit("ManimCE is required for the frame-sampling differential") from exc

PINNED_MANIM_VERSION = "0.21.0"
CASES = (
    ("one-second-30fps", 1.0, 30.0),
    ("fractional-30fps", 0.85, 30.0),
    ("sub-frame-30fps", 0.01, 30.0),
    ("zero-duration-30fps", 0.0, 30.0),
    ("one-second-60fps", 1.0, 60.0),
    ("long-60fps", 10.0, 60.0),
)


def manim_frame_times(run_time: float, frame_rate: float) -> tuple[float, ...]:
    old_frame_rate = manim.config.frame_rate
    old_progress_bar = manim.config.progress_bar
    try:
        manim.config.frame_rate = frame_rate
        manim.config.progress_bar = "none"
        scene_stub = SimpleNamespace(renderer=SimpleNamespace(skip_animations=False))
        progression = manim.Scene.get_time_progression(
            scene_stub,
            run_time,
            "Noon timing differential",
        )
        try:
            return tuple(float(value) for value in progression)
        finally:
            progression.close()
    finally:
        manim.config.frame_rate = old_frame_rate
        manim.config.progress_bar = old_progress_bar


def compare(case: str, run_time: float, frame_rate: float) -> list[str]:
    noon_samples = frame_times(run_time, frame_rate)
    manim_samples = manim_frame_times(run_time, frame_rate)
    errors: list[str] = []

    if len(noon_samples) != len(manim_samples):
        errors.append(
            f"sample-count Noon={len(noon_samples)} Manim={len(manim_samples)}"
        )
    for index, (noon_time, manim_time) in enumerate(zip(noon_samples, manim_samples)):
        if not math.isclose(noon_time, manim_time, rel_tol=0.0, abs_tol=1e-12):
            errors.append(
                f"sample[{index}] Noon={noon_time!r} Manim={manim_time!r}"
            )
            break

    endpoint = logical_endpoint(run_time)
    if noon_samples and not noon_samples[-1] < endpoint:
        errors.append(
            f"last rendered sample {noon_samples[-1]!r} must be before endpoint {endpoint!r}"
        )
    if any(math.isclose(sample, endpoint, rel_tol=0.0, abs_tol=1e-15) for sample in noon_samples):
        errors.append(f"logical endpoint {endpoint!r} was included as a rendered frame")

    marker = "PASS" if not errors else "FAIL"
    print(
        f"[{marker}] {case}: {len(noon_samples)} samples, "
        f"endpoint={endpoint:g}, fps={frame_rate:g}"
    )
    for error in errors:
        print(f"  {error}")
    return errors


def main() -> int:
    if manim.__version__ != PINNED_MANIM_VERSION:
        raise SystemExit(
            f"expected ManimCE {PINNED_MANIM_VERSION}, found {manim.__version__}"
        )

    failures = 0
    for case, run_time, frame_rate in CASES:
        failures += bool(compare(case, run_time, frame_rate))

    if failures:
        print(f"\n{failures}/{len(CASES)} frame-sampling fixtures differ from ManimCE")
        return 1
    print(f"\nAll {len(CASES)} frame-sampling fixtures match ManimCE {manim.__version__}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
