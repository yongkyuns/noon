"""Typed argument adaptation for shared persistent vector edits."""
import noon as _base
from _noon_errors import engine_call
from _manim_semantic_handles import _handle_for, _live_mutation_context, _live_constructor_context


def set_points_as_corners(value, points):
    from _manim_updaters import canonical_callback_phase_active
    if canonical_callback_phase_active():
        raise NotImplementedError("callback path editing requires shared transient resource publication")
    from pyodide.ffi import to_js
    coordinates = [_base._as_vec2(point) for point in points]
    values = to_js([component for point in coordinates for component in (point.x, point.y)])
    handle = _handle_for(value)
    if handle is None:
        raise RuntimeError("path editing requires a shared semantic handle")
    context = _live_mutation_context(value) or _live_constructor_context("path")
    if context is None:
        engine_call(handle.setPointsAsCorners, values, operation="VMobject.set_points_as_corners")
    else:
        engine_call(context.liveSetPointsAsCorners, handle, values, operation="VMobject.set_points_as_corners")
    return value
