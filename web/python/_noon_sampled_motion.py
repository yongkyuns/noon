"""Compact sampled motion requests for the shared Rust timeline.

Samples are destination keys, with phase zero captured from the effective pose
when the animation activates. Rust owns validation, timing and interpolation.
"""
from __future__ import annotations

from array import array

import noon as _base
from _noon_errors import engine_call


def _point(value):
    coordinates = tuple(float(component) for component in value)
    if len(coordinates) == 2:
        return (*coordinates, 0.0)
    if len(coordinates) != 3:
        raise ValueError("SampledMotion positions require two or three components")
    return coordinates


class SampledMotion:
    """Move one existing Mobject through nonuniform destination pose samples.

    ``key_times`` contains strictly increasing phases in ``(0, 1]`` ending at
    exactly 1. Positions are absolute world coordinates of the bounds center
    captured at activation. This center stays a fixed rigid anchor during
    rotation, including for asymmetric geometry. Optional angles are signed,
    unwrapped offsets from
    the effective starting rotation; omitted angles preserve that rotation.
    Scale and geometry remain unchanged. Add the target to the Scene (or finish
    its FadeIn) before playing this request.

    Ordinary duration/rate options, reversible timing and compositions apply.
    Per-leaf lag and path arcs are unsupported. All semantic checks occur in
    Rust; Python only normalizes the supplied iterable shapes and numbers.
    """

    def __init__(self, mobject, key_times, positions, angles=None, **kwargs):
        if not isinstance(mobject, _base.Mobject):
            raise TypeError("SampledMotion target must be one Mobject")
        self.mobject = mobject
        self.key_times = tuple(float(value) for value in key_times)
        self.positions = tuple(_point(value) for value in positions)
        self.angles = ((0.0,) * len(self.key_times) if angles is None
                       else tuple(float(value) for value in angles))
        if len(self.positions) != len(self.key_times) or len(self.angles) != len(self.key_times):
            raise ValueError("SampledMotion key, position and angle counts must match")
        self.anim_args = dict(kwargs)


def _bulk(values):
    from pyodide.ffi import to_js
    return to_js(memoryview(values))


def _strings(values):
    from pyodide.ffi import to_js
    return to_js(values)


def append_batch(scene, context, builder, animations, play_kwargs):
    """Marshal a consecutive run of records with one Python/WASM admission.

    Option presence is explicit: absent values never become NaN sentinels or
    Python-resolved defaults. The shared Rust resolver applies precedence for
    the entire batch, avoiding a hidden per-target option-resolution crossing.
    """
    import _manim_animation_options as options
    from _manim_scene import _canonical_play_options, _canonical_composition_rate_id

    ids, rates = [], []
    ends, presence = array("I"), array("B")
    phases, centers, angles, values = (array("d") for _ in range(4))
    for animation in animations:
        target = animation.mobject
        if target._scene is not scene or getattr(target, "_semantic_handle", None) is None:
            raise ValueError("SampledMotion target must already belong to this Scene")
        args = options.builder_args(animation)
        ids.append(str(target.id))
        phases.extend(animation.key_times)
        centers.extend(component for point in animation.positions for component in point)
        angles.extend(animation.angles)
        ends.append(len(phases))
        mask = 0
        for bit, name in ((1, "run_time"), (2, "lag_ratio"), (4, "path_arc")):
            if name in args:
                mask |= bit
                values.append(float(args[name]))
            else:
                values.append(0.0)
        if "reverse_rate_function" in args:
            mask |= 8
            if bool(args["reverse_rate_function"]):
                mask |= 16
        presence.append(mask)
        rates.append(options._optional_rate_func(args))
    play_duration = _canonical_play_options(dict(play_kwargs))
    play_rate = _canonical_composition_rate_id(play_kwargs) or ""
    play_lag = play_kwargs.get("lag_ratio")
    engine_call(
        builder.appendSampledMotionBatch,
        context, _strings(ids), _bulk(ends), _bulk(phases), _bulk(centers),
        _bulk(angles), _bulk(presence), _bulk(values), _strings(rates),
        play_duration, play_rate, None if play_lag is None else float(play_lag),
        operation="SampledMotion",
    )
