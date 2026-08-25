"""Canonical ManimCE frame-time sampling for deterministic/offline rendering.

The real-time Noon player remains continuous-time. This helper models the frame
sampling convention used by ManimCE's renderer: sample ``0, 1/fps, ...`` while
strictly below the logical animation endpoint, then apply the endpoint state
separately after rendering the sampled frames.
"""

from __future__ import annotations

import math


def frame_times(run_time: float, frame_rate: float) -> tuple[float, ...]:
    """Return ManimCE-compatible rendered frame timestamps.

    This is equivalent to ManimCE v0.21's
    ``np.arange(0, run_time, 1 / frame_rate)`` for finite, non-negative
    ``run_time`` and positive finite ``frame_rate``. The logical endpoint is
    deliberately excluded from the returned samples.
    """

    duration = float(run_time)
    fps = float(frame_rate)
    if not math.isfinite(duration) or duration < 0.0:
        raise ValueError("run_time must be finite and non-negative")
    if not math.isfinite(fps) or fps <= 0.0:
        raise ValueError("frame_rate must be finite and positive")

    samples: list[float] = []
    frame = 0
    while True:
        sample = frame / fps
        if sample >= duration:
            break
        samples.append(sample)
        frame += 1
    return tuple(samples)


def logical_endpoint(run_time: float) -> float:
    """Validate and return the post-render logical animation endpoint."""

    duration = float(run_time)
    if not math.isfinite(duration) or duration < 0.0:
        raise ValueError("run_time must be finite and non-negative")
    return duration
