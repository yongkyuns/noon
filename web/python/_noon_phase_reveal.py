"""Explicit phase timing over an existing retained path, evaluated in Rust."""
from __future__ import annotations

from array import array

import noon as _base
from _noon_errors import engine_call


class PhaseReveal:
    """Reveal one continuous retained path using phase at each curve boundary.

    ``key_times`` includes 0 and 1 and strictly increases between them. Supply
    one more time than the path's drawable curve count. Within an interval,
    progress is the corresponding curve parameter, not arc length. For a
    piecewise-linear pen, use its full phase array and the same polyline points.

    The shared Rust timeline applies duration, easing and reversal before this
    phase map. The path and its identity stay retained through seeking and
    restart. Completion keeps the mapped terminal reveal; ``remover=True``
    removes membership at completion. Detached targets are introduced by the
    normal scene transaction; an existing direct scene member is also valid.

    This native primitive supports a single static planar path. Multiple
    contours, active content/morph drivers, host callbacks, dissipating trails
    and pen-up/clearing operations are outside this slice. Ordinary Create
    retains its uniform curve-count timing.
    """

    def __init__(self, mobject, key_times, **kwargs):
        if not isinstance(mobject, _base.Mobject):
            raise TypeError("PhaseReveal target must be one Mobject")
        self.mobject = mobject
        self.key_times = tuple(float(value) for value in key_times)
        self.introducer = kwargs.pop("introducer", None)
        self.remover = kwargs.pop("remover", None)
        self.anim_args = dict(kwargs)


def _bulk(values):
    from pyodide.ffi import to_js
    return to_js(memoryview(array("d", values)))


def append(builder, animation, entering_id, play_kwargs):
    """Forward authored options and one timing table without Python scheduling."""
    import _manim_animation_options as options
    from _manim_scene import _canonical_play_options, _canonical_composition_rate_id

    args = options.builder_args(animation)
    optional_number = lambda key: None if key not in args else float(args[key])
    optional_bool = lambda value: None if value is None else bool(value)
    engine_call(
        builder.appendPhaseReveal,
        entering_id, animation.mobject._semantic_handle, _bulk(animation.key_times),
        optional_number("run_time"), options._optional_rate_func(args) or None,
        optional_number("lag_ratio"), optional_number("path_arc"),
        optional_bool(args.get("reverse_rate_function")),
        optional_bool(animation.introducer), optional_bool(animation.remover),
        _canonical_play_options(dict(play_kwargs)),
        _canonical_composition_rate_id(play_kwargs),
        None if "lag_ratio" not in play_kwargs else float(play_kwargs["lag_ratio"]),
        operation="PhaseReveal",
    )
