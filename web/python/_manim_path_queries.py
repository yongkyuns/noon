"""Argument/result adaptation for immutable shared Rust path observations."""
import operator
import noon as _base
import _noon_ir as _ir
from _noon_errors import engine_call
from _manim_semantic_handles import (
    _all_points_observation,
    _boundary_point_observation,
    _center_of_mass_observation,
    _extremum_along_dim_observation,
    _extremum_from_points_observation,
    _family_point_count_observation,
    _length_over_dim_observation,
    _path_boundary_points_observation,
    _typed_manim_observation,
    _handle_for,
)


def _query(value):
    query = _typed_manim_observation(value, "pathQuery", "queryMobjectPath")
    if query is None:
        raise RuntimeError("path queries require the shared Rust authoring host")
    return query


def point_from_proportion(value, alpha):
    alpha = _ir._finite_number("alpha", alpha)
    query = _query(value)
    try:
        coordinates = engine_call(query.pointFromProportion, alpha)
        return _base.Vec2(float(coordinates[0]), float(coordinates[1]))
    finally:
        query.free()


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
        proportion = engine_call(query.proportionFromPoint, *coordinates)
        if proportion is None:
            raise ValueError(f"Point {point} does not lie on this curve.")
        return float(proportion)
    finally:
        query.free()


def arc_length(value, sample_points_per_curve):
    query = _query(value)
    try:
        # Manim only reaches np.linspace while iterating actual curves. Empty
        # paths therefore return zero without validating this otherwise-unused
        # argument.
        if int(query.curveCount) == 0:
            return 0.0
        samples = (
            None
            if sample_points_per_curve is None
            else operator.index(sample_points_per_curve)
        )
        if samples is not None and samples < 0:
            raise ValueError(f"Number of samples, {samples}, must be non-negative.")
        if samples is not None and samples > 0xFFFFFFFF:
            raise ValueError("sample_points_per_curve must be between 0 and 4294967295")
        if samples == 0:
            _raise_zero_sample_length_axis_error()
        return float(engine_call(query.arcLength, samples))
    finally:
        query.free()


def endpoint(value, end):
    query = _query(value)
    try:
        coordinates = engine_call(query.end if end else query.start)
        return _base.Vec2(float(coordinates[0]), float(coordinates[1]))
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


def curve_count(value):
    query = _query(value)
    try:
        return int(query.curveCount)
    finally:
        query.free()


def point_count(value):
    family_count = _family_point_count_observation(value)
    if family_count is not None:
        return family_count
    query = _query(value)
    try:
        return int(query.pointCount)
    finally:
        query.free()


def all_points(value):
    coordinates = _all_points_observation(value)
    if coordinates is None:
        raise NotImplementedError("get_all_points requires a valid shared Rust handle")
    return _format_points(coordinates)


def boundary_points(value):
    coordinates = _path_boundary_points_observation(value)
    if coordinates is None:
        raise NotImplementedError(
            "get_points_defining_boundary requires a valid shared Rust handle"
        )
    return _format_points(coordinates)


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
    points_per_curve = operator.index(
        getattr(value, "n_points_per_cubic_curve", 4)
    )
    point_count = len(points)
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


def _format_points(coordinates):
    try:
        import numpy as np
    except ImportError:
        return _points(coordinates)
    return np.asarray(coordinates, dtype=np.float64).reshape((-1, 2))


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


def extremum_along_dim(value, points=None, dim=0, key=0):
    dim = operator.index(dim)
    key = float(key)
    if points is None:
        dim = _normalize_point_dimension(dim, 3)
        result = _extremum_along_dim_observation(value, dim, key)
        if result is None:
            raise NotImplementedError("get_extremum_along_dim requires a valid shared Rust handle")
        return float(result)

    try:
        import numpy as np
    except ImportError:
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
        # Indexing only shapes the input like Manim's NumPy path; Rust performs
        # the min/max/midpoint reduction over the selected coordinate values.
        selected = array[:, dim]
        coordinates = selected.astype(np.float64, copy=False).reshape(-1).tolist()
        point_dimensions = 1
        dim = 0

    result = _extremum_from_points_observation(
        value,
        coordinates,
        point_dimensions,
        dim,
        key,
    )
    if result is None:
        raise NotImplementedError("get_extremum_along_dim requires a valid shared Rust handle")
    return float(result)


def has_new_path_started(value):
    query = _query(value)
    try:
        return bool(engine_call(query.hasNewPathStarted))
    finally:
        query.free()


def curve_points(value, n):
    index = operator.index(n)
    if not 0 <= index <= 0xFFFFFFFF:
        raise IndexError("curve index must be between 0 and 4294967295")
    query = _query(value)
    try:
        return _points(engine_call(query.curvePoints, index))
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


def _assert_curve_index(query, index):
    if index < 0:
        raise IndexError("curve index must be non-negative")
    if index >= int(query.curveCount):
        raise AssertionError


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


def _curve_functions(value, with_lengths, sample_points=None):
    import numpy as np

    query = _query(value)
    snapshot = _CurveQuerySnapshot(query)
    try:
        count = int(query.curveCount)
        samples = _curve_sample_count(sample_points) if with_lengths and count else None
        if with_lengths and count and samples == 0:
            _raise_zero_sample_length_axis_error()
        for index in range(count):
            function = _curve_callable(snapshot, index)
            if with_lengths:
                length = np.float64(engine_call(query.curveLength, index, samples))
                yield function, length
            else:
                yield function
    except Exception:
        snapshot.__del__()
        raise


def curve_function(value, n):
    index = operator.index(n)
    query = _query(value)
    snapshot = _CurveQuerySnapshot(query)
    try:
        _assert_curve_index(query, index)
        return _curve_callable(snapshot, index)
    except Exception:
        snapshot.__del__()
        raise


def curve_function_with_length(value, n, sample_points=None):
    import numpy as np

    index = operator.index(n)
    query = _query(value)
    snapshot = _CurveQuerySnapshot(query)
    try:
        _assert_curve_index(query, index)
        samples = _curve_sample_count(sample_points)
        if samples == 0:
            _raise_zero_sample_length_axis_error()
        function = _curve_callable(snapshot, index)
        length = np.float64(engine_call(query.curveLength, index, samples))
        return function, length
    except Exception:
        snapshot.__del__()
        raise


def curve_functions(value):
    return _curve_functions(value, with_lengths=False)


def curve_functions_with_lengths(value, **kwargs):
    unexpected = set(kwargs) - {"sample_points"}
    if unexpected:
        name = sorted(unexpected)[0]
        raise TypeError(f"unexpected keyword argument: {name!r}")
    yield from _curve_functions(
        value, with_lengths=True, sample_points=kwargs.get("sample_points")
    )


def path_direction(value):
    query = _query(value)
    try:
        return str(engine_call(query.direction))
    finally:
        query.free()


def curve_length_pieces(value, n, sample_points):
    import numpy as np

    index = operator.index(n)
    if index < 0:
        raise IndexError("curve index must be between 0 and 4294967295")
    query = _query(value)
    try:
        _assert_curve_index(query, index)
        samples = _curve_sample_count(sample_points)
        if samples == 0:
            _raise_zero_sample_length_axis_error()
        return np.asarray(
            engine_call(query.curveLengthPieces, index, samples), dtype=np.float64
        )
    finally:
        query.free()


def curve_length(value, n, sample_points):
    import numpy as np

    index = operator.index(n)
    if index < 0:
        raise IndexError("curve index must be between 0 and 4294967295")
    query = _query(value)
    try:
        _assert_curve_index(query, index)
        samples = _curve_sample_count(sample_points)
        if samples == 0:
            _raise_zero_sample_length_axis_error()
        return np.float64(engine_call(query.curveLength, index, samples))
    finally:
        query.free()


def path_points(value, method):
    query = _query(value)
    try:
        return _points(engine_call(getattr(query, method)))
    finally:
        query.free()


def anchors_and_handles(value):
    query = _query(value)
    try:
        return [_points(engine_call(getattr(query, method))) for method in
                ("startAnchors", "firstHandles", "secondHandles", "endAnchors")]
    finally:
        query.free()


def subpaths(value):
    query = _query(value)
    try:
        return [_points(points) for points in engine_call(query.subpaths)]
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
