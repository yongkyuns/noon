"""Linear 2D coordinates and preparation-only plots over shared Rust handles.

The initial coordinate constructors require explicit three-value ranges and do
not support tips, automatic label objects, or construction after execution starts.
Existing coordinates remain queryable after plays; curves use ordinary live
geometry publication. Python owns callables/coercion, never coordinate math.
"""
from __future__ import annotations

from contextlib import contextmanager
from operator import index as _index
from typing import NamedTuple

import noon as _base
import _manim_compat as _compat
import _manim_semantic_handles as _shared
from _noon_errors import engine_call

try:
    from js import noonAuthoringCoordinateOptions as _coordinate_options
    from js import noonCreateAuthoringCoordinateHandle as _create_coordinates
    from js import noonPlotSamplingPlan as _sampling_plan
    from pyodide.ffi import to_js as _to_js
except ImportError:
    _coordinate_options = _create_coordinates = _sampling_plan = _to_js = None


class _NumberLabel(NamedTuple):
    number: float
    text: str
    point: _base.Vec2


class _TimeSeriesPlan(NamedTuple):
    points: tuple[_base.Vec2, ...]
    cursor_points: tuple[_base.Vec2, ...]
    key_times: tuple[float, ...]
    durations: tuple[float, ...]
    run_time: float


class _SynchronizedTimeSeriesPlan(NamedTuple):
    series_points: tuple[tuple[_base.Vec2, ...], ...]
    data_times: tuple[float, ...]
    cursor_points: tuple[_base.Vec2, ...]
    key_times: tuple[float, ...]
    durations: tuple[float, ...]
    run_time: float


class _GappedTimeSeriesPlan(NamedTuple):
    series_points: tuple[tuple[_base.Vec2 | None, ...], ...]
    series_segments: tuple[tuple[tuple[_base.Vec2, _base.Vec2] | None, ...], ...]
    data_times: tuple[float, ...]
    cursor_points: tuple[_base.Vec2, ...]
    key_times: tuple[float, ...]
    durations: tuple[float, ...]
    run_time: float


def _break_index(value):
    if isinstance(value, bool):
        raise TypeError("break-after indices must be integers, not booleans")
    return _index(value)


def _point_pairs(values):
    """Project an already validated Rust vector, without coordinate calculation."""
    return tuple(_base.Vec2(float(values[i]), float(values[i + 1]))
                 for i in range(0, len(values), 2))


@contextmanager
def _owned(value):
    """Release disposable WASM observations/plans, including callback failures."""
    try:
        yield value
    finally:
        value.free()


def _array(values):
    if _to_js is None:
        raise RuntimeError("plotting requires the shared Rust authoring host")
    return _to_js([float(value) for value in values])


def _outside_callback():
    from _manim_updaters import _ACTIVE_CANONICAL_CONTEXT
    if _ACTIVE_CANONICAL_CONTEXT.get() is not None:
        raise NotImplementedError(
            "coordinate queries and plot construction inside callbacks require pinned coordinate reads"
        )


def _coordinate_context(shafts):
    _outside_callback()
    contexts = [context for shaft in shafts
                if (context := _shared._live_mutation_context(shaft)) is not None]
    if not contexts:
        return _shared._live_constructor_context("coordinate query")
    if any(context is not contexts[0] for context in contexts[1:]):
        raise RuntimeError("coordinate shafts belong to different execution contexts")
    return contexts[0]


def _family(wrapper, handle, members):
    wrapper._semantic_family_handle = handle
    wrapper._semantic_member_wrappers = {
        _shared._family_wrapper_key(member): member for member in members
    }
    return wrapper


def _leaf(handle):
    wrapper = object.__new__(_compat.Line)
    _shared._attach_shared_handle(wrapper, handle)
    return wrapper


def _attach_number_line(wrapper, handle):
    shaft = _leaf(engine_call(handle.coordinateShaft))
    ticks = _family(
        object.__new__(_compat.Group),
        engine_call(handle.coordinateTicks),
        [_leaf(tick) for tick in engine_call(handle.coordinateTickObjects)],
    )
    return _family(wrapper, handle, [shaft, ticks])


def _coordinate_style(options, color, kwargs):
    values = dict(kwargs)
    allowed = {"stroke_width", "stroke_opacity", "opacity"}
    unknown = sorted(set(values) - allowed)
    if unknown:
        raise TypeError("unsupported coordinate option(s): " + ", ".join(unknown))
    _shared._apply_shared_constructor_options(options, values)
    _shared._apply_constructor_color(options, color)


def _cold_coordinates():
    _outside_callback()
    if _create_coordinates is None:
        raise RuntimeError("coordinates require the shared Rust authoring host")
    if _shared._live_constructor_context("coordinates") is not None:
        raise NotImplementedError(
            "create axes before the first play/wait; live coordinate-family construction is not yet supported"
        )


class NumberLine(_compat.Group):
    """Explicit-range, tipless linear number line with Rust-owned ticks/range."""

    def __init__(self, x_range, *, length=None, unit_size=1.0, rotation=0.0,
                 include_ticks=True, tick_size=0.1, exclude_origin_tick=False,
                 include_tip=False, color=None, **kwargs):
        _cold_coordinates()
        if include_tip:
            raise NotImplementedError("NumberLine tips are not yet supported")
        options = engine_call(
            _coordinate_options.numberLine, _array(x_range),
            None if length is None else float(length), float(unit_size), float(rotation),
        )
        try:
            engine_call(options.setTicks, bool(include_ticks), float(tick_size), bool(exclude_origin_tick))
            _coordinate_style(options, color, kwargs)
        except BaseException:
            options.free()
            raise
        _attach_number_line(self, engine_call(_create_coordinates, options))

    @property
    def shaft(self):
        return self.submobjects[0]

    @property
    def ticks(self):
        return self.submobjects[1]

    def _coordinate_frame(self):
        context = _coordinate_context([self.shaft])
        if context is not None:
            return engine_call(context.queryNumberLineFrame, self._semantic_family_handle)
        return engine_call(self._semantic_family_handle.numberLineFrame)

    @property
    def x_range(self):
        with _owned(self._coordinate_frame()) as frame:
            return tuple(float(value) for value in engine_call(frame.range))

    def number_to_point(self, number):
        with _owned(self._coordinate_frame()) as frame:
            return _base.Vec2(*engine_call(frame.numberToPoint, float(number)))

    n2p = number_to_point

    def point_to_number(self, point):
        point = _base._as_vec2(point)
        with _owned(self._coordinate_frame()) as frame:
            return float(engine_call(frame.pointToNumber, point.x, point.y))

    p2n = point_to_number

    def get_unit_size(self):
        with _owned(self._coordinate_frame()) as frame:
            return float(engine_call(frame.unitSize))


    def label_plan(self, numbers=None, *, decimal_places=0, exclude_zero=True):
        """Noon extension: immutable text/anchor preparation, not label objects.

        None selects Rust's tick values; an empty iterable selects no labels.
        Place ordinary Text objects with next_to(label.point, ...) and explicitly
        group them with the axes when they should move together. Re-prepare after
        moving the axes; this snapshot is not a live coordinate handle.
        """
        if not isinstance(decimal_places, int) or isinstance(decimal_places, bool):
            raise TypeError("decimal_places must be an integer")
        with _owned(self._coordinate_frame()) as frame:
            with _owned(engine_call(
                frame.numberLabelPlan, _array(() if numbers is None else numbers),
                numbers is None, decimal_places, bool(exclude_zero),
            )) as plan:
                values = tuple(float(x) for x in engine_call(plan.numbers))
                texts = tuple(str(x) for x in engine_call(plan.texts))
                points = _point_pairs(engine_call(plan.points))
                return tuple(_NumberLabel(*entry) for entry in zip(values, texts, points, strict=True))


class Axes(_compat.Group):
    """Explicitly sized, linear 2D axes. Curves are independent retained paths."""

    def __init__(self, x_range, y_range, *, x_length, y_length, tips=False,
                 include_ticks=True, tick_size=0.1, color=None, **kwargs):
        _cold_coordinates()
        if tips:
            raise NotImplementedError("Axes tips are not yet supported")
        options = engine_call(
            _coordinate_options.axes, _array(x_range), _array(y_range),
            float(x_length), float(y_length),
        )
        try:
            engine_call(options.setTicks, bool(include_ticks), float(tick_size), True)
            _coordinate_style(options, color, kwargs)
        except BaseException:
            options.free()
            raise
        handle = engine_call(_create_coordinates, options)
        members = [_attach_number_line(object.__new__(NumberLine), engine_call(handle.coordinateAxis, index))
                   for index in (0, 1)]
        _family(self, handle, members)

    @property
    def x_axis(self):
        return self.submobjects[0]

    @property
    def y_axis(self):
        return self.submobjects[1]

    def _coordinate_frame(self):
        # Observe only two shaft identities, not every tick/label in the family.
        context = _coordinate_context([self.x_axis.shaft, self.y_axis.shaft])
        if context is not None:
            return engine_call(context.queryAxesFrame, self._semantic_family_handle)
        return engine_call(self._semantic_family_handle.axesFrame)

    def coords_to_point(self, x, y=0.0):
        with _owned(self._coordinate_frame()) as frame:
            return _base.Vec2(*engine_call(frame.coordsToPoint, float(x), float(y)))

    c2p = coords_to_point

    def point_to_coords(self, point):
        point = _base._as_vec2(point)
        with _owned(self._coordinate_frame()) as frame:
            return _base.Vec2(*engine_call(frame.pointToCoords, point.x, point.y))

    p2c = point_to_coords

    def get_origin(self):
        return self.c2p(0.0, 0.0)

    def plot(self, function, x_range=None, *, use_smoothing=True,
             discontinuities=(), dt=None, color=None, **kwargs):
        _callable(function)
        with _owned(self._coordinate_frame()) as frame:
            plan = engine_call(frame.plotPlan, _array(() if x_range is None else x_range),
                               _array(discontinuities), None if dt is None else float(dt), None)
        options = _evaluate(plan, function, use_smoothing, parametric=False)
        return _curve(object.__new__(FunctionGraph), options, color, kwargs)

    def plot_samples(self, points, *, color=None, **kwargs):
        """Noon extension: preserve data order and repeated x values as a polyline."""
        with _owned(self._coordinate_frame()) as frame:
            options = engine_call(frame.sampledPlot, _points(points))
        return _curve(object.__new__(_compat.VMobject), options, color, kwargs)


    def time_series_plan(self, samples, *, run_time):
        """Noon extension: map timestamp/value pairs and prepare interval timing.

        Samples must be finite and strictly increasing in time. The immutable
        result is preparation data for ordinary plays, not a clock or updater.
        No interpolation, sorting, or timestamp normalization occurs in Python.
        """
        with _owned(self._coordinate_frame()) as frame:
            with _owned(engine_call(frame.timeSeriesPlan, _points(samples), float(run_time))) as plan:
                return _TimeSeriesPlan(
                    _point_pairs(engine_call(plan.points)),
                    _point_pairs(engine_call(plan.cursorPoints)),
                    tuple(float(x) for x in engine_call(plan.keyTimes)),
                    tuple(float(x) for x in engine_call(plan.durations)),
                    float(engine_call(plan.runTime)),
                )


    def synchronized_series_plan(self, series, *, time_range, run_time):
        """Noon extension: prepare multiple recordings in one explicit window.

        Every recording must cover time_range. Rust splits at the union of input
        timestamps and linearly interpolates each series there; it never fills
        missing coverage or extrapolates. This immutable, bounded preparation is
        for ordinary plays, not a streaming player or a live coordinate cache.
        """
        with _owned(self._coordinate_frame()) as frame:
            rows = tuple(tuple(_base._as_vec2(point) for point in row) for row in series)
            values = _array(component for row in rows for point in row for component in point)
            with _owned(engine_call(
                frame.synchronizedSeriesPlan, values, _array(len(row) for row in rows),
                _array(time_range), float(run_time),
            )) as plan:
                return _SynchronizedTimeSeriesPlan(
                    tuple(_point_pairs(engine_call(plan.seriesPoints, index))
                          for index in range(int(engine_call(plan.seriesCount)))),
                    tuple(float(x) for x in engine_call(plan.dataTimes)),
                    _point_pairs(engine_call(plan.cursorPoints)),
                    tuple(float(x) for x in engine_call(plan.keyTimes)),
                    tuple(float(x) for x in engine_call(plan.durations)),
                    float(engine_call(plan.runTime)),
                )


    def gapped_series_plan(self, series, *, break_after, time_range, run_time):
        """Prepare explicitly disconnected recordings on one shared time grid.

        Supply one strictly increasing list of source break-after indices per
        recording. Index i disconnects samples i and i+1; both measured endpoints
        remain known. Missing points AND missing drawable segments are None.
        Never infer a line from two known points: consult series_segments.
        NaN guessing, extrapolation, and hold-last-value filling are not provided.
        """
        with _owned(self._coordinate_frame()) as frame:
            rows = tuple(tuple(_base._as_vec2(point) for point in row) for row in series)
            breaks = tuple(tuple(_break_index(i) for i in row) for row in break_after)
            values = _array(component for row in rows for point in row for component in point)
            with _owned(engine_call(
                frame.gappedSeriesPlan, values, _array(len(row) for row in rows),
                _array(i for row in breaks for i in row), _array(len(row) for row in breaks),
                _array(time_range), float(run_time),
            )) as plan:
                count = int(engine_call(plan.seriesCount))
                return _GappedTimeSeriesPlan(
                    tuple(tuple(None if p is None else _base.Vec2(*map(float, p))
                                for p in engine_call(plan.seriesPoints, i)) for i in range(count)),
                    tuple(tuple(None if pair is None else _point_pairs(pair)
                                for pair in engine_call(plan.seriesSegments, i)) for i in range(count)),
                    tuple(float(x) for x in engine_call(plan.dataTimes)),
                    _point_pairs(engine_call(plan.cursorPoints)),
                    tuple(float(x) for x in engine_call(plan.keyTimes)),
                    tuple(float(x) for x in engine_call(plan.durations)),
                    float(engine_call(plan.runTime)),
                )


def _callable(function):
    if not callable(function):
        raise TypeError("plot function must be callable")
    _outside_callback()


def _points(points):
    return _array(component for point in points for component in _base._as_vec2(point))


def _evaluate(plan, function, use_smoothing, *, parametric):
    with _owned(plan):
        parameters = engine_call(plan.parameters)
        # This is the only host function evaluation. No callback is retained in
        # the returned curve or invoked during deterministic playback.
        if parametric:
            values = _points(function(float(t)) for t in parameters)
            return engine_call(plan.parametricSamples, values, bool(use_smoothing))
        values = _array(function(float(x)) for x in parameters)
        return engine_call(plan.functionSamples, values, bool(use_smoothing))


def _curve(wrapper, options, color, kwargs):
    try:
        _shared._apply_shared_constructor_options(options, kwargs)
        _shared._apply_constructor_color(options, color)
    except BaseException:
        options.free()
        raise
    _shared._attach_geometry_options(wrapper, options, "static plot")
    return wrapper


class ParametricFunction(_compat.VMobject):
    """Preparation-only 2D parametric curve; supply an explicit t range."""

    def __init__(self, function, t_range, *, use_smoothing=True,
                 discontinuities=(), dt=None, color=None, **kwargs):
        _callable(function)
        plan = engine_call(_sampling_plan.parametric, _array(t_range), _array(discontinuities),
                           None if dt is None else float(dt), None)
        options = _evaluate(plan, function, use_smoothing, parametric=True)
        _curve(self, options, color, kwargs)


class FunctionGraph(_compat.VMobject):
    """Preparation-only y=f(x) in scene coordinates; supply an explicit range."""

    def __init__(self, function, x_range, *, use_smoothing=True,
                 discontinuities=(), dt=None, color=None, **kwargs):
        _callable(function)
        plan = engine_call(_sampling_plan.parametric, _array(x_range), _array(discontinuities),
                           None if dt is None else float(dt), None)
        options = _evaluate(plan, function, use_smoothing, parametric=False)
        _curve(self, options, color, kwargs)


__all__ = ["NumberLine", "Axes", "FunctionGraph", "ParametricFunction"]
