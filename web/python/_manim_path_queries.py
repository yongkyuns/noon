"""Argument/result adaptation for immutable shared Rust path observations."""
import operator
import noon as _base
import _noon_ir as _ir
from _noon_errors import engine_call
from _manim_semantic_handles import _typed_manim_observation


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


def arc_length(value, sample_points_per_curve):
    samples = None if sample_points_per_curve is None else operator.index(sample_points_per_curve)
    if samples is not None and not 2 <= samples <= 0xFFFFFFFF:
        raise ValueError("sample_points_per_curve must be between 2 and 4294967295")
    query = _query(value)
    try:
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


def _points(coordinates):
    return [_base.Vec2(float(coordinates[i]), float(coordinates[i + 1])) for i in range(0, len(coordinates), 2)]


def curve_count(value):
    query = _query(value)
    try:
        return int(query.curveCount)
    finally:
        query.free()


def point_count(value):
    query = _query(value)
    try:
        return int(query.pointCount)
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

    samples = _curve_sample_count(sample_points) if with_lengths else None
    query = _query(value)
    snapshot = _CurveQuerySnapshot(query)
    try:
        count = int(query.curveCount)
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
    samples = _curve_sample_count(sample_points)
    query = _query(value)
    snapshot = _CurveQuerySnapshot(query)
    try:
        _assert_curve_index(query, index)
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
    samples = _curve_sample_count(sample_points)
    query = _query(value)
    try:
        _assert_curve_index(query, index)
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
    samples = _curve_sample_count(sample_points)
    query = _query(value)
    try:
        _assert_curve_index(query, index)
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
        return bool(engine_call(query.isClosed))
    finally:
        query.free()


def vertex_groups(value):
    query = _query(value)
    try:
        return [_points(points) for points in engine_call(query.vertexGroups)]
    finally:
        query.free()
