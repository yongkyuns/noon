"""Typed argument adaptation for shared persistent vector edits."""
import noon as _base
from _noon_errors import engine_call
from _manim_semantic_handles import _handle_for, _live_mutation_context, _live_constructor_context


def set_points_as_corners(value, points):
    sample_values = list(
        getattr(value, "_bezier_t_values", (0.0, 1 / 3, 2 / 3, 1.0))
    )
    if len(sample_values) != 4:
        raise TypeError(
            "set_points_as_corners requires four constructor interpolation samples"
        )
    if value.n_points_per_cubic_curve != 4:
        raise NotImplementedError(
            "set_points_as_corners requires the supported four-point cubic stride"
        )
    if sample_values != [0.0, 1 / 3, 2 / 3, 1.0]:
        from pyodide.ffi import to_js

        return edit_points(
            value,
            "setPointsAsCornersWithSamples",
            "liveSetPointsAsCornersWithSamples",
            points,
            array=True,
            suffix=(to_js(sample_values),),
        )
    return edit_points(value, "setPointsAsCorners", "liveSetPointsAsCorners", points, array=True)


def clear_points(value):
    return edit_points(value, "clearPoints", "liveClearPoints", ())


def set_points(value, points):
    return edit_points(value, "setPoints", "liveSetPoints", points, array=True)


def append_points(value, points):
    return edit_points(value, "appendPoints", "liveAppendPoints", points, array=True)


def start_new_path(value, point):
    import operator

    stride = operator.index(getattr(value, "n_points_per_curve", 4))
    if stride == 0:
        # Pinned Manim evaluates len(points) % n_points_per_curve first.
        raise ZeroDivisionError("integer modulo by zero")
    if not -(1 << 31) <= stride < (1 << 31):
        raise ValueError("n_points_per_curve exceeds the supported path stride")
    if stride == 4:
        return edit_points(value, "startNewPath", "liveStartNewPath", (point,))
    return edit_points(
        value,
        "startNewPathWithStride",
        "liveStartNewPathWithStride",
        (point,),
        suffix=(stride,),
    )


def resize_points(value, new_length, resize_func=None):
    import operator
    from _manim_updaters import canonical_callback_phase_active
    if canonical_callback_phase_active():
        raise NotImplementedError("callback path editing requires shared transient resource publication")

    length = operator.index(new_length)
    from _manim_path_queries import _query
    from _noon_errors import engine_call
    query = _query(value)
    try:
        point_count = int(query.pointCount)
        if point_count == length:
            return value
        coordinates = engine_call(query.points)
    finally:
        query.free()

    import numpy as np
    xy_points = np.asarray(coordinates, dtype=float).reshape((-1, 2))
    # Manim's point array has three columns. Noon stores planar XY internally,
    # so present an equivalent zero-Z array to caller-supplied resize functions.
    points = np.column_stack((xy_points, np.zeros(len(xy_points))))
    return set_points(value, resize_func(points, length))


def add_cubic_bezier_curves(value, curves):
    curves = list(curves)
    if curves and _looks_like_point(curves[0]):
        curves = [curves]
    points = [point for curve in curves for point in curve]
    return edit_points(
        value,
        "addCubicBezierCurves",
        "liveAddCubicBezierCurves",
        points,
        array=True,
    )


def add_points_as_corners(value, points):
    from _manim_updaters import canonical_callback_phase_active

    if canonical_callback_phase_active():
        raise NotImplementedError("callback path editing requires shared transient resource publication")
    import operator

    stride = operator.index(getattr(value, "n_points_per_cubic_curve", 4))
    sample_values = [
        float(sample)
        for sample in getattr(value, "_bezier_t_values", (0.0, 1 / 3, 2 / 3, 1.0))
    ]
    default_samples = [0.0, 1 / 3, 2 / 3, 1.0]
    if stride == 4 and sample_values == default_samples:
        return edit_points(
            value,
            "addPointsAsCorners",
            "liveAddPointsAsCorners",
            points,
            array=True,
        )
    value.throw_error_if_no_points()
    points = list(points)
    if not points:
        return value
    if stride == 0:
        raise ZeroDivisionError("integer modulo by zero")
    if stride < 0:
        raise ValueError("negative dimensions are not allowed")
    if len(sample_values) > stride:
        raise NotImplementedError(
            "add_points_as_corners cannot lower constructor samples across stride groups"
        )
    if stride > len(sample_values) + 1:
        raise NotImplementedError(
            "add_points_as_corners would leave uninitialized points for this stride"
        )
    from pyodide.ffi import to_js

    return edit_points(
        value,
        "addPointsAsCornersWithStride",
        "liveAddPointsAsCornersWithStride",
        points,
        array=True,
        suffix=(stride, to_js(sample_values)),
    )


def add_subpath(value, points):
    assert len(points) % 4 == 0
    curves = [points[index : index + 4] for index in range(0, len(points), 4)]
    return add_cubic_bezier_curves(value, curves)


def add_smooth_curve_to(value, *points):
    if len(points) not in (1, 2):
        raise ValueError("Only call add_smooth_curve_to with 1 or 2 points")
    stride = _curve_append_stride(value, empty_checks_first=False)
    if _line_append_arity_is_invalid(value) and (
        value.has_new_path_started()
        if stride == 4
        else _has_new_path_started_with_stride(value, stride)
    ):
        _validate_line_append_arity(value)
    return edit_points(
        value,
        "addSmoothCurveTo",
        "liveAddSmoothCurveTo",
        points,
        array=True,
        suffix=_curve_append_suffix(stride),
    )


def add_line_to(value, point):
    _validate_line_append_arity(value)
    stride = _curve_append_stride(value, empty_checks_first=True)
    return edit_points(
        value,
        "addLineTo",
        "liveAddLineTo",
        (point,),
        suffix=_curve_append_suffix(stride),
    )


def add_quadratic_bezier_curve_to(value, control, anchor):
    stride = _curve_append_stride(value, empty_checks_first=True)
    return edit_points(
        value,
        "addQuadraticBezierCurveTo",
        "liveAddQuadraticBezierCurveTo",
        (control, anchor),
        suffix=_curve_append_suffix(stride),
    )


def add_cubic_bezier_curve_to(value, control1, control2, anchor):
    stride = _curve_append_stride(value, empty_checks_first=True)
    return edit_points(
        value,
        "addCubicBezierCurveTo",
        "liveAddCubicBezierCurveTo",
        (control1, control2, anchor),
        suffix=_curve_append_suffix(stride),
    )


def _curve_append_suffix(stride):
    return () if stride == 4 else (stride,)


def _has_new_path_started_with_stride(value, stride):
    from _manim_path_queries import _query

    query = _query(value)
    try:
        return bool(engine_call(query.hasNewPathStarted, stride))
    finally:
        query.free()


def _curve_append_stride(value, *, empty_checks_first):
    import operator

    stride = operator.index(getattr(value, "n_points_per_cubic_curve", 4))
    if not -(1 << 31) <= stride < (1 << 31):
        raise ValueError("n_points_per_cubic_curve exceeds the supported path stride")
    if stride == 0:
        if not empty_checks_first:
            raise ZeroDivisionError("integer modulo by zero")
        from _manim_path_queries import _query

        query = _query(value)
        try:
            if int(query.pointCount) != 0:
                raise ZeroDivisionError("integer modulo by zero")
        finally:
            query.free()
    return stride


def _validate_line_append_arity(value):
    try:
        sample_count = len(value._bezier_t_values)
    except AttributeError:
        return
    generated_points = sample_count - 1
    if generated_points == 3:
        return
    method = f"{type(value).__name__}.add_cubic_bezier_curve_to()"
    parameter_names = ("handle1", "handle2", "anchor")
    if generated_points < 3:
        missing = parameter_names[max(generated_points, 0) :]
        if len(missing) == 1:
            description = f"1 required positional argument: '{missing[0]}'"
        else:
            names = ", ".join(f"'{name}'" for name in missing[:-1])
            names = f"{names} and '{missing[-1]}'" if names else f"'{missing[-1]}'"
            description = f"{len(missing)} required positional arguments: {names}"
        raise TypeError(f"{method} missing {description}")
    raise TypeError(
        f"{method} takes 4 positional arguments but {generated_points + 1} were given"
    )


def _line_append_arity_is_invalid(value):
    try:
        return len(value._bezier_t_values) != 4
    except AttributeError:
        return False


def scale_handle_to_anchor_distances(value, factor):
    from _manim_updaters import canonical_callback_phase_active
    if canonical_callback_phase_active():
        raise NotImplementedError("callback path editing requires shared transient resource publication")
    factor = float(factor)
    handle = _handle_for(value)
    if handle is None:
        raise RuntimeError("path editing requires a shared semantic handle")
    context = _live_mutation_context(value) or _live_constructor_context("path")
    if context is None:
        engine_call(
            handle.scaleHandleToAnchorDistances,
            factor,
            operation="VMobject.scale_handle_to_anchor_distances",
        )
    else:
        engine_call(
            context.liveScaleHandleToAnchorDistances,
            handle,
            factor,
            operation="VMobject.scale_handle_to_anchor_distances",
        )
    return value


def set_anchors_and_handles(value, anchors1, handles1, handles2, anchors2):
    assert len(anchors1) == len(handles1) == len(handles2) == len(anchors2)
    if value.n_points_per_cubic_curve != 4:
        raise NotImplementedError(
            "set_anchors_and_handles requires the supported four-point cubic stride"
        )
    curves = [
        [anchor1, handle1, handle2, anchor2]
        for anchor1, handle1, handle2, anchor2 in zip(
            anchors1, handles1, handles2, anchors2, strict=True
        )
    ]
    return edit_points(
        value,
        "setCubicBezierCurves",
        "liveSetCubicBezierCurves",
        [point for curve in curves for point in curve],
        array=True,
    )


def append_vectorized_mobject(value, source):
    from _manim_compat import VMobject
    from _manim_updaters import canonical_callback_phase_active
    if not isinstance(source, VMobject):
        raise TypeError("append_vectorized_mobject requires a VMobject")
    if canonical_callback_phase_active():
        raise NotImplementedError("callback path editing requires shared transient resource publication")
    stride = _curve_append_stride(value, empty_checks_first=False)
    handle, source_handle = _handle_for(value), _handle_for(source)
    if handle is None or source_handle is None:
        raise RuntimeError("path editing requires shared semantic handles")
    context = (
        _live_mutation_context(value)
        or _live_mutation_context(source)
        or _live_constructor_context("path")
    )
    if context is None:
        method = (
            handle.appendVectorizedMobject
            if stride == 4
            else handle.appendVectorizedMobjectWithStride
        )
        engine_call(
            method,
            source_handle,
            *(() if stride == 4 else (stride,)),
            operation="VMobject.append_vectorized_mobject",
        )
    else:
        method = (
            context.liveAppendVectorizedMobject
            if stride == 4
            else context.liveAppendVectorizedMobjectWithStride
        )
        engine_call(
            method,
            handle,
            source_handle,
            *(() if stride == 4 else (stride,)),
            operation="VMobject.append_vectorized_mobject",
        )
    return value


def close_path(value):
    from _manim_path_queries import is_closed

    if is_closed(value):
        return value
    stride = _curve_append_stride(value, empty_checks_first=False)
    tolerance = float(getattr(value, "tolerance_for_point_equality", 1e-6))
    sample_values = list(getattr(value, "_bezier_t_values", (0.0, 1 / 3, 2 / 3, 1.0)))
    if len(sample_values) != 4:
        raise TypeError("close_path requires three constructor interpolation samples")
    from pyodide.ffi import to_js

    return edit_points(
        value,
        "closePathWithStride",
        "liveClosePathWithStride",
        (),
        suffix=(stride, tolerance, to_js(sample_values)),
    )


def _looks_like_point(value):
    if isinstance(value, _base.Vec2):
        return True
    try:
        size = len(value)
    except (TypeError, AttributeError):
        return False
    return size in (2, 3) and all(not hasattr(component, "__len__") for component in value)


def edit_points(value, method, live_method, points, *, array=False, suffix=()):
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
        engine_call(
            getattr(handle, method),
            *values,
            *suffix,
            operation="VMobject." + method,
        )
    else:
        engine_call(
            getattr(context, live_method),
            handle,
            *values,
            *suffix,
            operation="VMobject." + method,
        )
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


def align_points(value, other):
    from _manim_compat import VMobject
    from _manim_updaters import canonical_callback_phase_active
    if not isinstance(other, VMobject):
        raise TypeError("align_points requires a VMobject")
    if canonical_callback_phase_active():
        raise NotImplementedError("callback path editing requires shared transient resource publication")
    handle, other_handle = _handle_for(value), _handle_for(other)
    if handle is None or other_handle is None:
        raise RuntimeError("path alignment requires shared semantic handles")
    context = _live_mutation_context(value) or _live_mutation_context(other) or _live_constructor_context("path")
    if context is None:
        engine_call(handle.alignPoints, other_handle, operation="VMobject.align_points")
    else:
        engine_call(context.liveAlignPoints, handle, other_handle, operation="VMobject.align_points")
    return value
