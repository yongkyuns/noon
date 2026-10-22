"""Argument/result adaptation for immutable shared Rust path observations."""
import operator
import noon as _base
import _noon_ir as _ir
from _noon_errors import engine_call
from _manim_semantic_handles import (
    _point_array_observation,
    _critical_point_observation,
    _default_point_query_hooks,
    _boundary_point_observation,
    _center_of_mass_observation,
    _extremum_from_points_observation,
    _family_point_count_observation,
    _length_over_dim_observation,
    _path_boundary_points_observation,
    _vector_boundary_points_observation,
    _point_reduction_tree_observation,
    _typed_manim_observation,
    _handle_for,
)


def _reject_staged_custom_stride_query(value):
    stride = getattr(value, "n_points_per_cubic_curve", 4)
    try:
        integer_stride = operator.index(stride)
    except TypeError:
        integer_stride = None
    if integer_stride != 4:
        from _manim_updaters import canonical_callback_phase_active

        if canonical_callback_phase_active():
            raise NotImplementedError(
                "custom-stride VMobject path queries are unavailable during staged callbacks"
            )


def _query(value):
    _reject_staged_custom_stride_query(value)
    query = _typed_manim_observation(value, "pathQuery", "queryMobjectPath")
    if query is None:
        raise RuntimeError("path queries require the shared Rust authoring host")
    return query


def point_from_proportion(value, alpha):
    alpha = _ir._finite_number("alpha", alpha)
    query = _query(value)
    try:
        stride = getattr(value, "n_points_per_cubic_curve", 4)
        try:
            integer_stride = operator.index(stride)
        except TypeError:
            integer_stride = None
        if integer_stride != 4:
            return _custom_stride_point_from_proportion(
                value, alpha, query, stride
            )

        coordinates = engine_call(query.pointFromProportion, alpha)
        return _base.Vec2(float(coordinates[0]), float(coordinates[1]))
    finally:
        query.free()


def _custom_stride_point_from_proportion(value, alpha, query, stride):
    if alpha < 0 or alpha > 1:
        raise ValueError(f"Alpha {alpha} not between 0 and 1.")
    point_count = int(query.pointCount)
    if point_count == 0:
        raise Exception(
            "Cannot call Mobject.point_from_proportion for a Mobject with no points"
        )
    if alpha == 1:
        points = engine_call(query.points)
        return _base.Vec2(float(points[-2]), float(points[-1]))

    count = point_count // stride
    selections = []
    lengths = []
    for index in range(count):
        selection = _nth_curve_selection(value, index, query)
        if selection is None:
            raise IndexError("index 0 is out of bounds for axis 1 with size 0")
        length = _curve_length_from_selection(query, selection, None)
        selections.append(selection)
        lengths.append(length)

    target_length = alpha * sum(lengths)
    current_length = 0.0
    for selection, length in zip(selections, lengths):
        if current_length + length >= target_length:
            residue = (
                (target_length - current_length) / length
                if length != 0
                else 0.0
            )
            coordinates = engine_call(
                query.bezierPointsAtRange,
                selection[1],
                selection[2],
                [residue],
            )
            return _base.Vec2(float(coordinates[0]), float(coordinates[1]))
        current_length += length
    raise Exception(
        "Not sure how you reached here, please file a bug report at "
        "https://github.com/ManimCommunity/manim/issues/new/choose"
    )


def proportion_from_point(value, point):
    """Return the shared Rust path's arc-length proportion for a point on it."""
    try:
        dimensions = len(point)
    except (TypeError, AttributeError):
        raise ValueError("point must have two or three coordinates") from None
    if dimensions == 2:
        coordinates = (float(point[0]), float(point[1]), 0.0)
    elif dimensions == 3:
        coordinates = (float(point[0]), float(point[1]), float(point[2]))
    else:
        raise ValueError("point must have two or three coordinates")

    query = _query(value)
    try:
        stride = getattr(value, "n_points_per_cubic_curve", 4)
        try:
            integer_stride = operator.index(stride)
        except TypeError:
            integer_stride = None
        if integer_stride != 4:
            return _custom_stride_proportion_from_point(
                value, query, stride, coordinates, point
            )
        proportion = engine_call(query.proportionFromPoint, *coordinates)
        if proportion is None:
            raise ValueError(f"Point {point} does not lie on this curve.")
        return float(proportion)
    finally:
        query.free()


def _custom_stride_proportion_from_point(value, query, stride, coordinates, point):
    point_count = int(query.pointCount)
    count = point_count // stride
    ranges = []
    for index in range(count):
        selection = _nth_curve_selection(value, index, query)
        if selection is None:
            raise IndexError("index 0 is out of bounds for axis 1 with size 0")
        if selection[0] == "cubic":
            start = 4 * selection[1]
            ranges.extend((start, start + 4))
        else:
            ranges.extend((selection[1], selection[2]))
    proportion = engine_call(query.proportionFromPointForRanges, ranges, *coordinates)
    if proportion is None:
        raise ValueError(f"Point {point} does not lie on this curve.")
    return float(proportion)


def arc_length(value, sample_points_per_curve):
    query = _query(value)
    try:
        point_count = int(query.pointCount)
        curve_count = point_count // getattr(value, "n_points_per_cubic_curve", 4)
        curve_indices = range(curve_count)
        if curve_count <= 0:
            return 0.0
        samples = _curve_sample_count(sample_points_per_curve)
        if samples == 0:
            _raise_zero_sample_length_axis_error()
        total = 0.0
        for index in curve_indices:
            selection = _nth_curve_selection(value, index, query)
            if selection is None:
                raise IndexError("index 0 is out of bounds for axis 1 with size 0")
            total += _curve_length_from_selection(query, selection, samples)
        return float(total)
    finally:
        query.free()


def endpoint(value, end):
    query = _query(value)
    try:
        coordinates = engine_call(query.end if end else query.start)
        # ManimCE returns independent NumPy point rows for get_start/get_end.
        # Reuse the optional array conversion used by the other path queries,
        # retaining Vec2 for installations without NumPy.
        return _points_array([
            _base.Vec2(float(coordinates[0]), float(coordinates[1]))
        ])[0]
    finally:
        query.free()


def last_point(value):
    query = _query(value)
    try:
        if int(query.pointCount) == 0:
            raise IndexError("index -1 is out of bounds for axis 0 with size 0")
        coordinates = engine_call(query.end)
        return _base.Vec2(float(coordinates[0]), float(coordinates[1]))
    finally:
        query.free()


def _points(coordinates):
    return [_base.Vec2(float(coordinates[i]), float(coordinates[i + 1])) for i in range(0, len(coordinates), 2)]


def _points_array(points):
    """Match Manim's NumPy point containers when NumPy is available."""
    try:
        import numpy as np
    except ImportError:
        return points
    rows = [[float(point[0]), float(point[1])] for point in points]
    return np.asarray(rows, dtype=np.float64).reshape((-1, 2))


def _point_rows(points):
    """Return Manim's list of NumPy point rows when NumPy is available."""
    try:
        import numpy as np
    except ImportError:
        return points
    return list(_points_array(points))


def _cubic_bezier_point_ranges(value, point_count):
    points_per_curve = operator.index(
        getattr(value, "n_points_per_cubic_curve", 4)
    )
    if point_count > 0xFFFFFFFF:
        raise ValueError("points exceed the supported array size")
    if points_per_curve == 0:
        # Preserve the source helper's native integer modulo error.
        point_count % points_per_curve
    try:
        stride_number = float(points_per_curve)
    except OverflowError:
        stride_number = float("inf") if points_per_curve > 0 else float("-inf")
    handle = _handle_for(value)
    if handle is None:
        raise RuntimeError("cubic tuple queries require a shared Rust Mobject handle")
    ranges = engine_call(handle.cubicBezierPointRanges, point_count, stride_number)
    indices = [int(index) for index in ranges]
    if len(indices) % 2:
        raise RuntimeError("Rust cubic tuple query returned an incomplete range")
    return indices


def _curve_count_for_point_count(value, point_count):
    points_per_curve = getattr(value, "n_points_per_cubic_curve", 4)
    try:
        integer_stride = operator.index(points_per_curve)
    except TypeError:
        # Manim's property is mutable and get_num_curves uses Python's //
        # directly, so preserve its behavior for non-integer assignments.
        return point_count // points_per_curve
    points_per_curve = integer_stride
    if points_per_curve <= 0:
        # Match len(self.points) // n_points_per_cubic_curve, including the
        # source's negative result and division-by-zero error.
        return point_count // points_per_curve
    return len(_cubic_bezier_point_ranges(value, point_count)) // 2


def curve_count(value):
    query = _query(value)
    try:
        point_count = int(query.pointCount)
    finally:
        query.free()
    return _curve_count_for_point_count(value, point_count)


def point_count(value):
    family_count = _family_point_count_observation(value)
    if family_count is not None:
        return family_count
    _reject_staged_custom_stride_query(value)
    count = _typed_manim_observation(
        value, "pointCount", "queryMobjectPointCount", handle_property=True
    )
    if count is None:
        raise RuntimeError("point counts require the shared Rust authoring host")
    return int(count)


def all_points(value):
    array = _anchor_observation(value, _point_array_observation)
    if array is None:
        raise NotImplementedError("get_all_points requires a valid shared Rust handle")
    return _format_point_array(array)


def merged_array(value, array_attr):
    """Return Rust-owned points in Manim's recursive pre-order family profile."""
    if array_attr != "points":
        raise NotImplementedError(
            "get_merged_array currently supports only the shared Rust points array"
        )
    from _manim_updaters import canonical_callback_phase_active

    if canonical_callback_phase_active():
        raise NotImplementedError(
            "merged-array queries are unavailable during staged callbacks"
        )

    return all_points(value)


def _format_point_array(array):
    coordinates, dimensions = array[0], array[1]
    shape = tuple(int(dimensions[index]) for index in range(len(dimensions)))
    try:
        import numpy as np
    except ImportError:
        return _format_points(coordinates, dimensions=3)
    # JS typed arrays need scalar indexing; array slices are not required.
    values = [float(coordinates[index]) for index in range(len(coordinates))]
    return np.asarray(values, dtype=np.float64).reshape(shape)


def reduce_across_dimension(value, reduce_func, dim):
    """Reduce Rust-owned point snapshots in Manim's recursive family order."""

    assert dim >= 0
    assert dim <= 2
    from _manim_updaters import canonical_callback_phase_active

    if canonical_callback_phase_active():
        raise NotImplementedError(
            "dimension reduction queries are unavailable during staged callbacks"
        )

    tree = _point_reduction_tree_observation(value)
    if tree is None:
        query = _query(value)
        try:
            tree = (engine_call(query.points3D), ())
        finally:
            query.free()
    return _reduce_point_reduction_tree(tree, reduce_func, dim)


def _reduce_point_reduction_tree(tree, reduce_func, dim):
    """Invoke arbitrary Python reducers over a Rust-supplied semantic tree."""
    coordinates = [float(value) for value in tree[0]]
    if len(coordinates) % 3:
        raise RuntimeError("point reduction tree returned an incomplete XYZ point")

    try:
        import numpy as np
    except ModuleNotFoundError as error:
        if error.name != "numpy":
            raise
        np = None

    result = None
    if coordinates:
        if np is not None:
            points = np.asarray(coordinates, dtype=np.float64).reshape((-1, 3))
            values = points[:, dim]
        else:
            values = [coordinates[index + dim] for index in range(0, len(coordinates), 3)]
        result = reduce_func(values)

    children = tree[1]
    for index in range(len(children)):
        child_value = _reduce_point_reduction_tree(children[index], reduce_func, dim)
        if result is None:
            result = child_value
        elif child_value is not None:
            result = reduce_func([child_value, result])
    return result


def _default_vector_anchor_hooks(value):
    # Inspect language hooks without invoking arbitrary descriptors. Geometry,
    # semantic membership and anchor order remain Rust-owned.
    from inspect import getattr_static
    from _manim_compat import VMobject

    anchors = getattr_static(value, "get_anchors", None)
    if anchors is None:
        return getattr_static(value, "_semantic_family_handle", None) is not None
    if any(getattr_static(value, name, None) is not getattr(VMobject, name)
           for name in ("get_anchors", "get_start_anchors", "get_end_anchors")):
        return False
    stride = getattr_static(value, "n_points_per_cubic_curve", 4)
    return type(stride) is int and stride == 4


def _anchor_observation(value, observe):
    from _noon_errors import NoonValueError
    try:
        return observe(value)
    except NoonValueError as error:
        if error.code not in ("path.unpaired_anchors", "point.array_shape"):
            raise
        # Rust owns array admission; Python supplies CE's public class.
        raise ValueError(str(error)) from error


def _boundary_snapshot(value, observe):
    coordinates = _anchor_observation(value, observe)
    if coordinates is None:
        raise NotImplementedError("boundary points require a valid shared Rust handle")
    result = _format_points(coordinates, 3)
    return result.reshape((0,)) if len(coordinates) == 0 and hasattr(result, "reshape") else result


def _compat_point3(point):
    if isinstance(point, _base.Vec2):
        from _manim_compat import _manim_point3
        return _manim_point3(point)
    return point


def boundary_points(value):
    try:
        integer_stride = operator.index(
            getattr(value, "n_points_per_cubic_curve", 4)
        )
    except TypeError:
        integer_stride = None
    # Resolve existing wrappers only to identify caller-owned Python hooks.
    # Default retained families use one shared Rust vector-anchor observation.
    has_family_handle = getattr(value, "_semantic_family_handle", None) is not None
    if integer_stride != 4 or has_family_handle:
        family = value.get_family()
        from inspect import getattr_static

        if (has_family_handle
                and getattr_static(value, "get_family", None) is _base.Mobject.get_family
                and all(_default_vector_anchor_hooks(member) for member in family)):
            return _boundary_snapshot(value, _vector_boundary_points_observation)

        def member_anchors(member):
            get_anchors = getattr(member, "get_anchors", None)
            return () if get_anchors is None else get_anchors()

        anchors = [_compat_point3(point)
                   for member in family
                   for point in member_anchors(member)]
        try:
            import numpy as np
        except ImportError:
            return anchors
        result = np.asarray(anchors, dtype=np.float64)
        # The pinned VMobject implementation calls np.array(tuple(...))
        # without forcing a second dimension, so an empty VGroup returns (0,).
        return result
    return _boundary_snapshot(value, _path_boundary_points_observation)


def consider_points_equals(value, p0, p1):
    """Adapt NumPy point-array broadcasting, then compare values in Rust."""
    try:
        import numpy as np
    except ImportError:
        left = [float(component) for component in p0]
        right = [float(component) for component in p1]
        if len(left) != len(right):
            raise ValueError("point arrays must have equal lengths when NumPy is unavailable")
    else:
        left_array, right_array = np.broadcast_arrays(np.asarray(p0), np.asarray(p1))
        left = left_array.astype(np.float64, copy=False).reshape(-1).tolist()
        right = right_array.astype(np.float64, copy=False).reshape(-1).tolist()
    handle = _handle_for(value)
    if handle is None:
        raise RuntimeError("point equality requires a shared Rust Mobject handle")
    tolerance = float(getattr(value, "tolerance_for_point_equality", 1e-6))
    return bool(engine_call(handle.pointsAllClose, left, right, tolerance))


def consider_points_equals_2d(value, p0, p1):
    left = [float(p0[0]), float(p0[1])]
    right = [float(p1[0]), float(p1[1])]
    handle = _handle_for(value)
    if handle is None:
        raise RuntimeError("point equality requires a shared Rust Mobject handle")
    tolerance = float(getattr(value, "tolerance_for_point_equality", 1e-6))
    return bool(engine_call(handle.points2dClose, left, right, tolerance))


def _subpath_range_indices(value, points, n_dims, *, compare_2d, drop_short_ranges):
    if n_dims is not None:
        n_dims = operator.index(n_dims)
    try:
        import numpy as np
    except ImportError:
        np = None
        try:
            rows = [list(row) for row in points]
        except (TypeError, ValueError) as error:
            raise ValueError("points must be a two-dimensional point array") from error
        point_count = len(rows)
        point_dimensions = len(rows[0]) if rows else 0
        if any(len(row) != point_dimensions for row in rows):
            raise ValueError("point rows must have a consistent number of dimensions")
        coordinates = [float(coordinate) for row in rows for coordinate in row]
    else:
        array = np.asarray(points, dtype=np.float64)
        if array.ndim != 2:
            raise ValueError("points must be a two-dimensional point array")
        point_count, point_dimensions = array.shape
        coordinates = array.reshape(-1).tolist()

    if point_count > 0xFFFFFFFF or point_dimensions > 0xFFFFFFFF:
        raise ValueError("points exceed the supported array size")
    if compare_2d:
        compared_dimensions = 2
    elif n_dims is None:
        compared_dimensions = point_dimensions
    else:
        compared_dimensions = slice(None, n_dims).indices(point_dimensions)[1]
    points_per_curve = operator.index(
        getattr(value, "n_points_per_cubic_curve", 4)
    )
    if points_per_curve == 0:
        # Preserve the source's zero-step error before calling the Rust query.
        range(0, point_count, points_per_curve)
    try:
        stride_number = float(points_per_curve)
    except OverflowError:
        stride_number = float("inf") if points_per_curve > 0 else float("-inf")
    handle = _handle_for(value)
    if handle is None:
        raise RuntimeError("subpath queries require a shared Rust Mobject handle")
    tolerance = float(getattr(value, "tolerance_for_point_equality", 1e-6))
    indices = engine_call(
        handle.subpathRanges,
        coordinates,
        point_count,
        point_dimensions,
        compared_dimensions,
        stride_number,
        tolerance,
        drop_short_ranges,
    )
    indices = [int(index) for index in indices]
    if len(indices) % 2:
        raise RuntimeError("Rust subpath query returned an incomplete index range")
    return np, indices


def subpath_split_indices(value, points, n_dims=3):
    """Return Manim's [start, end) point ranges for connected cubic runs."""
    np, indices = _subpath_range_indices(
        value,
        points,
        n_dims,
        compare_2d=False,
        drop_short_ranges=False,
    )
    if np is None:
        return [indices[index:index + 2] for index in range(0, len(indices), 2)]
    return np.asarray(indices, dtype=np.int_).reshape((-1, 2))


def subpaths_from_points(value, points):
    """Return Manim's non-short connected point slices as a list."""
    _, indices = _subpath_range_indices(
        value,
        points,
        None,
        compare_2d=False,
        drop_short_ranges=True,
    )
    return [
        points[indices[index]:indices[index + 1]]
        for index in range(0, len(indices), 2)
    ]


def gen_subpaths_from_points_2d(value, points):
    """Yield connected point slices using ManimCE's planar anchor comparison."""
    _, indices = _subpath_range_indices(
        value,
        points,
        None,
        compare_2d=True,
        drop_short_ranges=True,
    )
    return (
        points[indices[index]:indices[index + 1]]
        for index in range(0, len(indices), 2)
    )


def gen_cubic_bezier_tuples_from_points(value, points):
    """Group complete point strides using Rust-computed slice boundaries."""
    point_count = len(points)
    indices = _cubic_bezier_point_ranges(value, point_count)
    return tuple(
        points[indices[index]:indices[index + 1]]
        for index in range(0, len(indices), 2)
    )


def cubic_bezier_tuples_from_points(value, points):
    """Return the NumPy array built from ManimCE's complete cubic point groups."""
    groups = gen_cubic_bezier_tuples_from_points(value, points)
    try:
        import numpy as np
    except ImportError:
        return [list(group) for group in groups]
    return np.array(groups)


def cubic_bezier_tuples(value):
    """Group the object's current Rust-owned point snapshot by cubic stride."""
    query = _query(value)
    try:
        coordinates = engine_call(query.points)
    finally:
        query.free()
    return cubic_bezier_tuples_from_points(value, _format_points(coordinates))


def _format_points(coordinates, dimensions=2):
    try:
        import numpy as np
    except ImportError:
        if dimensions == 2:
            return _points(coordinates)
        # Pyodide typed arrays support scalar indexing but not Python slices.
        return [tuple(float(coordinates[index + offset]) for offset in range(dimensions))
                for index in range(0, len(coordinates), dimensions)]
    return np.asarray(coordinates, dtype=np.float64).reshape((-1, dimensions))


def center_of_mass(value):
    coordinates = _center_of_mass_observation(value)
    if coordinates is None:
        raise NotImplementedError("get_center_of_mass requires a valid shared Rust handle")
    return _base.Vec2(float(coordinates[0]), float(coordinates[1]))


def _point_direction_xy(direction):
    try:
        dimensions = len(direction)
    except (TypeError, AttributeError):
        return _base._as_vec2(direction)
    if dimensions == 2:
        return _base._as_vec2(direction)
    if dimensions == 3:
        return _base.Vec2(float(direction[0]), float(direction[1]))
    raise TypeError("expected a two- or three-component vector")


def boundary_point(value, direction):
    axis = _point_direction_xy(direction)
    coordinates = _boundary_point_observation(value, axis.x, axis.y)
    if coordinates is None:
        raise NotImplementedError("get_boundary_point requires a valid shared Rust handle")
    return _base.Vec2(float(coordinates[0]), float(coordinates[1]))


def length_over_dim(value, dim):
    dim = _normalize_point_dimension(dim, 3)
    result = _length_over_dim_observation(value, dim)
    if result is None:
        raise NotImplementedError("length_over_dim requires a valid shared Rust handle")
    return float(result)


def _normalize_point_dimension(dim, point_dimensions):
    dim = operator.index(dim)
    normalized = dim if dim >= 0 else point_dimensions + dim
    if not 0 <= normalized < point_dimensions:
        raise IndexError(
            f"index {dim} is out of bounds for axis 1 with size {point_dimensions}"
        )
    return normalized


def _plain_direction_keys(direction, np):
    # Restrict the direct path to scalar containers without caller-defined
    # indexing/coercion hooks. Other directions retain CE's exact access order.
    if type(direction) is _base.Vec2:
        return (*direction, 0.0)
    if type(direction) is _base._Direction3:
        return tuple(direction)
    if np is not None and type(direction) is np.ndarray:
        if direction.shape == (3,) and direction.dtype.kind in "fiu":
            return tuple(float(direction[index]) for index in range(3))
        return None
    if type(direction) in (tuple, list) and len(direction) == 3:
        types = (int, float, bool) if np is None else (int, float, bool, np.float64)
        if all(type(component) in types for component in direction):
            return tuple(float(component) for component in direction)
    return None


def _default_critical_query(value):
    if type(value).__getattribute__ is not object.__getattribute__:
        return False
    if not _default_point_query_hooks(value):
        return False
    from inspect import getattr_static
    from _manim_compat import VMobject
    members = value.get_family()
    if isinstance(value, VMobject):
        return all(_default_vector_anchor_hooks(member) for member in members)
    return all(getattr_static(member, "get_all_points", None) is _base.Mobject.get_all_points
               and type(member).__getattribute__ is object.__getattribute__ for member in members)


def critical_point(value, direction):
    """Use one Rust critical-point query, preserving caller-owned Python hooks."""
    dimensions = getattr(value, "dim", 3)
    try:
        import numpy as np
    except ImportError:
        np = None
    keys = _plain_direction_keys(direction, np)
    if type(dimensions) is int and dimensions == 3 and keys is not None and _default_critical_query(value):
        from _manim_compat import VMobject
        coordinates = _anchor_observation(
            value, lambda target: _critical_point_observation(target, keys, isinstance(value, VMobject)))
        if coordinates is None:
            raise NotImplementedError("critical points require a valid shared Rust handle")
        result = [float(coordinates[index]) for index in range(3)]
        return result if np is None else np.asarray(result, dtype=np.float64)

    result = [0.0] * dimensions if np is None else np.zeros(dimensions)
    points = value.get_points_defining_boundary()
    if len(points) == 0:
        return result
    keys = (*direction, 0.0) if isinstance(direction, _base.Vec2) else direction
    for dim in range(dimensions):
        result[dim] = value.get_extremum_along_dim(points, dim=dim, key=keys[dim])
    return result


def extremum_along_dim(value, points=None, dim=0, key=0):
    dim = operator.index(dim)
    key = float(key)
    if points is None:
        points = value.get_points_defining_boundary()
    try:
        import numpy as np
    except ImportError:
        np = None
        rows = [list(row) for row in points]
        if not rows:
            raise IndexError("too many indices for array: array is 1-dimensional, but 2 were indexed")
        point_dimensions = len(rows[0])
        if any(len(row) != point_dimensions for row in rows):
            raise ValueError("point rows must have a consistent number of dimensions")
        dim = _normalize_point_dimension(dim, point_dimensions)
        coordinates = [float(coordinate) for row in rows for coordinate in row]
    else:
        array = np.asarray(points)
        # Indexing preserves NumPy's shape/errors. Rust owns every nonempty
        # min/max/midpoint reduction; no Python bounds state is maintained.
        selected = array[:, dim]
        coordinates = selected.astype(np.float64, copy=False).reshape(-1).tolist()
        point_dimensions = 1
        dim = 0
        if len(coordinates) == 0:
            operation = "maximum" if key > 0 else "minimum"
            raise ValueError(f"zero-size array to reduction operation {operation} which has no identity")

    result = _extremum_from_points_observation(
        value, coordinates, point_dimensions, dim, key,
    )
    if result is None:
        raise NotImplementedError("get_extremum_along_dim requires a valid shared Rust handle")
    return float(result) if np is None else np.float64(result)


def has_new_path_started(value):
    stride = operator.index(getattr(value, "n_points_per_cubic_curve", 4))
    if not -(1 << 31) <= stride < (1 << 31):
        raise ValueError("n_points_per_cubic_curve exceeds the supported path stride")
    if stride == 0:
        raise ZeroDivisionError("integer modulo by zero")
    query = _query(value)
    try:
        arguments = () if stride == 4 else (stride,)
        return bool(engine_call(query.hasNewPathStarted, *arguments))
    finally:
        query.free()


def curve_points(value, n):
    # Match VMobject.get_nth_curve_points(): it only asserts the upper bound,
    # then uses Python slicing over the stored point array. In particular,
    # negative indices retain the source's slice behavior.
    assert n < value.get_num_curves()
    stride = getattr(value, "n_points_per_cubic_curve", 4)
    query = _query(value)
    try:
        points = _points(engine_call(query.points))
        selected = points[stride * n : stride * (n + 1)]
        return _points_array(selected)
    finally:
        query.free()


def _curve_sample_count(sample_points):
    if sample_points is None:
        return None
    samples = operator.index(sample_points)
    if samples < 0:
        raise ValueError(f"Number of samples, {samples}, must be non-negative.")
    if samples > 0xFFFFFFFF:
        raise ValueError("sample_points must be between 0 and 4294967295")
    return samples


def _raise_zero_sample_length_axis_error():
    """Match NumPy's empty one-dimensional norm failure in Manim's sampler."""
    import numpy as np

    axis_error = getattr(np, "AxisError", None)
    if axis_error is None:
        axis_error = np.exceptions.AxisError
    raise axis_error(1, ndim=1)


def _nth_curve_selection(value, index, query):
    """Resolve Manim's Python point slice to a Rust cubic or arbitrary range."""
    stride = getattr(value, "n_points_per_cubic_curve", 4)
    point_count = int(query.pointCount)
    curve_count = point_count // stride
    assert index < curve_count
    start, stop, _ = slice(stride * index, stride * (index + 1)).indices(
        point_count
    )
    if start >= stop:
        return None
    try:
        integer_stride = operator.index(stride)
    except TypeError:
        integer_stride = None
    if integer_stride == 4 and stop - start == 4 and start % 4 == 0:
        return ("cubic", start // 4)
    return ("range", start, stop)


class _CurveQuerySnapshot:
    """Keep one immutable Rust path observation alive for returned callables."""

    __slots__ = ("query",)

    def __init__(self, query):
        self.query = query

    def __del__(self):
        query = getattr(self, "query", None)
        if query is not None:
            self.query = None
            try:
                query.free()
            except Exception:
                # Finalizers must not surface errors during interpreter teardown.
                pass


def _curve_callable(snapshot, index):
    def curve(t):
        import numpy as np

        parameters = np.asarray(t, dtype=np.float64)
        flat = parameters.reshape(-1).tolist()
        coordinates = engine_call(
            snapshot.query.curvePointsAt, index, flat
        )
        points = np.asarray(coordinates, dtype=np.float64).reshape((-1, 3))
        if parameters.ndim == 0:
            return points[0]
        if parameters.ndim == 2 and parameters.shape[1] == 1:
            return points.reshape((parameters.shape[0], 3))
        return points.reshape((*parameters.shape, 3))

    curve.__name__ = "cubic_bezier"
    return curve


def _bezier_curve_name(point_count):
    return {
        1: "zero_bezier",
        2: "linear_bezier",
        3: "quadratic_bezier",
        4: "cubic_bezier",
    }.get(point_count, "nth_grade_bezier")


def _range_curve_callable(snapshot, start, stop):
    def curve(t):
        import numpy as np

        parameters = np.asarray(t, dtype=np.float64)
        flat = parameters.reshape(-1).tolist()
        coordinates = engine_call(
            snapshot.query.bezierPointsAtRange, start, stop, flat
        )
        points = np.asarray(coordinates, dtype=np.float64).reshape((-1, 3))
        if parameters.ndim == 0:
            return points[0]
        if stop - start > 4:
            # Manim's nth_grade_bezier flattens every array input to one
            # leading sample dimension, even when the input is multidimensional.
            return points
        if parameters.ndim == 2 and parameters.shape[1] == 1:
            return points.reshape((parameters.shape[0], 3))
        return points.reshape((*parameters.shape, 3))

    curve.__name__ = _bezier_curve_name(stop - start)
    return curve


def _selected_curve_callable(snapshot, selection):
    if selection[0] == "cubic":
        return _curve_callable(snapshot, selection[1])
    return _range_curve_callable(snapshot, selection[1], selection[2])


def _curve_length_from_selection(query, selection, samples):
    if selection[0] == "cubic":
        return float(engine_call(query.curveLength, selection[1], samples))
    pieces = engine_call(
        query.bezierLengthPiecesRange,
        selection[1],
        selection[2],
        samples,
    )
    return float(sum(pieces))


def _empty_curve_callable():
    def curve(_t):
        raise IndexError("index 0 is out of bounds for axis 1 with size 0")

    curve.__name__ = "nth_grade_bezier"
    return curve


def _curve_functions(value, with_lengths, sample_points=None, extra_kwargs=None):
    query = _query(value)
    snapshot = _CurveQuerySnapshot(query)
    # Yielded callables share this owner. Generator failure must not release
    # the query while any previously returned callable still needs it.
    point_count = int(query.pointCount)
    count = point_count // getattr(value, "n_points_per_cubic_curve", 4)
    curve_indices = range(count)
    if with_lengths and count > 0 and extra_kwargs:
        unexpected = set(extra_kwargs) - {"sample_points"}
        if unexpected:
            # Match Manim's lazy per-curve dispatch, including subclass
            # overrides and Python's bound-method TypeError text.
            for index in curve_indices:
                yield value.get_nth_curve_function_with_length(
                    index, **extra_kwargs
                )
            return
    samples = (
        _curve_sample_count(sample_points)
        if with_lengths and count > 0
        else None
    )
    if with_lengths and count > 0 and samples == 0:
        _raise_zero_sample_length_axis_error()
    for index in curve_indices:
        selection = _nth_curve_selection(value, index, query)
        function = (
            _empty_curve_callable()
            if selection is None
            else _selected_curve_callable(snapshot, selection)
        )
        if with_lengths:
            import numpy as np

            if selection is None:
                raise IndexError("index 0 is out of bounds for axis 1 with size 0")
            length = np.float64(
                _curve_length_from_selection(query, selection, samples)
            )
            yield function, length
        else:
            yield function


def curve_function(value, n):
    query = _query(value)
    snapshot = _CurveQuerySnapshot(query)
    try:
        selection = _nth_curve_selection(value, n, query)
        if selection is None:
            snapshot.__del__()
            return _empty_curve_callable()
        return _selected_curve_callable(snapshot, selection)
    except Exception:
        snapshot.__del__()
        raise


def curve_function_with_length(value, n, sample_points=None):
    import numpy as np

    query = _query(value)
    snapshot = _CurveQuerySnapshot(query)
    try:
        selection = _nth_curve_selection(value, n, query)
        samples = _curve_sample_count(sample_points)
        if samples == 0:
            _raise_zero_sample_length_axis_error()
        if selection is None:
            raise IndexError("index 0 is out of bounds for axis 1 with size 0")
        function = _selected_curve_callable(snapshot, selection)
        length = np.float64(
            _curve_length_from_selection(query, selection, samples)
        )
        return function, length
    except Exception:
        snapshot.__del__()
        raise


def curve_functions(value):
    return _curve_functions(value, with_lengths=False)


def curve_functions_with_lengths(value, **kwargs):
    yield from _curve_functions(
        value,
        with_lengths=True,
        sample_points=kwargs.get("sample_points"),
        extra_kwargs=kwargs,
    )


def path_direction(value):
    stride_value = getattr(value, "n_points_per_cubic_curve", 4)
    if stride_value is None:
        stride = 1
    else:
        try:
            stride = operator.index(stride_value)
        except TypeError:
            # Preserve the exact Python slice validation used by
            # get_start_anchors(), including its native TypeError message.
            slice(None, None, stride_value).indices(0)
            raise AssertionError("invalid slice step unexpectedly succeeded")
    if stride == 0:
        # NumPy raises before Manim reaches the shoelace reduction.
        slice(None, None, stride).indices(0)
    if not -(1 << 31) <= stride < (1 << 31):
        raise ValueError("n_points_per_cubic_curve exceeds the supported path stride")
    query = _query(value)
    try:
        if stride == 4:
            return str(engine_call(query.direction))
        return str(engine_call(query.directionWithStride, stride))
    finally:
        query.free()


def curve_length_pieces(value, n, sample_points):
    import numpy as np

    query = _query(value)
    try:
        selection = _nth_curve_selection(value, n, query)
        samples = _curve_sample_count(sample_points)
        if samples == 0:
            _raise_zero_sample_length_axis_error()
        if selection is None:
            raise IndexError("index 0 is out of bounds for axis 1 with size 0")
        if selection[0] == "cubic":
            pieces = engine_call(
                query.curveLengthPieces, selection[1], samples
            )
        else:
            pieces = engine_call(
                query.bezierLengthPiecesRange,
                selection[1],
                selection[2],
                samples,
            )
        return np.asarray(
            pieces, dtype=np.float64
        )
    finally:
        query.free()


def curve_length(value, n, sample_points):
    import numpy as np

    query = _query(value)
    try:
        selection = _nth_curve_selection(value, n, query)
        samples = _curve_sample_count(sample_points)
        if samples == 0:
            _raise_zero_sample_length_axis_error()
        if selection is None:
            raise IndexError("index 0 is out of bounds for axis 1 with size 0")
        return np.float64(_curve_length_from_selection(query, selection, samples))
    finally:
        query.free()


def path_points(value, method):
    query = _query(value)
    try:
        # Manim returns its single point as an array before inspecting stride.
        if method == "anchors" and query.pointCount == 1:
            return _boundary_snapshot(query, lambda captured: engine_call(captured.anchors3D))
        stride = getattr(value, "n_points_per_cubic_curve", 4)
        try:
            integer_stride = operator.index(stride)
        except TypeError:
            integer_stride = None
        if method in ("startAnchors", "endAnchors", "anchors") and integer_stride != 4:
            points = _format_points(engine_call(query.points3D), 3)
            starts = points[::stride]
            if method == "startAnchors":
                return starts
            ends = points[stride - 1 :: stride]
            if method == "endAnchors":
                return ends
            return [point for pair in zip(starts, ends, strict=True) for point in pair]
        if method == "anchors":
            return list(_boundary_snapshot(query, lambda captured: engine_call(captured.anchors3D)))
        return _format_points(engine_call(getattr(query, method + "3D")), 3)
    finally:
        query.free()


def anchors_and_handles(value):
    stride = operator.index(getattr(value, "n_points_per_cubic_curve", 4))
    if stride > 0xFFFFFFFF:
        raise ValueError("n_points_per_cubic_curve exceeds the supported array size")
    if stride <= 0:
        return []
    if stride != 4:
        query = _query(value)
        try:
            points = _format_points(engine_call(query.points3D), 3)
            return [points[index::stride] for index in range(stride)]
        finally:
            query.free()
    query = _query(value)
    try:
        return [
            _format_points(engine_call(getattr(query, method + "3D")), 3)
            for method in ("startAnchors", "firstHandles", "secondHandles", "endAnchors")
        ]
    finally:
        query.free()


def subpaths(value):
    stride = getattr(value, "n_points_per_cubic_curve", 4)
    try:
        integer_stride = operator.index(stride)
    except TypeError:
        integer_stride = None
    if integer_stride != 4:
        query = _query(value)
        try:
            points = _points(engine_call(query.points))
            return [
                _points_array(points)
                for points in subpaths_from_points(value, points)
            ]
        finally:
            query.free()
    query = _query(value)
    try:
        return [
            _points_array(_points(points))
            for points in engine_call(query.subpaths)
        ]
    finally:
        query.free()


def is_closed(value):
    query = _query(value)
    try:
        if int(query.pointCount) == 0:
            raise IndexError("index 0 is out of bounds for axis 0 with size 0")
        tolerance = float(getattr(value, "tolerance_for_point_equality", 1e-6))
        return bool(engine_call(query.isClosed, tolerance))
    finally:
        query.free()


def vertex_groups(value):
    query = _query(value)
    try:
        return [_points(points) for points in engine_call(query.vertexGroups)]
    finally:
        query.free()
