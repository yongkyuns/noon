"""Static NumberPlane grid over ordinary shared Rust coordinate/family handles."""
from operator import index

import _manim_plotting as _plot
from _noon_errors import engine_call


def _line_style(options, values, *, faded):
    values = dict(values)
    unknown = sorted(set(values) - {"stroke_color", "stroke_width", "stroke_opacity"})
    if unknown:
        raise TypeError("unsupported NumberPlane line style: " + ", ".join(unknown))
    color = values.get("stroke_color")
    rgba = ()
    if color is not None:
        color = _plot._compat._as_color("stroke_color", color)
        rgba = (color.red, color.green, color.blue, color.alpha)
    engine_call(
        options.setPlaneLineStyle, faded, _plot._array(rgba),
        None if "stroke_width" not in values else _plot._compat._manim_stroke_width(values["stroke_width"]),
        None if "stroke_opacity" not in values else float(values["stroke_opacity"]),
    )


def _lines(handle):
    return _plot._family(object.__new__(_plot._compat.Group), handle,
                         [_plot._leaf(leaf) for leaf in engine_call(handle.directMobjects)])


class NumberPlane(_plot.Axes):
    """Linear 2D axes with bounded retained major/faded grid lines.

    Supports ranges, lengths, grid stroke styles and common axis stroke style.
    Tips, ticks, nonlinear deformation and axis-specific configuration remain
    unsupported. Coordinates and plots use the same shared Rust AxesFrame.
    """

    def __init__(self, x_range=None, y_range=None, x_length=None, y_length=None,
                 background_line_style=None, faded_line_style=None,
                 faded_line_ratio=1, *, axis_config=None, **kwargs):
        if kwargs:
            raise TypeError("unsupported NumberPlane option(s): " + ", ".join(sorted(kwargs)))
        if isinstance(faded_line_ratio, bool):
            raise TypeError("faded_line_ratio must be an integer, not bool")
        ratio = index(faded_line_ratio)
        config = dict(axis_config or {})
        for flag in ("include_ticks", "include_tip"):
            if config.pop(flag, False):
                raise NotImplementedError("NumberPlane ticks and tips are not yet supported")
        color = config.pop("color", config.pop("stroke_color", None))
        context = _plot._coordinate_constructor_context()
        options = engine_call(
            _plot._coordinate_options.numberPlane,
            _plot._array(() if x_range is None else x_range),
            _plot._array(() if y_range is None else y_range),
            None if x_length is None else float(x_length),
            None if y_length is None else float(y_length), ratio,
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
        members = [_lines(engine_call(handle.numberPlanePart, i)) for i in (0, 1)]
        members.extend(_plot._attach_number_line(object.__new__(_plot.NumberLine), engine_call(handle.numberPlanePart, i))
                       for i in (2, 3))
        _plot._family(self, handle, members)

    @property
    def faded_lines(self):
        return self.submobjects[0]

    @property
    def background_lines(self):
        return self.submobjects[1]

    @property
    def x_axis(self):
        return self.submobjects[2]

    @property
    def y_axis(self):
        return self.submobjects[3]

    def _coordinate_frame(self):
        context = _plot._coordinate_context([self.x_axis.shaft, self.y_axis.shaft])
        if context is not None:
            return engine_call(context.queryNumberPlaneFrame, self._semantic_family_handle)
        return engine_call(self._semantic_family_handle.numberPlaneFrame)

    def add_coordinates(self, *args, **kwargs):
        raise NotImplementedError("NumberPlane numeric label families are not yet supported")


__all__ = ["NumberPlane"]
