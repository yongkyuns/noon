"""Rust-prepared native Text and DecimalNumber coordinate label families."""
from __future__ import annotations

import noon as _base
import _manim_compat as _compat
import _manim_plotting as _plot
import _manim_semantic_handles as _shared
from _noon_errors import engine_call

try:
    from js import noonNumberPlaneDecimalCoordinateLabelFamilies as _plane_decimal_labels
except ImportError:
    _plane_decimal_labels = None


def _cold_labels():
    _plot._outside_callback()
    if _plot._coordinate_options is None:
        raise RuntimeError("numeric labels require the shared Rust authoring host")
    if _shared._live_constructor_context("numeric labels") is not None:
        raise NotImplementedError("construct numeric label families before the first play/wait")


def _options(config):
    config = dict(config)
    font = config.pop("font", "DejaVu Sans Mono")
    size = float(config.pop("font_size", 18))
    precision = config.pop("decimal_places", 0)
    direction = _base._as_vec2(config.pop("direction", _base.DOWN))
    buff = float(config.pop("buff", 0.12))
    exclude = bool(config.pop("exclude_zero", True))
    color = _shared._constructor_color("label color", config.pop("color", _base.WHITE))
    if config:
        raise TypeError("unsupported native numeric-label option(s): " + ", ".join(sorted(config)))
    if not isinstance(font, str) or not font.strip():
        raise TypeError("font must be a non-empty string")
    if not isinstance(precision, int) or isinstance(precision, bool):
        raise TypeError("decimal_places must be an integer")
    options = engine_call(_plot._coordinate_options.numberLabels, font, size, precision, buff)
    try:
        engine_call(options.setDirection, direction.x, direction.y)
        engine_call(options.setExcludeZero, exclude)
        engine_call(options.setColor, color.red, color.green, color.blue, color.alpha)
    except BaseException:
        options.free()
        raise
    return options, size, color


def _family(handle, size, color):
    members = []
    for object_handle, source in engine_call(handle.numberLabelMembers):
        wrapper = object.__new__(_base.Text)
        wrapper._initialize_text(str(source), size, object_handle, color, 1.0,
                                 presentation_applied=True)
        members.append(wrapper)
    return _plot._family(object.__new__(_compat.Group), handle, members)


def _decimal_family(handle, color):
    from _manim_numbers import DecimalNumber

    members = [DecimalNumber._from_numeric_handle(
        member, None, color=color, presentation_applied=True,
    )
               for member in engine_call(handle.decimalNumberLabelMembers)]
    return _plot._family(object.__new__(_compat.Group), handle, members)


def _remember(axis, labels):
    # Only register aliases to the new Rust-owned family. Membership and order
    # have already committed in Rust; Python does not repeat the attachment.
    axis._semantic_member_wrappers[_shared._family_wrapper_key(labels)] = labels
    axis.numbers = labels


def number_mobjects(axis, values, *, attach, config):
    _cold_labels()
    values_js = _plot._array(() if values is None else values)
    options, size, color = _options(config)
    handle = engine_call(axis._semantic_family_handle.numberLabelFamily,
                         values_js, values is None, options, bool(attach))
    labels = _family(handle, size, color)
    if attach:
        _remember(axis, labels)
    return labels


def add_coordinates(axes, x_values, y_values, *, x_config, y_config, config):
    return _add_coordinate_labels(
        axes, "coordinateLabelFamilies", x_values, y_values,
        x_config=x_config, y_config=y_config, config=config,
    )


def add_number_plane_coordinates(plane, x_values, y_values, *, x_config, y_config, config):
    # An explicit native font opts into the retained native-Text profile.
    x_config = dict(x_config or {})
    y_config = dict(y_config or {})
    axis_fonts = [axis.get("font") for axis in (x_config, y_config)]
    if "font" in config:
        return _add_coordinate_labels(
            plane, "numberPlaneCoordinateLabelFamilies", x_values, y_values,
            x_config=x_config, y_config=y_config, config=config,
        )
    if any(font is not None for font in axis_fonts):
        if any(font is None for font in axis_fonts):
            raise NotImplementedError("per-axis native font requires fonts for both NumberPlane axes")
        return _add_coordinate_labels(
            plane, "numberPlaneCoordinateLabelFamilies", x_values, y_values,
            x_config=x_config, y_config=y_config, config=config,
        )
    return add_number_plane_decimal_coordinates(
        plane, x_values, y_values, x_config=x_config, y_config=y_config, config=config,
    )


def add_number_plane_decimal_coordinates(plane, x_values, y_values, *, x_config, y_config, config):
    _cold_labels()
    if _plane_decimal_labels is None:
        raise RuntimeError("NumberPlane DecimalNumber labels require await prepare_latex()")
    x_values_js = _plot._array(() if x_values is None else x_values)
    y_values_js = _plot._array(() if y_values is None else y_values)
    x_settings = dict(config)
    y_settings = dict(config)
    x_settings.setdefault("direction", _base.DR)
    y_settings.setdefault("direction", _base.DR)
    x_settings.update({} if x_config is None else x_config)
    y_settings.update({} if y_config is None else y_config)
    for settings in (x_settings, y_settings):
        if "num_decimal_places" in settings:
            places = settings.pop("num_decimal_places")
            if "decimal_places" in settings and settings["decimal_places"] != places:
                raise ValueError("decimal_places and num_decimal_places disagree")
            settings.setdefault("decimal_places", places)
    precision = getattr(plane, "_coordinate_decimal_places", (1, 1))
    x_settings.setdefault("decimal_places", precision[0])
    y_settings.setdefault("decimal_places", precision[1])
    x_options, x_size, x_color = _options(x_settings)
    try:
        y_options, y_size, y_color = _options(y_settings)
    except BaseException:
        x_options.free()
        raise
    handles = engine_call(
        _plane_decimal_labels,
        plane._semantic_family_handle,
        x_values_js, x_values is None,
        y_values_js, y_values is None,
        x_options, y_options,
    )
    _remember(plane.x_axis, _decimal_family(handles[0], x_color))
    _remember(plane.y_axis, _decimal_family(handles[1], y_color))
    return plane


def _add_coordinate_labels(owner, method_name, x_values, y_values, *, x_config, y_config, config):
    _cold_labels()
    x_axis, y_axis = owner.x_axis, owner.y_axis
    x_values_js = _plot._array(() if x_values is None else x_values)
    y_values_js = _plot._array(() if y_values is None else y_values)
    x_settings = dict(config)
    y_settings = dict(config)
    x_settings.setdefault("direction", _base.DOWN)
    y_settings.setdefault("direction", _base.LEFT)
    x_settings.update({} if x_config is None else x_config)
    y_settings.update({} if y_config is None else y_config)
    x_options, x_size, x_color = _options(x_settings)
    try:
        y_options, y_size, y_color = _options(y_settings)
    except BaseException:
        x_options.free()
        raise
    # Both inert option handles are consumed by this one Rust call, including
    # its error path. Do not free their moved JS proxies a second time.
    handles = engine_call(
        getattr(owner._semantic_family_handle, method_name),
        x_values_js, x_values is None,
        y_values_js, y_values is None,
        x_options, y_options,
    )
    _remember(x_axis, _family(handles[0], x_size, x_color))
    _remember(y_axis, _family(handles[1], y_size, y_color))
    return owner
