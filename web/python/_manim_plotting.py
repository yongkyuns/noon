"""Linear coordinate families and preparation-only plots over shared Rust handles.

The 2D coordinate constructors require explicit three-value ranges and do not
support tips. The bounded ThreeDAxes constructor retains its three default tips.
Coordinates may be constructed after ordinary plays/waits;
add late coordinates to the Scene before querying or plotting against them.
Numeric native Text label families must be constructed before the first play/wait.
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
    from js import noonAuthoringBarChartOptions as _bar_chart_options
    from js import noonCreateAuthoringBarChart as _create_bar_chart
    from js import noonBarChartLabels as _bar_labels
    from js import noonPlotSamplingPlan as _sampling_plan
    from pyodide.ffi import to_js as _to_js, jsnull as _jsnull
except ImportError:
    _coordinate_options = _create_coordinates = _bar_chart_options = _create_bar_chart = _bar_labels = _sampling_plan = _to_js = _jsnull = None


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
    if contexts and any(context is not contexts[0] for context in contexts[1:]):
        raise RuntimeError("coordinate shafts belong to different execution contexts")
    context = contexts[0] if contexts else _shared._live_constructor_context("coordinate query")
    if context is not None and not all(_shared._is_bound(shaft) for shaft in shafts):
        raise NotImplementedError(
            "add live coordinates to the Scene before querying or plotting; "
            "detached live coordinate reads are not yet exposed by the Python context"
        )
    return context


def _family(wrapper, handle, members):
    wrapper._semantic_family_handle = handle
    wrapper._semantic_member_wrappers = {
        _shared._family_wrapper_key(member): member for member in members
    }
    return wrapper


def _leaf(handle, kind=_compat.Line):
    wrapper = object.__new__(kind)
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


def _coordinate_constructor_context():
    _outside_callback()
    if _create_coordinates is None:
        raise RuntimeError("coordinates require the shared Rust authoring host")
    return _shared._live_constructor_context("coordinates")


class NumberLine(_compat.Group):
    """Explicit-range, tipless linear number line with Rust-owned ticks/range."""

    def __init__(self, x_range, *, length=None, unit_size=1.0, rotation=0.0,
                 include_ticks=True, tick_size=0.1, exclude_origin_tick=False,
                 include_tip=False, numbers_with_elongated_ticks=None,
                 longer_tick_multiple=2, color=None, **kwargs):
        context = _coordinate_constructor_context()
        if include_tip:
            raise NotImplementedError("NumberLine tips are not yet supported")
        options = engine_call(
            _coordinate_options.numberLine, _array(x_range),
            None if length is None else float(length), float(unit_size), float(rotation),
        )
        try:
            engine_call(options.setTicks, bool(include_ticks), float(tick_size), bool(exclude_origin_tick))
            engine_call(options.setElongatedTicks,
                        _array(() if numbers_with_elongated_ticks is None else numbers_with_elongated_ticks),
                        float(longer_tick_multiple))
            _coordinate_style(options, color, kwargs)
        except BaseException:
            options.free()
            raise
        _attach_number_line(self, engine_call(context.liveCreateCoordinates if context is not None else _create_coordinates, options))

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

    def get_number_mobjects(self, *numbers, **kwargs):
        """Detached ordinary Text labels; no arguments selects automatic ticks."""
        from _manim_number_labels import number_mobjects
        return number_mobjects(self, numbers or None, attach=False, config=kwargs)

    def add_numbers(self, x_values=None, **kwargs):
        """Atomically append native Text labels before playback; expose .numbers.

        Repeated calls append another label family, rather than replacing the
        earlier one. This bounded subset does not construct DecimalNumber/TeX.
        """
        from _manim_number_labels import number_mobjects
        number_mobjects(self, x_values, attach=True, config=kwargs)
        return self

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


class UnitInterval(NumberLine):
    """Unit interval with shared Rust endpoint ticks and coordinate queries.

    Numeric label formatting remains the explicit NumberLine native-Text API.
    """

    def __init__(self, unit_size=10, numbers_with_elongated_ticks=None, **kwargs):
        super().__init__((0, 1, 0.1), unit_size=unit_size,
                         numbers_with_elongated_ticks=(0, 1) if numbers_with_elongated_ticks is None
                         else numbers_with_elongated_ticks, **kwargs)


class Axes(_compat.Group):
    """Explicitly sized, linear 2D axes. Curves are independent retained paths."""

    def __init__(self, x_range, y_range, *, x_length, y_length, tips=False,
                 include_ticks=True, tick_size=0.1, color=None, **kwargs):
        context = _coordinate_constructor_context()
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
        handle = engine_call(context.liveCreateCoordinates if context is not None else _create_coordinates, options)
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
        graph = _curve(object.__new__(FunctionGraph), options, color, kwargs)
        graph.underlying_function = function
        return graph



    def get_area(self, graph, x_range=None, color=None, opacity=0.3,
                 bounded_graph=None, **kwargs):
        """Create one static closed path prepared by the shared Rust query."""
        context = _coordinate_context([self.x_axis.shaft, self.y_axis.shaft])
        if context is not None:
            options = engine_call(
                context.effectiveAxesAreaOptions, self._semantic_family_handle,
                graph._semantic_handle, _array(() if x_range is None else x_range),
                graph._semantic_handle if bounded_graph is None else bounded_graph._semantic_handle,
                bounded_graph is not None,
            )
        else:
            with _owned(self._coordinate_frame()) as frame:
                options = engine_call(
                    frame.area, graph._semantic_handle,
                    _array(() if x_range is None else x_range),
                    graph._semantic_handle if bounded_graph is None else bounded_graph._semantic_handle,
                    bounded_graph is not None,
                )
        kwargs.setdefault("fill_opacity", float(opacity))
        kwargs.setdefault("stroke_opacity", float(opacity))
        colors = (_base.BLUE, _base.GREEN) if color is None else color
        if isinstance(colors, (list, tuple)):
            result = _curve(object.__new__(_compat.VMobject), options, None, kwargs)
            return result.set_color_by_gradient(*colors)
        return _curve(object.__new__(_compat.VMobject), options, colors, kwargs)

    def get_riemann_rectangles(self, graph, x_range=None, dx=0.1,
                               input_sample_type="left", stroke_width=1,
                               stroke_color=_base.BLACK, fill_opacity=1,
                               color=(_base.BLUE, _base.GREEN), show_signed_area=True,
                               bounded_graph=None, blend=False, width_scale_factor=1.001):
        """Sample original scalar callables against one Rust-captured snapshot."""
        context = _coordinate_context([self.x_axis.shaft, self.y_axis.shaft])
        sample = {"left": 0, "right": 1, "center": 2}.get(input_sample_type)
        if sample is None:
            raise ValueError("input_sample_type must be 'left', 'right', or 'center'")
        colors = color if isinstance(color, (tuple, list)) else (color,)
        colors = [_compat._as_color("Riemann color", value) for value in colors]
        stroke_color = _compat._as_color("Riemann stroke color", stroke_color)
        bounded = graph if bounded_graph is None else bounded_graph
        graph_handle, bounded_handle = graph._semantic_handle, bounded._semantic_handle
        if context is None:
            prepare = self._semantic_family_handle.riemannSamplePlan
            prepare_args = (graph_handle,)
        else:
            prepare = context.liveEffectiveRiemannSamplePlan
            prepare_args = (self._semantic_family_handle, graph_handle)
        options = engine_call(_coordinate_options.riemann, _array(() if x_range is None else x_range),
                              float(dx), sample, float(width_scale_factor))
        try:
            engine_call(options.setPaint, _shared._gradient_components(colors),
                        _shared._gradient_components([stroke_color]), float(stroke_width),
                        float(fill_opacity), bool(show_signed_area), bool(blend))
        except BaseException:
            options.free()
            raise
        plan = engine_call(prepare, *prepare_args, options, bounded_handle,
                           bounded_graph is not None)
        with _owned(plan):
            function = getattr(graph, "underlying_function", None)
            bounded_function = (None if bounded_graph is None else
                                getattr(bounded_graph, "underlying_function", None))
            top_values, baseline_values = [], []
            # Preserve per-rectangle top-then-lower invocation order. Each graph
            # independently uses its callable or the captured Rust path fallback.
            for x, sample_x in zip(engine_call(plan.starts), engine_call(plan.samples), strict=True):
                if function is not None:
                    top_values.append(float(function(float(sample_x))))
                if bounded_function is not None:
                    baseline_values.append(float(bounded_function(float(x))))
            values = (_array(top_values), _array(baseline_values))
            handle = (engine_call(plan.publish, *values) if context is None else
                      engine_call(context.livePublishRiemannPlan, plan, *values))
        members = [_leaf(member, _compat.Rectangle) for member in engine_call(handle.directMobjects)]
        return _family(object.__new__(_compat.VGroup), handle, members)

    def add_coordinates(self, x_values=None, y_values=None, *, x_config=None, y_config=None, **kwargs):
        """Atomically append both axes' native Text label families before playback.

        Options shared by both axes may be overridden with x_config/y_config.
        Labels become members and follow subsequent transforms and copies.
        """
        from _manim_number_labels import add_coordinates
        return add_coordinates(self, x_values, y_values, x_config=x_config,
                               y_config=y_config, config=kwargs)

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
                    tuple(tuple(None if p is None or p is _jsnull else _base.Vec2(*map(float, p))
                                for p in engine_call(plan.seriesPoints, i)) for i in range(count)),
                    tuple(tuple(None if pair is None or pair is _jsnull else _point_pairs(pair)
                                for pair in engine_call(plan.seriesSegments, i)) for i in range(count)),
                    tuple(float(x) for x in engine_call(plan.dataTimes)),
                    _point_pairs(engine_call(plan.cursorPoints)),
                    tuple(float(x) for x in engine_call(plan.keyTimes)),
                    tuple(float(x) for x in engine_call(plan.durations)),
                    float(engine_call(plan.runTime)),
                )


    def plot_implicit_curve(self, func, min_depth=5, max_quads=1500, **kwargs):
        from _manim_implicit import plot_implicit_curve
        return plot_implicit_curve(self, func, min_depth, max_quads, **kwargs)


def _attach_three_d_axis(wrapper, handle, tip_handle):
    shaft = _leaf(engine_call(handle.coordinateShaft))
    ticks = _family(
        object.__new__(_compat.Group),
        engine_call(handle.coordinateTicks),
        [_leaf(tick) for tick in engine_call(handle.coordinateTickObjects)],
    )
    members = [shaft, ticks]
    wrapper.tip = None
    if tip_handle is not None:
        wrapper.tip = _leaf(tip_handle, _compat.VMobject)
        members.append(wrapper.tip)
    return _family(wrapper, handle, members)


class ThreeDAxes(_compat.Group):
    """Linear X/Y/Z axes with pinned Manim ranges, lengths and positive tips.

    The numeric frame, world-axis transforms, ticks, tips, and coordinate
    conversions are Rust-owned. Cairo axis pieces/shading, labels, custom axis
    configurations, z-normal changes, and custom tip shapes are unsupported.
    """

    def __init__(self, x_range=(-6, 6, 1), y_range=(-5, 5, 1),
                 z_range=(-4, 4, 1), x_length=10.5, y_length=10.5,
                 z_length=6.5, *, tips=True, include_ticks=True, tick_size=0.1,
                 labels=None, color=None, **kwargs):
        context = _coordinate_constructor_context()
        if labels is not None:
            raise NotImplementedError("ThreeDAxes axis labels are not yet supported")
        if not isinstance(tips, bool) or not isinstance(include_ticks, bool):
            raise TypeError("tips and include_ticks require booleans")
        options = engine_call(
            _coordinate_options.threeDAxes, _array(x_range), _array(y_range),
            _array(z_range), float(x_length), float(y_length), float(z_length),
        )
        try:
            engine_call(options.setTicks, include_ticks, float(tick_size), True)
            engine_call(options.setTips, tips)
            _coordinate_style(options, color, kwargs)
        except BaseException:
            options.free()
            raise
        handle = engine_call(
            context.liveCreateCoordinates if context is not None else _create_coordinates,
            options,
        )
        axes = [
            _attach_three_d_axis(
                object.__new__(NumberLine),
                engine_call(handle.threeDAxesAxis, index),
                engine_call(handle.threeDAxesTip, index),
            )
            for index in range(3)
        ]
        _family(self, handle, axes)

    @property
    def x_axis(self):
        return self.submobjects[0]

    @property
    def y_axis(self):
        return self.submobjects[1]

    @property
    def z_axis(self):
        return self.submobjects[2]

    def _coordinate_frame(self):
        shafts = [self.x_axis.shaft, self.y_axis.shaft, self.z_axis.shaft]
        context = _coordinate_context(shafts)
        if context is not None:
            return engine_call(context.queryThreeDAxesFrame, self._semantic_family_handle)
        return engine_call(self._semantic_family_handle.threeDAxesFrame)

    def coords_to_point(self, x, y, z):
        with _owned(self._coordinate_frame()) as frame:
            return tuple(float(value) for value in engine_call(
                frame.coordsToPoint, float(x), float(y), float(z)))

    c2p = coords_to_point

    def point_to_coords(self, point):
        try:
            values = tuple(float(component) for component in point)
        except TypeError as error:
            raise TypeError("point must contain three numeric coordinates") from error
        if len(values) != 3:
            raise ValueError("point must contain three numeric coordinates")
        with _owned(self._coordinate_frame()) as frame:
            return tuple(float(value) for value in engine_call(
                frame.pointToCoords, *values))

    p2c = point_to_coords

    def plot_samples(self, points, *, color=None, **kwargs):
        """Noon extension: preserve data order and repeated x values as a polyline."""
        with _owned(self._coordinate_frame()) as frame:
            options = engine_call(frame.sampledPlot, _points(points))
        return _curve(object.__new__(_compat.VMobject), options, color, kwargs)



class BarChart(Axes):
    """Shared-Rust static bar chart with explicit, atomic value changes.

    The compatibility wrapper owns argument coercion and wrapper identity. Axes,
    rectangle layout, color gradients and updates stay in the typed Rust chart.
    """

    def __init__(self, values, bar_names=None, y_range=None, x_length=None,
                 y_length=None, bar_colors=None, bar_width=0.6,
                 bar_fill_opacity=0.7, bar_stroke_width=3, **kwargs):
        x_config = dict(kwargs.pop("x_axis_config", {}))
        name_size = float(x_config.pop("font_size", 24))
        y_config = dict(kwargs.pop("y_axis_config", {}))
        label_settings = {"font": "DejaVu Sans Mono", "font_size": 36, "buff": 0.25, "direction": _base.LEFT}
        aliases = {"label_direction": "direction", "line_to_number_buff": "buff"}
        for key, value in y_config.items():
            label_settings[aliases.get(key, key)] = value
        if kwargs or x_config:
            unsupported = sorted(kwargs) + ["x_axis_config." + key for key in sorted(x_config)]
            raise NotImplementedError("unsupported BarChart option(s): " + ", ".join(unsupported))
        if bar_names is not None:
            bar_names = tuple(bar_names)
            if not all(isinstance(name, str) for name in bar_names):
                raise TypeError("bar names must be strings")
        values = tuple(float(value) for value in values)
        if _bar_chart_options is None:
            raise RuntimeError("BarChart requires the shared Rust authoring host")
        options = engine_call(_bar_chart_options, _array(values),
                              _array(() if y_range is None else y_range),
                              0.0 if x_length is None else float(x_length),
                              0.0 if y_length is None else float(y_length))
        try:
            engine_call(options.setStyle, float(bar_width), float(bar_fill_opacity), float(bar_stroke_width))
            if bar_colors is not None:
                colors = bar_colors if isinstance(bar_colors, (tuple, list)) else (bar_colors,)
                rgba = [component for color in colors for component in (
                    _compat._as_color("bar color", color).red,
                    _compat._as_color("bar color", color).green,
                    _compat._as_color("bar color", color).blue,
                    _compat._as_color("bar color", color).alpha,
                )]
                engine_call(options.setColors, _array(rgba))
            context = _shared._live_constructor_context("bar chart", allow_unstarted=True)
            if bar_names is not None:
                engine_call(options.setNames, _to_js(list(bar_names)), name_size)
            from _manim_number_labels import _options
            label_options, size, color = _options(label_settings)
        except BaseException:
            options.free()
            raise
        self._bar_chart_handle = engine_call(_create_bar_chart, options, label_options, context)
        bars = engine_call(self._bar_chart_handle.bars)
        self.bars = _family(object.__new__(_compat.VGroup), bars,
                            [_leaf(handle, _compat.Rectangle) for handle in engine_call(bars.directMobjects)])
        axes = engine_call(self._bar_chart_handle.axes)
        axis_members = [
            _attach_number_line(object.__new__(NumberLine), engine_call(axes.coordinateAxis, index))
            for index in (0, 1)
        ]
        self.axes = _family(object.__new__(Axes), axes, axis_members)
        from _manim_number_labels import _family as _numeric_family, _remember
        names = engine_call(self._bar_chart_handle.xLabels)
        if names is not None and names is not _jsnull:
            self.x_axis.labels = _chart_text_family(names, name_size, context)
            self.x_axis._semantic_member_wrappers[_shared._family_wrapper_key(self.x_axis.labels)] = self.x_axis.labels
        labels = engine_call(self._bar_chart_handle.yLabels)
        _remember(self.y_axis, _numeric_family(labels, size, color))
        _family(self, engine_call(self._bar_chart_handle.family), [self.bars, self.axes])
        self._bar_chart_context = context

    def _rehydrate_semantic_family_handle(self):
        """Rebind this copied wrapper to its copied Rust chart family."""
        self._bar_chart_handle = engine_call(self._semantic_family_handle.barChart)
        self.__dict__.pop("_bar_chart_context", None)

    @property
    def x_axis(self):
        return self.axes.x_axis

    @property
    def y_axis(self):
        return self.axes.y_axis

    @property
    def values(self):
        return list(engine_call(self._bar_chart_handle.values))

    @property
    def _coordinate_handle(self):
        return self.axes._coordinate_handle

    def get_bar_labels(self, color=None, font_size=24, buff=0.25, label_constructor=None):
        from _manim_latex import Tex, MathTex
        constructor = Tex if label_constructor is None else label_constructor
        if constructor not in (Tex, MathTex):
            raise NotImplementedError("BarChart labels support Tex or MathTex")
        if _bar_labels is None:
            raise RuntimeError("BarChart labels require the shared Rust authoring host")
        context = _coordinate_context([self.x_axis.shaft, self.y_axis.shaft])
        rgba = []
        if color is not None:
            c = _compat._as_color("label color", color)
            rgba = [c.red, c.green, c.blue, c.alpha]
        handle = engine_call(_bar_labels, self._bar_chart_handle, float(font_size), float(buff),
                             constructor is MathTex, _array(rgba), context)
        return _chart_text_family(handle, float(font_size), context)

    def change_bar_values(self, values, update_colors=True):
        values = tuple(float(value) for value in values)
        context = _coordinate_context([self.x_axis.shaft, self.y_axis.shaft])
        old = engine_call(self._bar_chart_handle.barPrefix, len(values))
        if context is None:
            engine_call(self._bar_chart_handle.changeBarValues, _array(values), bool(update_colors))
        else:
            engine_call(context.liveChangeBarValues, self._bar_chart_handle, _array(values), bool(update_colors))
        new = engine_call(self._bar_chart_handle.barPrefix, len(values))
        changed = []
        for before, after in zip(old, new):
            before_key = f"{int(before.semanticSlot)}:{int(before.semanticGeneration)}"
            after_key = f"{int(after.semanticSlot)}:{int(after.semanticGeneration)}"
            if before_key != after_key:
                previous = self.bars._semantic_member_wrappers.pop(before_key, None)
                replacement = _leaf(after, _compat.Rectangle)
                if context is not None:
                    replacement._canonical_live_target_context = context
                self.bars._semantic_member_wrappers[after_key] = replacement
                changed.extend((previous, replacement) if previous is not None else (replacement,))
        scene = self.x_axis.shaft._scene
        if changed and scene is not None:
            from _manim_scene import _reconcile_completed_family_bindings
            _reconcile_completed_family_bindings(scene, tuple(changed))
        return self


def _chart_text_family(handle, font_size, context=None):
    from _manim_latex import _CompiledTexLeaf

    members = []
    for object_handle, source in engine_call(handle.numberLabelMembers):
        label = object.__new__(_CompiledTexLeaf)
        label._initialize_text(str(source), font_size, object_handle, _base.WHITE, 1.0,
                               presentation_applied=True)
        if context is not None:
            label._canonical_live_target_context = context
        members.append(label)
    return _family(object.__new__(_compat.VGroup), handle, members)


def _callable(function):
    if not callable(function):
        raise TypeError("plot function must be callable")
    _outside_callback()


def _points(points):
    return _array(component for point in points for component in _base._as_vec2(point))


def _evaluate(plan, function, use_smoothing, *, parametric):
    with _owned(plan):
        parameters = engine_call(plan.parameters)
        # Construction evaluates once. Scalar callable identity may be retained
        # for explicit Riemann queries, never for deterministic frame playback.
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
        self.underlying_function = function


__all__ = ["NumberLine", "Axes", "BarChart", "FunctionGraph", "ParametricFunction"]
