"""Rust-prepared native Text and DecimalNumber coordinate label families."""
from __future__ import annotations

from collections.abc import Mapping

import noon as _base
import _manim_compat as _compat
import _manim_plotting as _plot
import _manim_semantic_handles as _shared
from _noon_errors import engine_call

try:
    from js import noonNumberPlaneDecimalCoordinateLabelFamilies as _plane_decimal_labels
    from js import noonNumberLineDecimalNumberLabelFamily as _line_decimal_labels
    from js import noonAxesDecimalCoordinateLabelFamilies as _axes_decimal_labels
    from js import noonDecimalNumberLabelMembers as _decimal_members
except ImportError:
    _plane_decimal_labels = None
    _line_decimal_labels = _axes_decimal_labels = _decimal_members = None


def _cold_labels():
    _plot._outside_callback()
    if _plot._coordinate_options is None:
        raise RuntimeError("numeric labels require the shared Rust authoring host")
    if _shared._live_constructor_context("numeric labels") is not None:
        raise NotImplementedError("construct numeric label families before the first play/wait")


def _merge_label_config(*layers):
    """Normalize aliases within each option layer before applying overrides."""
    config = {}
    for layer in layers:
        layer = dict(layer)
        if "num_decimal_places" in layer:
            places = layer.pop("num_decimal_places")
            if "decimal_places" in layer and layer["decimal_places"] != places:
                raise ValueError("decimal_places and num_decimal_places disagree")
            layer.setdefault("decimal_places", places)
        config.update(layer)
    return config


def _options(config, *, excluding=()):
    config = _merge_label_config(config)
    font = config.pop("font", "DejaVu Sans Mono")
    size = float(config.pop("font_size", 18))
    precision = config.pop("decimal_places", 0)
    direction = _base._as_vec2(config.pop("direction", _base.DOWN))
    buff = float(config.pop("buff", 0.12))
    exclude = bool(config.pop("exclude_zero", True))
    include_sign = bool(config.pop("include_sign", False))
    group_with_commas = bool(config.pop("group_with_commas", True))
    show_ellipsis = bool(config.pop("show_ellipsis", False))
    unit = config.pop("unit", None)
    if unit is not None and not isinstance(unit, str):
        raise TypeError("unit must be a string or None")
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
        engine_call(options.setIncludeSign, include_sign)
        engine_call(options.setGroupWithCommas, group_with_commas)
        engine_call(options.setShowEllipsis, show_ellipsis)
        engine_call(options.setUnit, unit)
        engine_call(options.setExcludedNumbers, _plot._array(excluding))
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
    return _plot._family(object.__new__(_compat.VGroup), handle, members)


def _decimal_family(handle, color):
    from _manim_numbers import DecimalNumber

    if _decimal_members is None:
        raise RuntimeError("DecimalNumber labels require the shared Rust authoring host")
    members = [DecimalNumber._from_numeric_handle(
        member, None, color=color, presentation_applied=True,
    )
               for member in engine_call(_decimal_members, handle)]
    return _plot._family(object.__new__(_compat.VGroup), handle, members)


def _remember(axis, labels):
    # Only register aliases to the new Rust-owned family. Membership and order
    # have already committed in Rust; Python does not repeat the attachment.
    axis._semantic_member_wrappers[_shared._family_wrapper_key(labels)] = labels
    axis.numbers = labels


def _axis_options(axis, config, *, excluding=()):
    options, size, color = _options(config, excluding=excluding)
    try:
        engine_call(options.setTickRangeFlags,
                    bool(getattr(axis, "include_tip", False)),
                    bool(getattr(axis, "exclude_origin_tick", False)))
    except BaseException:
        options.free()
        raise
    return options, size, color


def number_mobjects(axis, values, *, attach, config, excluding=None):
    _cold_labels()
    native_text = "font" in config
    values_js = _plot._array(() if values is None else values)
    options, size, color = _axis_options(
        axis, config, excluding=() if excluding is None else excluding,
    )
    if native_text:
        handle = engine_call(axis._semantic_family_handle.numberLabelFamily,
                             values_js, values is None, options, bool(attach))
        labels = _family(handle, size, color)
    else:
        if _line_decimal_labels is None:
            options.free()
            raise RuntimeError("NumberLine DecimalNumber labels require await prepare_latex()")
        handle = engine_call(
            _line_decimal_labels,
            axis._semantic_family_handle,
            values_js,
            values is None,
            options,
            bool(attach),
        )
        labels = _decimal_family(handle, color)
    if attach:
        _remember(axis, labels)
    return labels


def custom_labels(axis, labels, *, config):
    """Attach string/number labels without moving text placement into Python."""
    _cold_labels()
    if not isinstance(labels, Mapping):
        raise TypeError("add_labels requires a mapping from positions to labels")
    positions = []
    texts = []
    for position, label in labels.items():
        positions.append(_base._ir._finite_number("label position", position))
        if isinstance(label, bool):
            raise TypeError("custom labels must be strings or finite numbers")
        if not isinstance(label, str):
            if isinstance(label, (int, float)):
                _base._ir._finite_number("label value", label)
            else:
                raise NotImplementedError(
                    "native NumberLine labels currently accept strings or finite numbers"
                )
        texts.append(str(label))

    values_js = _plot._array(positions)
    texts_js = _plot._to_js(texts)
    options, size, color = _options(config)
    family_handle = engine_call(
        axis._semantic_family_handle.numberLineCustomLabelFamily,
        values_js, texts_js, options,
    )
    family = _family(family_handle, size, color)
    axis._semantic_member_wrappers[_shared._family_wrapper_key(family)] = family
    axis.labels = family
    return family


def add_coordinates(axes, *axes_numbers, x_config=None, y_config=None, config):
    return _coordinate_labels(axes, axes_numbers, (x_config, y_config), config, plane=False)


def add_number_plane_coordinates(plane, *axes_numbers, x_config=None, y_config=None, config):
    return _coordinate_labels(plane, axes_numbers, (x_config, y_config), config, plane=True)


def _coordinate_labels(owner, axes_numbers, overrides, config, *, plane):
    """Coerce per-axis presentation, then forward to existing Rust label operations."""
    config = dict(config)
    if "excluding" in config:
        config["excluding"] = _validated_exclusion_values(config["excluding"])
    values = (None, None) if not axes_numbers else axes_numbers[:2]
    axes = (owner.x_axis, owner.y_axis)
    axis_configs = []
    for index, (axis, override) in enumerate(zip(axes, overrides, strict=True)):
        settings = _merge_label_config(
            getattr(axis, "decimal_number_config", {}),
            config, {} if override is None else override,
        )
        if plane:
            settings.setdefault("direction", _base.DR)
            settings.setdefault("decimal_places",
                                getattr(owner, "_coordinate_decimal_places", (1, 1))[index])
        axis_configs.append(settings)
    axis_exclusions = [
        _coordinate_exclusions(axis, settings)
        for axis, settings in zip(axes, axis_configs, strict=True)
    ]
    native_profiles = ["font" in settings for settings in axis_configs]
    if any(isinstance(value, Mapping) and not native_profiles[index]
           for index, value in enumerate(values)):
        raise NotImplementedError(
            "mapping coordinate labels require an explicit font for native Text; "
            "default TeX mappings are not yet supported"
        )
    if len(values) == 2 and (native_profiles[0] != native_profiles[1]
                            or any(isinstance(value, Mapping) for value in values)):
        raise NotImplementedError(
            "two-axis coordinate labels require numeric labels of one type; "
            "add mixed or custom labels one axis at a time"
        )
    if any(value is None and _plot._number_line_scale(getattr(axes[index], "scaling", None))[2]
           for index, value in enumerate(values)):
        raise NotImplementedError(_plot._UNSUPPORTED_LOG_LABELS)
    if len(values) == 2:
        if native_profiles[0]:
            method = "numberPlaneCoordinateLabelFamilies" if plane else "coordinateLabelFamilies"
            result = _add_coordinate_labels(
                owner, method, *values, axis_settings=axis_configs,
                excluded_numbers=axis_exclusions,
            )
        else:
            result = _add_decimal_coordinate_labels(
                owner, *values, axis_settings=axis_configs, excluded_numbers=axis_exclusions,
                bridge=_plane_decimal_labels if plane else _axes_decimal_labels,
            )
        owner.coordinate_labels = _compat.VGroup(*(axis.numbers for axis in axes))
        return result

    families = []
    for index, value in enumerate(values):
        axis, settings = axes[index], axis_configs[index]
        if isinstance(value, Mapping):
            _add_axis_custom_labels(axis, value, settings)
            families.append(axis.labels)
        elif plane and not native_profiles[index]:
            settings.setdefault("exclude_zero", False)
            families.append(number_mobjects(axis, value, attach=True, config=settings,
                                            excluding=axis_exclusions[index]))
        else:
            axis.add_numbers(value, excluding=axis_exclusions[index], **settings)
            families.append(axis.numbers)
    owner.coordinate_labels = _compat.VGroup(*families)
    return owner


def _coordinate_exclusions(axis, settings):
    if "excluding" in settings:
        values = settings.pop("excluding")
    else:
        values = getattr(axis, "numbers_to_exclude", (0.0,))
    if values is None:
        values = ()
    exclusions = _validated_exclusion_values(values)
    if settings.get("exclude_zero") is False:
        exclusions = tuple(value for value in exclusions if value != 0.0)
    return exclusions


def _validated_exclusion_values(values):
    if values is None:
        return ()
    if isinstance(values, (str, bytes, Mapping)):
        raise TypeError("excluding must be an iterable of finite numbers")
    try:
        values = tuple(values)
    except TypeError as error:
        raise TypeError("excluding must be an iterable of finite numbers") from error
    return tuple(
        _base._ir._finite_number("excluding", value) for value in values
    )


def _add_axis_custom_labels(axis, values, settings):
    """Forward only layout/constructor options accepted by ``add_labels``."""
    options = {
        "direction": settings.get(
            "direction", settings.get(
                "label_direction", getattr(axis, "label_direction", _base.DOWN)
            )
        ),
        "buff": settings.get(
            "buff", settings.get(
                "line_to_number_buff",
                getattr(axis, "line_to_number_buff", _base.MED_SMALL_BUFF),
            )
        ),
        "font_size": settings.get("font_size", getattr(axis, "font_size", 36)),
    }
    if "label_constructor" in settings:
        options["label_constructor"] = settings["label_constructor"]
    options["font"] = settings["font"]
    return custom_labels(axis, values, config=options)


def _add_decimal_coordinate_labels(
    owner, x_values, y_values, *, axis_settings, excluded_numbers, bridge,
):
    _cold_labels()
    if bridge is None:
        raise RuntimeError("DecimalNumber coordinate labels require await prepare_latex()")
    x_settings, y_settings = (dict(settings) for settings in axis_settings)
    x_settings.setdefault("direction", _base.DOWN)
    y_settings.setdefault("direction", _base.LEFT)
    for settings in (x_settings, y_settings):
        settings.pop("excluding", None)
        settings.setdefault("exclude_zero", False)
    x_options, _, x_color = _axis_options(owner.x_axis, x_settings, excluding=excluded_numbers[0])
    try:
        y_options, _, y_color = _axis_options(owner.y_axis, y_settings, excluding=excluded_numbers[1])
    except BaseException:
        x_options.free()
        raise
    handles = engine_call(
        bridge, owner._semantic_family_handle,
        _plot._array(() if x_values is None else x_values), x_values is None,
        _plot._array(() if y_values is None else y_values), y_values is None,
        x_options, y_options,
    )
    _remember(owner.x_axis, _decimal_family(handles[0], x_color))
    _remember(owner.y_axis, _decimal_family(handles[1], y_color))
    return owner


def _add_coordinate_labels(
    owner, method_name, x_values, y_values, *, axis_settings,
    excluded_numbers,
):
    _cold_labels()
    x_axis, y_axis = owner.x_axis, owner.y_axis
    x_values_js = _plot._array(() if x_values is None else x_values)
    y_values_js = _plot._array(() if y_values is None else y_values)
    x_settings = dict(axis_settings[0])
    y_settings = dict(axis_settings[1])
    x_settings.setdefault("direction", _base.DOWN)
    y_settings.setdefault("direction", _base.LEFT)
    for settings in (x_settings, y_settings):
        settings.pop("excluding", None)
        settings.setdefault("exclude_zero", False)
    x_options, x_size, x_color = _axis_options(x_axis, x_settings, excluding=excluded_numbers[0])
    try:
        y_options, y_size, y_color = _axis_options(y_axis, y_settings, excluding=excluded_numbers[1])
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
