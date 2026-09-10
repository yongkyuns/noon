"""Typed argument adaptation for shared persistent vector edits."""
import noon as _base
from _noon_errors import engine_call
from _manim_semantic_handles import _handle_for, _live_mutation_context, _live_constructor_context


def set_points_as_corners(value, points):
    return edit_points(value, "setPointsAsCorners", "liveSetPointsAsCorners", points, array=True)


def edit_points(value, method, live_method, points, *, array=False):
    from _manim_updaters import canonical_callback_phase_active
    if canonical_callback_phase_active():
        raise NotImplementedError("callback path editing requires shared transient resource publication")
    coordinates = [_base._as_vec2(point) for point in points]
    values = [component for point in coordinates for component in (point.x, point.y)]
    if array:
        from pyodide.ffi import to_js
        values = [to_js(values)]
    handle = _handle_for(value)
    if handle is None:
        raise RuntimeError("path editing requires a shared semantic handle")
    context = _live_mutation_context(value) or _live_constructor_context("path")
    if context is None:
        engine_call(getattr(handle, method), *values, operation="VMobject." + method)
    else:
        engine_call(getattr(context, live_method), handle, *values, operation="VMobject." + method)
    return value


def pointwise_become_partial(value, source, a, b):
    from _manim_compat import VMobject
    from _manim_updaters import canonical_callback_phase_active
    if not isinstance(source, VMobject):
        raise TypeError("pointwise_become_partial requires a VMobject source")
    if canonical_callback_phase_active():
        raise NotImplementedError("callback path editing requires shared transient resource publication")
    handle, source_handle = _handle_for(value), _handle_for(source)
    if handle is None or source_handle is None:
        raise RuntimeError("partial path editing requires shared semantic handles")
    context = _live_mutation_context(value) or _live_mutation_context(source) or _live_constructor_context("path")
    if context is None:
        engine_call(handle.pointwiseBecomePartial, source_handle, float(a), float(b), operation="VMobject.pointwise_become_partial")
    else:
        engine_call(context.livePointwiseBecomePartial, handle, source_handle, float(a), float(b), operation="VMobject.pointwise_become_partial")
    return value


def insert_n_curves(value, n):
    import operator
    from _manim_updaters import canonical_callback_phase_active
    count = operator.index(n)
    if not 0 <= count <= 0xFFFFFFFF:
        raise ValueError("n must be between 0 and 4294967295")
    if canonical_callback_phase_active():
        raise NotImplementedError("callback path editing requires shared transient resource publication")
    handle = _handle_for(value)
    if handle is None:
        raise RuntimeError("path editing requires a shared semantic handle")
    context = _live_mutation_context(value) or _live_constructor_context("path")
    if context is None:
        engine_call(handle.insertNCurves, count, operation="VMobject.insert_n_curves")
    else:
        engine_call(context.liveInsertNCurves, handle, count, operation="VMobject.insert_n_curves")
    return value


def change_anchor_mode(value, mode):
    from _manim_compat import Group
    from _manim_updaters import canonical_callback_phase_active
    if mode not in ("smooth", "jagged"):
        raise ValueError("mode must be 'smooth' or 'jagged'")
    if canonical_callback_phase_active():
        raise NotImplementedError("callback path editing requires shared transient resource publication")
    family = isinstance(value, Group)
    handle = getattr(value, "_semantic_family_handle", None) if family else _handle_for(value)
    if handle is None:
        raise RuntimeError("path editing requires a shared semantic handle")
    from _manim_semantic_handles import _group_target_context
    context = _group_target_context(value) if family else (_live_mutation_context(value) or _live_constructor_context("path"))
    if context is None:
        engine_call(handle.makeSmooth if mode == "smooth" else handle.makeJagged)
    else:
        method = context.liveChangeFamilyAnchorMode if family else context.liveChangeAnchorMode
        engine_call(method, handle, mode == "smooth")
    return value
