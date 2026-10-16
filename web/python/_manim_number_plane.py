"""Static NumberPlane grid over ordinary shared Rust coordinate/family handles."""
import math
from collections.abc import Mapping
from operator import index

import noon as _base
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


def _parallel_grid_lines(lines, axis):
    """Select retained grid lines parallel to an axis in the current frame."""
    axis = getattr(axis, "shaft", axis)
    axis_start = axis.get_start()
    axis_end = axis.get_end()
    axis_dx = axis_end.x - axis_start.x
    axis_dy = axis_end.y - axis_start.y
    axis_length = math.hypot(axis_dx, axis_dy)
    if axis_length == 0.0:
        raise ValueError("NumberPlane axis must have nonzero length")

    selected = []
    for line in lines.submobjects:
        start = line.get_start()
        end = line.get_end()
        line_dx = end.x - start.x
        line_dy = end.y - start.y
        line_length = math.hypot(line_dx, line_dy)
        if line_length == 0.0:
            continue
        cross = axis_dx * line_dy - axis_dy * line_dx
        if abs(cross) <= 1e-9 * axis_length * line_length:
            selected.append(line)
    return _plot._compat.VGroup(*selected)


def _coordinate_decimal_places(axis_range):
    """Mirror NumberLine formatting without normalizing the authored step type."""
    try:
        step = 1 if axis_range is None or len(axis_range) < 3 else axis_range[2]
    except (TypeError, IndexError):
        return 0
    spelling = str(step)
    return len(spelling.split(".")[-1]) if "." in spelling else 0


_AXIS_STYLE_KEYS = {"color", "stroke_color", "stroke_width", "stroke_opacity", "opacity"}
_AXIS_LABEL_KEYS = _plot._AXES_CONFIG_KEYS - {"scaling"}
_NUMBER_PLANE_AXIS_KEYS = _AXIS_STYLE_KEYS | _AXIS_LABEL_KEYS | {
    "include_ticks", "include_tip", "tick_size", "exclude_origin_tick", "scaling",
}


def _config_mapping(config, name):
    if config is None:
        return {}
    if not isinstance(config, Mapping):
        raise TypeError(f"{name} must be a mapping")
    if any(not isinstance(key, str) for key in config):
        raise TypeError(f"{name} keys must be strings")
    unknown = sorted(set(config) - _NUMBER_PLANE_AXIS_KEYS)
    if unknown:
        raise NotImplementedError(
            f"unsupported {name} option(s): " + ", ".join(unknown)
        )
    return dict(config)


def _axis_style_override(config, common):
    """Normalize style differences from the shared axis style for Rust."""
    override = {}
    common_color = common.get("color", common.get("stroke_color"))
    color = config.get("color", config.get("stroke_color"))
    parsed = None if color is None else _plot._compat._as_color("color", color)
    if parsed is not None and color != common_color:
        override["color"] = (parsed.red, parsed.green, parsed.blue, parsed.alpha)

    for key in ("stroke_width", "stroke_opacity", "opacity"):
        value = config.get(key)
        if key not in config or value is None:
            continue
        if key == "stroke_width":
            normalized = _plot._compat._manim_stroke_width(value)
        else:
            normalized = _plot._compat._opacity(key, value)
        if value != common.get(key):
            override[key] = normalized
    return override


def _axis_configurations(axis_config, x_axis_config, y_axis_config):
    common = {
        "stroke_width": 2,
        "include_ticks": False,
        "include_tip": False,
        "tick_size": 0.1,
        "exclude_origin_tick": True,
        "line_to_number_buff": _plot._base.SMALL_BUFF,
        "label_direction": _plot._base.DR,
        "font_size": 24,
    }
    common.update(_config_mapping(axis_config, "axis_config"))
    x_config = {**common, **_config_mapping(x_axis_config, "x_axis_config")}
    y_config = {
        **common,
        "label_direction": _plot._base.DR,
        **_config_mapping(y_axis_config, "y_axis_config"),
    }

    axis_options = []
    for name, config, direction in (
        ("x_axis_config", x_config, _plot._base.DR),
        ("y_axis_config", y_config, _plot._base.DR),
    ):
        for key in ("include_ticks", "include_tip", "exclude_origin_tick"):
            if not isinstance(config[key], bool):
                raise TypeError(f"{name}.{key} must be a boolean")
        if config["include_tip"]:
            raise NotImplementedError("NumberPlane tips are not yet supported")
        config["tick_size"] = _plot._finite_axis_option(
            config["tick_size"], f"{name}.tick_size", nonnegative=True
        )
        if config.get("scaling") is not None:
            raise NotImplementedError("NumberPlane nonlinear axis scaling is not yet supported")
        # Axes normalizes every linear axis to omit its origin tick after
        # merging axis_config and x/y_axis_config. Match that inherited rule,
        # including when the caller explicitly supplies False.
        config["exclude_origin_tick"] = True
        labels = {key: config[key] for key in _AXIS_LABEL_KEYS if key in config}
        label_options = _plot._axes_label_options(
            labels, ("linear", 1.0, False), name, direction
        )
        axis_options.append((label_options, config))

    style = {key: common[key] for key in ("stroke_width", "stroke_opacity", "opacity")
             if key in common}
    color = common.get("color", common.get("stroke_color"))
    x_style = _axis_style_override(x_config, common)
    y_style = _axis_style_override(y_config, common)
    return (
        axis_options[0][0], axis_options[1][0], color, style,
        axis_options[0][1], axis_options[1][1], x_style, y_style,
    )


class NumberPlane(_plot.Axes):
    """Linear 2D axes with bounded retained major/faded grid lines.

    Supports ranges, lengths, grid stroke styles, common axis stroke style,
    native Text constructor labels with per-axis formatting, independent ticks
    and axis stroke styling, and major x/y grid-line aliases. Tips and nonlinear
    deformation remain unsupported.
    Coordinates and plots use the same shared Rust AxesFrame.
    """

    def __init__(self, x_range=None, y_range=None, x_length=None, y_length=None,
                 background_line_style=None, faded_line_style=None,
                 faded_line_ratio=1, *, axis_config=None, x_axis_config=None,
                 y_axis_config=None, **kwargs):
        if kwargs:
            raise TypeError("unsupported NumberPlane option(s): " + ", ".join(sorted(kwargs)))
        if isinstance(faded_line_ratio, bool):
            raise TypeError("faded_line_ratio must be an integer, not bool")
        ratio = index(faded_line_ratio)
        (x_options, y_options, color, style, x_axis_options, y_axis_options,
         x_axis_style, y_axis_style) = _axis_configurations(
            axis_config, x_axis_config, y_axis_config
        )
        x_range_values = () if x_range is None else tuple(x_range)
        y_range_values = () if y_range is None else tuple(y_range)
        context = _plot._coordinate_constructor_context()
        options = engine_call(
            _plot._coordinate_options.numberPlane,
            _plot._array(x_range_values),
            _plot._array(y_range_values),
            None if x_length is None else float(x_length),
            None if y_length is None else float(y_length), ratio,
        )
        try:
            _plot._coordinate_style(options, color, style)
            for axis_index, axis_style in enumerate((x_axis_style, y_axis_style)):
                if axis_style:
                    engine_call(
                        options.setPlaneAxisStyle,
                        axis_index,
                        _plot._array(axis_style.get("color", ())),
                        axis_style.get("stroke_width"),
                        axis_style.get("stroke_opacity"),
                        axis_style.get("opacity"),
                    )
            if background_line_style is not None:
                _line_style(options, background_line_style, faded=False)
            if faded_line_style is not None:
                _line_style(options, faded_line_style, faded=True)
            for axis_index, config in enumerate((x_axis_options, y_axis_options)):
                engine_call(
                    options.setPlaneAxisTicks,
                    axis_index,
                    config["include_ticks"],
                    config["tick_size"],
                    config["exclude_origin_tick"],
                )
        except BaseException:
            options.free()
            raise
        handle = engine_call(context.liveCreateCoordinates if context is not None else _plot._create_coordinates, options)
        self._coordinate_decimal_places = (
            _coordinate_decimal_places(None if x_range is None else x_range_values),
            _coordinate_decimal_places(None if y_range is None else y_range_values),
        )
        members = [_lines(engine_call(handle.numberPlanePart, i)) for i in (0, 1)]
        x_axis = _plot._attach_number_line(
            object.__new__(_plot.NumberLine), engine_call(handle.numberPlanePart, 2),
            tick_size=x_axis_options["tick_size"],
            include_ticks=x_axis_options["include_ticks"],
            exclude_origin_tick=x_axis_options["exclude_origin_tick"],
        )
        y_axis = _plot._attach_number_line(
            object.__new__(_plot.NumberLine), engine_call(handle.numberPlanePart, 3),
            tick_size=y_axis_options["tick_size"],
            include_ticks=y_axis_options["include_ticks"],
            exclude_origin_tick=y_axis_options["exclude_origin_tick"],
        )
        x_axis.scaling = y_axis.scaling = _plot.LinearBase()
        members.extend((x_axis, y_axis))
        _plot._family(self, handle, members)
        self.axes = _plot._compat.VGroup(x_axis, y_axis)
        self.x_lines = _parallel_grid_lines(self.background_lines, x_axis)
        self.y_lines = _parallel_grid_lines(self.background_lines, y_axis)
        for axis_index, (axis, config, axis_range) in enumerate((
            (x_axis, x_options, x_range_values),
            (y_axis, y_options, y_range_values),
        )):
            if config["decimal_places"] is None:
                config["decimal_places"] = self._coordinate_decimal_places[axis_index]
            _plot._configure_axes_numeric_labels(
                axis, config, axis_range
            )

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

    def add_coordinates(self, *axes_numbers,
                        x_values=_plot._COORDINATE_ARGUMENT_UNSET,
                        y_values=_plot._COORDINATE_ARGUMENT_UNSET, **kwargs):
        if (x_values is not _plot._COORDINATE_ARGUMENT_UNSET
                or y_values is not _plot._COORDINATE_ARGUMENT_UNSET):
            if axes_numbers:
                raise TypeError("use positional axis values or x_values/y_values, not both")
            axes_numbers = (
                None if x_values is _plot._COORDINATE_ARGUMENT_UNSET else x_values,
                None if y_values is _plot._COORDINATE_ARGUMENT_UNSET else y_values,
            )
        from _manim_number_labels import add_number_plane_coordinates
        x_config = dict(kwargs.pop("x_config", None) or {})
        y_config = dict(kwargs.pop("y_config", None) or {})
        if "direction" not in kwargs:
            x_config.setdefault("direction", _base.DR)
            y_config.setdefault("direction", _base.DR)
        kwargs.setdefault("font_size", 24)
        kwargs.setdefault("buff", _base.SMALL_BUFF)
        return add_number_plane_coordinates(
            self,
            *axes_numbers,
            x_config=x_config,
            y_config=y_config,
            config=kwargs,
        )


__all__ = ["NumberPlane"]
