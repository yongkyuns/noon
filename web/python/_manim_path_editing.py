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


def set_points_smoothly(value, points):
    value.set_points_as_corners(points)
    return value.make_smooth()


def clear_points(value):
    return edit_points(value, "clearPoints", "liveClearPoints", ())


def set_points(value, points):
    return edit_points(value, "setPoints", "liveSetPoints", points, array=True)


def apply_over_attr_arrays(value, func):
    """Map the object's Rust-owned point snapshot, then publish it via Rust."""
    attrs = list(value.get_array_attrs())
    unsupported = [attr for attr in attrs if attr != "points"]
    if unsupported:
        raise NotImplementedError(
            "apply_over_attr_arrays currently supports only the shared Rust points array"
        )
    if not attrs:
        return value

    from _manim_updaters import canonical_callback_phase_active
    if canonical_callback_phase_active():
        raise NotImplementedError(
            "array mapping is unavailable during staged callbacks"
        )

    from _manim_path_queries import _query
    for attr in attrs:
        query = _query(value)
        try:
            coordinates = engine_call(query.points)
        finally:
            query.free()

        try:
            import numpy as np
        except ImportError:
            rows = [
                [float(coordinates[index]), float(coordinates[index + 1]), 0.0]
                for index in range(0, len(coordinates), 2)
            ]
        else:
            xy = np.asarray(coordinates, dtype=np.float64).reshape((-1, 2))
            rows = np.column_stack((xy, np.zeros(len(xy), dtype=np.float64)))

        mapped = func(rows)
        mapped = _mapped_planar_points(mapped)
        edit_points(
            value,
            "setPoints",
            "liveSetPoints",
            mapped,
            array=True,
        )
    return value


def apply_points_function_about_point(value, func, about_point=None, about_edge=None):
    """Apply a caller-supplied array mapping to Rust-selected family paths."""
    from _manim_updaters import canonical_callback_phase_active
    if canonical_callback_phase_active():
        raise NotImplementedError(
            "family point mappings are unavailable during staged callbacks"
        )

    try:
        import numpy as np
    except ImportError:
        np = None

    if about_point is None:
        if about_edge is None:
            about_edge = _base.ORIGIN
        edge = _planar_critical_direction(about_edge)
        about_point = value.get_critical_point(edge)
    pivot = _point3_array(about_point, np)

    def map_about_pivot(points):
        if np is not None:
            centered = points - pivot
            return func(centered) + pivot
        centered = [
            [float(point[index]) - pivot[index] for index in range(3)]
            for point in points
        ]
        mapped = func(centered)
        return [
            [float(point[index]) + pivot[index] for index in range(3)]
            for point in mapped
        ]

    for member in value.family_members_with_points():
        member.apply_over_attr_arrays(map_about_pivot)
    return value


def apply_function(value, function, *, about_point=None, about_edge=None):
    """Apply a pointwise callback with Manim's origin default and family scope."""
    if about_point is None and about_edge is None:
        about_point = _base.ORIGIN

    try:
        import numpy as np
    except ImportError:
        np = None

    def map_points(points):
        if np is not None:
            return np.apply_along_axis(function, 1, points)
        return [function(point) for point in points]

    return apply_points_function_about_point(
        value,
        map_points,
        about_point=about_point,
        about_edge=about_edge,
    )


def apply_complex_function(value, function, *, about_point=None, about_edge=None):
    """Map each planar path point through a complex callback, preserving z."""
    def map_point(point):
        x, y, z = point
        result = function(complex(float(x), float(y)))
        return [float(result.real), float(result.imag), float(z)]

    return apply_function(
        value,
        map_point,
        about_point=about_point,
        about_edge=about_edge,
    )


def apply_matrix(value, matrix, *, about_point=None, about_edge=None):
    """Apply Manim's expanded linear transform over supported family paths."""
    if about_point is None and about_edge is None:
        about_point = _base.ORIGIN

    try:
        import numpy as np
    except ModuleNotFoundError as error:
        if error.name != "numpy":
            raise
        np = None

    dimension = getattr(value, "dim", 3)
    if np is not None:
        full_matrix = np.identity(dimension)
        matrix = np.array(matrix)
        full_matrix[: matrix.shape[0], : matrix.shape[1]] = matrix

        def map_points(points):
            return np.dot(points, full_matrix.T)
    else:
        full_matrix = _expanded_matrix_without_numpy(dimension, matrix)

        def map_points(points):
            if len(full_matrix) != 3:
                raise ValueError("matrix dimension does not match planar point arrays")
            return [
                [
                    sum(
                        float(point[column]) * full_matrix[row][column]
                        for column in range(3)
                    )
                    for row in range(3)
                ]
                for point in points
            ]

    return apply_points_function_about_point(
        value,
        map_points,
        about_point=about_point,
        about_edge=about_edge,
    )


def _expanded_matrix_without_numpy(dim, matrix):
    import operator

    dimension = operator.index(dim)
    if dimension < 0:
        raise ValueError("negative dimensions are not allowed")
    rows = [list(row) for row in matrix]
    if not rows:
        raise IndexError("tuple index out of range")
    columns = len(rows[0])
    if any(len(row) != columns for row in rows):
        raise ValueError("matrix rows must all have the same length")
    if len(rows) > dimension or columns > dimension:
        raise ValueError("matrix shape does not fit the configured dimension")

    expanded = [
        [1.0 if row == column else 0.0 for column in range(dimension)]
        for row in range(dimension)
    ]
    for row_index, row in enumerate(rows):
        for column_index, component in enumerate(row):
            expanded[row_index][column_index] = float(component)
    return expanded


def _planar_critical_direction(direction):
    try:
        dimensions = len(direction)
    except (TypeError, AttributeError):
        return _base._as_vec2(direction)
    if dimensions == 3:
        return _base._as_vec2((direction[0], direction[1]))
    return _base._as_vec2(direction)


def _point3_array(point, numpy):
    try:
        dimensions = len(point)
    except (TypeError, AttributeError):
        raise TypeError("about_point must have two or three coordinates") from None
    if dimensions == 2:
        coordinates = (float(point[0]), float(point[1]), 0.0)
    elif dimensions == 3:
        coordinates = (float(point[0]), float(point[1]), float(point[2]))
    else:
        raise ValueError("about_point must have two or three coordinates")
    return (
        numpy.asarray(coordinates, dtype=numpy.float64)
        if numpy is not None
        else coordinates
    )


def _mapped_planar_points(points):
    """Validate the planar Rust path boundary before publishing mapped rows."""
    try:
        import numpy as np
    except ImportError:
        rows = [list(point) for point in points]
        if not rows:
            return []
        if any(len(row) not in (2, 3) for row in rows):
            raise ValueError("mapped points must have two or three coordinates")
        if any(len(row) == 3 and float(row[2]) != 0.0 for row in rows):
            raise NotImplementedError("nonzero-z mapped points are unsupported")
        return [(float(row[0]), float(row[1])) for row in rows]

    mapped = np.asarray(points, dtype=np.float64)
    if mapped.ndim == 1 and mapped.size == 0:
        return []
    if mapped.ndim != 2 or mapped.shape[1] not in (2, 3):
        raise ValueError("mapped points must be a two-dimensional point array")
    if mapped.shape[1] == 3 and np.any(mapped[:, 2] != 0.0):
        raise NotImplementedError("nonzero-z mapped points are unsupported")
    return mapped[:, :2]


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
    sample_values = list(
        getattr(value, "_bezier_t_values", (0.0, 1 / 3, 2 / 3, 1.0))
    )
    if sample_values != [0.0, 1 / 3, 2 / 3, 1.0]:
        from pyodide.ffi import to_js

        return edit_points(
            value,
            "addLineToWithSamples",
            "liveAddLineToWithSamples",
            (point,),
            suffix=(stride, to_js(sample_values)),
        )
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
    import operator

    stride = operator.index(getattr(value, "n_points_per_cubic_curve", 4))
    if stride != 4:
        members = value.family_members_with_points()
        if any(member.get_num_points() >= stride for member in members):
            raise NotImplementedError(
                "scale_handle_to_anchor_distances requires the supported four-point cubic stride"
            )
        return value
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
    from _manim_compat import Group, VGroup
    from _manim_updaters import canonical_callback_phase_active
    assert mode in ["jagged", "smooth"], 'mode must be either "jagged" or "smooth"'
    if canonical_callback_phase_active():
        raise NotImplementedError("callback path editing requires shared transient resource publication")
    family = isinstance(value, (Group, VGroup))
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


def reverse_points(value):
    from _manim_compat import Group, VGroup
    from _manim_updaters import canonical_callback_phase_active

    if canonical_callback_phase_active():
        raise NotImplementedError("callback path editing requires shared transient resource publication")
    family = isinstance(value, (Group, VGroup))
    handle = (
        getattr(value, "_semantic_family_handle", None)
        if family
        else _handle_for(value)
    )
    # The pinned base Mobject has no point arrays and no children, so its
    # inherited reverse_points implementation is an identity-preserving no-op.
    if handle is None and not family:
        return value
    if handle is None:
        raise RuntimeError("family point editing requires a shared Rust semantic handle")
    from _manim_semantic_handles import _group_target_context
    context = (
        _group_target_context(value)
        if family
        else (_live_mutation_context(value) or _live_constructor_context("path"))
    )
    if context is None:
        engine_call(
            handle.reversePoints,
            operation="Mobject.reverse_points",
        )
    else:
        engine_call(
            context.liveReverseFamilyPoints if family else context.liveReversePoints,
            handle,
            operation="Mobject.reverse_points",
        )
    return value


def repeat_points(value, count):
    import operator
    from _manim_compat import Group, VGroup
    from _manim_updaters import canonical_callback_phase_active

    if canonical_callback_phase_active():
        raise NotImplementedError("callback path editing requires shared transient resource publication")
    members = value.family_members_with_points()
    if not members:
        return value
    try:
        repeat_count = operator.index(count)
    except TypeError:
        # Preserve list repetition's native error type/message for point-owning
        # objects without allocating a count-sized validation list.
        [] * count
        raise AssertionError("unreachable")
    if repeat_count <= 0:
        raise TypeError("reduce() of empty iterable with no initial value")
    if repeat_count > 0xFFFFFFFF:
        raise OverflowError("Python int too large to convert to C unsigned int")

    family = isinstance(value, (Group, VGroup))
    handle = (
        getattr(value, "_semantic_family_handle", None)
        if family
        else _handle_for(value)
    )
    if handle is None:
        raise RuntimeError("family point editing requires a shared Rust semantic handle")
    from _manim_semantic_handles import _group_target_context
    context = (
        _group_target_context(value)
        if family
        else (_live_mutation_context(value) or _live_constructor_context("path"))
    )
    if context is None:
        engine_call(
            handle.repeatPoints,
            repeat_count,
            operation="Mobject.repeat",
        )
    else:
        method = context.liveRepeatFamilyPoints if family else context.liveRepeatPoints
        engine_call(method, handle, repeat_count, operation="Mobject.repeat")
    return value


def align_points(value, other):
    from _manim_compat import VMobject
    from _manim_updaters import canonical_callback_phase_active
    if not isinstance(other, VMobject):
        raise TypeError("align_points requires a VMobject")
    if canonical_callback_phase_active():
        raise NotImplementedError("callback path editing requires shared transient resource publication")
    value.align_rgbas(other)
    handle, other_handle = _handle_for(value), _handle_for(other)
    if handle is None or other_handle is None:
        raise RuntimeError("path alignment requires shared semantic handles")
    context = _live_mutation_context(value) or _live_mutation_context(other) or _live_constructor_context("path")
    if context is None:
        engine_call(handle.alignPoints, other_handle, operation="VMobject.align_points")
    else:
        engine_call(context.liveAlignPoints, handle, other_handle, operation="VMobject.align_points")
    return value


def align_rgbas(value, other):
    """Validate the shared single-row paint profile used by VMobject alignment.

    Manim stretches fill, stroke, and background-stroke RGBA arrays to equal
    lengths. Noon currently represents one solid RGBA row per layer in shared
    Rust; unequal per-object gradient arrays have no shared representation.
    """
    left_fill = value.get_fill_rgbas()
    right_fill = other.get_fill_rgbas()
    left_stroke = value.get_stroke_rgbas()
    right_stroke = other.get_stroke_rgbas()
    left_background_stroke = value.get_stroke_rgbas(background=True)
    right_background_stroke = other.get_stroke_rgbas(background=True)
    layers = (
        (left_fill, right_fill),
        (left_stroke, right_stroke),
        (left_background_stroke, right_background_stroke),
    )
    if any(len(left) != len(right) for left, right in layers):
        raise NotImplementedError(
            "align_rgbas for unequal per-object arrays requires shared gradient paint"
        )
    return value
