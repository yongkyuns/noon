"""Native Text numeric labels; Rust owns preparation and atomic attachment."""
from __future__ import annotations

import noon as _base
import _manim_compat as _compat
import _manim_plotting as _plot
import _manim_semantic_handles as _shared
from _noon_errors import engine_call


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
    _cold_labels()
    x_axis, y_axis = axes.x_axis, axes.y_axis
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
    x_handle, y_handle = engine_call(axes._semantic_family_handle.coordinateLabelFamilies,
                                   x_values_js, x_values is None, y_values_js, y_values is None,
                                   x_options, y_options)
    _remember(x_axis, _family(x_handle, x_size, x_color))
    _remember(y_axis, _family(y_handle, y_size, y_color))
    return axes
