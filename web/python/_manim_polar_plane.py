"""PolarPlane syntax over retained Rust circles, rays and axis families."""
from operator import index

import _manim_plotting as _plot
from _noon_errors import engine_call


def _line_style(options, values, *, faded):
    values = dict(values)
    unknown = sorted(set(values) - {"stroke_color", "stroke_width", "stroke_opacity"})
    if unknown:
        raise TypeError("unsupported PolarPlane line style: " + ", ".join(unknown))
    color = values.get("stroke_color")
    rgba = ()
    if color is not None:
        color = _plot._compat._as_color("stroke_color", color)
        rgba = (color.red, color.green, color.blue, color.alpha)
    engine_call(
        options.setPolarLineStyle, faded, _plot._array(rgba),
        None if "stroke_width" not in values else _plot._compat._manim_stroke_width(values["stroke_width"]),
        None if "stroke_opacity" not in values else float(values["stroke_opacity"]),
    )


def _lines(handle):
    return _plot._family(object.__new__(_plot._compat.Group), handle,
                         [_plot._leaf(leaf) for leaf in engine_call(handle.directMobjects)])


class PolarPlane(_plot.Axes):
    """A retained polar grid built by shared Rust semantics.

    Radius and azimuth conversion, concentric circles, radial rays, painter
    order, and admission limits are owned by shared Rust coordinate authoring.
    Numeric label families and nonlinear deformation remain unsupported.
    """
    def __init__(self, radius_max=4, size=None, radius_step=1, azimuth_step=None,
                 azimuth_units="PI radians", azimuth_compact_fraction=True,
                 azimuth_offset=0, azimuth_direction="CCW", azimuth_label_buff=0.1,
                 azimuth_label_font_size=24, radius_config=None,
                 background_line_style=None, faded_line_style=None,
                 faded_line_ratio=1, make_smooth_after_applying_functions=True, **kwargs):
        if kwargs:
            raise TypeError("unsupported PolarPlane option(s): " + ", ".join(sorted(kwargs)))
        if azimuth_units not in {"PI radians", "TAU radians", "degrees", "gradians", None}:
            raise ValueError("invalid azimuth_units")
        if azimuth_direction not in {"CW", "CCW"}:
            raise ValueError("azimuth_direction must be 'CW' or 'CCW'")
        if isinstance(faded_line_ratio, bool):
            raise TypeError("faded_line_ratio must be an integer, not bool")
        config = dict(radius_config or {})
        for flag in ("include_ticks", "include_tip"):
            if config.pop(flag, False):
                raise NotImplementedError("PolarPlane ticks and tips are not yet supported")
        color = config.pop("color", config.pop("stroke_color", None))
        defaults = {"PI radians": 20, "TAU radians": 20, "degrees": 36, "gradians": 40, None: 1}
        step = defaults[azimuth_units] if azimuth_step is None else float(azimuth_step)
        context = _plot._coordinate_constructor_context()
        options = engine_call(
            _plot._coordinate_options.polarPlane, float(radius_max),
            None if size is None else float(size), float(radius_step), step,
            float(azimuth_offset), azimuth_direction == "CW", index(faded_line_ratio),
        )
        try:
            _plot._coordinate_style(options, color, config)
            if background_line_style is not None:
                _line_style(options, background_line_style, faded=False)
            if faded_line_style is not None:
                _line_style(options, faded_line_style, faded=True)
        except BaseException:
            options.free()
            raise
        handle = engine_call(context.liveCreateCoordinates if context is not None else _plot._create_coordinates, options)
        members = [_lines(engine_call(handle.polarPlanePart, i)) for i in (0, 1)]
        members.extend(_plot._attach_number_line(object.__new__(_plot.NumberLine), engine_call(handle.polarPlanePart, i)) for i in (2, 3))
        _plot._family(self, handle, members)
        self.azimuth_units = azimuth_units
        self.azimuth_compact_fraction = bool(azimuth_compact_fraction)
        self.azimuth_label_buff = float(azimuth_label_buff)
        self.azimuth_label_font_size = float(azimuth_label_font_size)
        self.make_smooth_after_applying_functions = bool(make_smooth_after_applying_functions)

    @property
    def faded_lines(self): return self.submobjects[0]
    @property
    def background_lines(self): return self.submobjects[1]
    @property
    def x_axis(self): return self.submobjects[2]
    @property
    def y_axis(self): return self.submobjects[3]

    def _polar_frame(self):
        context = _plot._coordinate_context([self.x_axis.shaft, self.y_axis.shaft])
        if context is not None:
            return engine_call(context.queryPolarPlaneFrame, self._semantic_family_handle)
        return engine_call(self._semantic_family_handle.polarPlaneFrame)

    def _coordinate_frame(self):
        with _plot._owned(self._polar_frame()) as frame:
            return engine_call(frame.axesFrame)

    def polar_to_point(self, radius, azimuth):
        with _plot._owned(self._polar_frame()) as frame:
            return _plot._base.Vec2(*engine_call(frame.polarToPoint, float(radius), float(azimuth)))

    def point_to_polar(self, point):
        point = _plot._base._as_vec2(point)
        with _plot._owned(self._polar_frame()) as frame:
            return _plot._base.Vec2(*engine_call(frame.pointToPolar, point.x, point.y))

    def get_origin(self): return self.polar_to_point(0.0, 0.0)

    def add_coordinates(self, *args, **kwargs):
        raise NotImplementedError("PolarPlane numeric label families are not yet supported")


__all__ = ["PolarPlane"]
