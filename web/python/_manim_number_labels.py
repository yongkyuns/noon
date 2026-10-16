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
    from js import noonNumberPlaneDecimalCoordinateLabelFamily as _plane_decimal_label
    from js import noonNumberLineDecimalNumberLabelFamily as _line_decimal_labels
    from js import noonAxesDecimalCoordinateLabelFamilies as _axes_decimal_labels
    from js import noonDecimalNumberLabelMembers as _decimal_members
except ImportError:
    _plane_decimal_labels = _plane_decimal_label = None
    _line_decimal_labels = _axes_decimal_labels = _decimal_members = None


def _cold_labels():
    _plot._outside_callback()
    if _plot._coordinate_options is None:
        raise RuntimeError("numeric labels require the shared Rust authoring host")
    if _shared._live_constructor_context("numeric labels") is not None:
        raise NotImplementedError("construct numeric label families before the first play/wait")


def _options(config, *, excluding=()):
    config = dict(config)
    if "num_decimal_places" in config:
        places = config.pop("num_decimal_places")
        if "decimal_places" in config and config["decimal_places"] != places:
            raise ValueError("decimal_places and num_decimal_places disagree")
        config.setdefault("decimal_places", places)
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
    return _plot._family(object.__new__(_compat.Group), handle, members)


def _decimal_family(handle, color):
    from _manim_numbers import DecimalNumber

    if _decimal_members is None:
        raise RuntimeError("DecimalNumber labels require the shared Rust authoring host")
    members = [DecimalNumber._from_numeric_handle(
        member, None, color=color, presentation_applied=True,
    )
               for member in engine_call(_decimal_members, handle)]
    return _plot._family(object.__new__(_compat.Group), handle, members)


def _remember(axis, labels):
    # Only register aliases to the new Rust-owned family. Membership and order
    # have already committed in Rust; Python does not repeat the attachment.
    axis._semantic_member_wrappers[_shared._family_wrapper_key(labels)] = labels
    axis.numbers = labels


def number_mobjects(axis, values, *, attach, config, excluding=None):
    _cold_labels()
    native_text = "font" in config
    values_js = _plot._array(() if values is None else values)
    options, size, color = _options(
        config, excluding=() if excluding is None else excluding,
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
    """Attach coordinate labels using ManimCE's positional per-axis contract.

    No positional values selects default labels on both axes. Otherwise each
    value configures only its corresponding axis; a mapping creates custom
    native-Text labels and ``None`` selects that axis' defaults.
    """
    config = dict(config)
    if "excluding" in config:
        config["excluding"] = _validated_exclusion_values(config["excluding"])
    x_axis, y_axis = axes.x_axis, axes.y_axis
    values = (None, None) if not axes_numbers else axes_numbers[:2]

    axis_configs = []
    for axis_config in (x_config, y_config):
        settings = dict(config)
        settings.update({} if axis_config is None else axis_config)
        axis_configs.append(settings)
    axis_exclusions = [
        _coordinate_exclusions(axis, settings)
        for axis, settings in zip((x_axis, y_axis), axis_configs, strict=True)
    ]

    # Keep same-profile two-axis construction atomic in Rust. A font explicitly
    # selects Noon’s native-Text extension; ManimCE numeric defaults use retained
    # DecimalNumber resources from the prepared TeX backend.
    if len(values) == 2 and not any(isinstance(value, Mapping) for value in values):
        native_profiles = ["font" in settings for settings in axis_configs]
        if all(native_profiles):
            result = _add_coordinate_labels(
                axes, "coordinateLabelFamilies", values[0], values[1],
                x_config=x_config, y_config=y_config, config=config,
                excluded_numbers=axis_exclusions,
            )
        elif not any(native_profiles):
            result = _add_axes_decimal_coordinate_labels(
                axes, values[0], values[1],
                x_config=x_config, y_config=y_config, config=config,
                excluded_numbers=axis_exclusions,
            )
        else:
            return _add_mixed_coordinate_labels(
                axes, values, axis_configs, axis_exclusions,
            )
        axes.coordinate_labels = _compat.VGroup(x_axis.numbers, y_axis.numbers)
        return result

    families = []
    for index, value in enumerate(values):
        axis = (x_axis, y_axis)[index]
        options = axis_configs[index]
        if isinstance(value, Mapping):
            # NumberLine.add_labels owns its Manim-compatible option defaults
            # and rejects custom TeX constructors in the native-Text profile.
            axis.add_labels(value, **options)
            families.append(axis.labels)
        else:
            # An omitted positional axis is not passed here; only explicit
            # None asks ManimCE to create that axis' default labels.
            axis.add_numbers(value, excluding=axis_exclusions[index], **options)
            families.append(axis.numbers)

    axes.coordinate_labels = _compat.VGroup(*families)
    return axes


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


def _add_mixed_coordinate_labels(axes, values, axis_configs, axis_exclusions):
    families = []
    for index, value in enumerate(values):
        axis = (axes.x_axis, axes.y_axis)[index]
        options = axis_configs[index]
        if isinstance(value, Mapping):
            axis.add_labels(value, **options)
            families.append(axis.labels)
        else:
            axis.add_numbers(value, excluding=axis_exclusions[index], **options)
            families.append(axis.numbers)
    axes.coordinate_labels = _compat.VGroup(*families)
    return axes


def _add_axes_decimal_coordinate_labels(
    axes, x_values, y_values, *, x_config, y_config, config, excluded_numbers,
):
    _cold_labels()
    if _axes_decimal_labels is None:
        raise RuntimeError("Axes DecimalNumber labels require await prepare_latex()")
    x_settings = dict(config)
    y_settings = dict(config)
    x_settings.setdefault("direction", _base.DOWN)
    y_settings.setdefault("direction", _base.LEFT)
    x_settings.update({} if x_config is None else x_config)
    y_settings.update({} if y_config is None else y_config)
    for settings in (x_settings, y_settings):
        settings.pop("excluding", None)
        settings.setdefault("exclude_zero", False)
    x_options, _, x_color = _options(x_settings, excluding=excluded_numbers[0])
    try:
        y_options, _, y_color = _options(y_settings, excluding=excluded_numbers[1])
    except BaseException:
        x_options.free()
        raise
    handles = engine_call(
        _axes_decimal_labels,
        axes._semantic_family_handle,
        _plot._array(() if x_values is None else x_values),
        x_values is None,
        _plot._array(() if y_values is None else y_values),
        y_values is None,
        x_options,
        y_options,
    )
    _remember(axes.x_axis, _decimal_family(handles[0], x_color))
    _remember(axes.y_axis, _decimal_family(handles[1], y_color))
    return axes


def add_number_plane_coordinates(plane, *axes_numbers, x_config=None, y_config=None, config):
    """Add NumberPlane labels with positional per-axis ManimCE semantics."""
    config = dict(config)
    if "excluding" in config:
        config["excluding"] = _validated_exclusion_values(config["excluding"])
    values = (None, None) if not axes_numbers else axes_numbers[:2]
    x_config = dict(x_config or {})
    y_config = dict(y_config or {})
    axis_configs = (x_config, y_config)
    native_profile = "font" in config or any("font" in axis for axis in axis_configs)
    atomic_native_profile = (
        "font" in config
        or all("font" in axis for axis in axis_configs)
    )
    axis_configs = [dict(config), dict(config)]
    axis_configs[0].update(x_config)
    axis_configs[1].update(y_config)
    axis_exclusions = [
        _coordinate_exclusions(axis, settings)
        for axis, settings in zip((plane.x_axis, plane.y_axis), axis_configs, strict=True)
    ]

    # Preserve atomic DecimalNumber construction for the ordinary two-axis path.
    if len(values) == 2 and (not native_profile or atomic_native_profile) and not any(
        isinstance(value, Mapping) for value in values
    ):
        if atomic_native_profile:
            result = _add_coordinate_labels(
                plane, "numberPlaneCoordinateLabelFamilies", values[0], values[1],
                x_config=x_config, y_config=y_config, config=config,
                excluded_numbers=axis_exclusions,
            )
        else:
            result = add_number_plane_decimal_coordinates(
                plane, values[0], values[1], x_config=x_config,
                y_config=y_config, config=config,
                excluded_numbers=axis_exclusions,
            )
        plane.coordinate_labels = _compat.VGroup(plane.x_axis.numbers, plane.y_axis.numbers)
        return result

    families = []
    for index, value in enumerate(values):
        axis = (plane.x_axis, plane.y_axis)[index]
        options = axis_configs[index]
        if isinstance(value, Mapping):
            axis.add_labels(value, **options)
            families.append(axis.labels)
        elif "font" in options:
            axis.add_numbers(value, excluding=axis_exclusions[index], **options)
            families.append(axis.numbers)
        else:
            families.append(_add_number_plane_decimal_coordinate_axis(
                plane, index, value, config=options,
                excluded_numbers=axis_exclusions[index],
            ))

    plane.coordinate_labels = _compat.VGroup(*families)
    return plane


def add_number_plane_decimal_coordinates(
    plane, x_values, y_values, *, x_config, y_config, config, excluded_numbers,
):
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
        settings.pop("excluding", None)
        settings.setdefault("exclude_zero", False)
        if "num_decimal_places" in settings:
            places = settings.pop("num_decimal_places")
            if "decimal_places" in settings and settings["decimal_places"] != places:
                raise ValueError("decimal_places and num_decimal_places disagree")
            settings.setdefault("decimal_places", places)
    precision = getattr(plane, "_coordinate_decimal_places", (1, 1))
    x_settings.setdefault("decimal_places", precision[0])
    y_settings.setdefault("decimal_places", precision[1])
    x_options, x_size, x_color = _options(x_settings, excluding=excluded_numbers[0])
    try:
        y_options, y_size, y_color = _options(y_settings, excluding=excluded_numbers[1])
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


def _add_number_plane_decimal_coordinate_axis(
    plane, axis_index, values, *, config, excluded_numbers,
):
    _cold_labels()
    if _plane_decimal_label is None or _decimal_members is None:
        raise RuntimeError("NumberPlane DecimalNumber labels require await prepare_latex()")
    settings = dict(config)
    settings.pop("excluding", None)
    settings.setdefault("exclude_zero", False)
    if "num_decimal_places" in settings:
        places = settings.pop("num_decimal_places")
        if "decimal_places" in settings and settings["decimal_places"] != places:
            raise ValueError("decimal_places and num_decimal_places disagree")
        settings.setdefault("decimal_places", places)
    precision = plane._coordinate_decimal_places[axis_index]
    settings.setdefault("decimal_places", precision)
    options, _, color = _options(settings, excluding=excluded_numbers)
    family_handle = engine_call(
        _plane_decimal_label,
        plane._semantic_family_handle,
        axis_index,
        _plot._array(() if values is None else values),
        values is None,
        options,
    )
    family = _decimal_family(family_handle, color)
    _remember((plane.x_axis, plane.y_axis)[axis_index], family)
    return family


def _add_coordinate_labels(
    owner, method_name, x_values, y_values, *, x_config, y_config, config,
    excluded_numbers,
):
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
    for settings in (x_settings, y_settings):
        settings.pop("excluding", None)
        settings.setdefault("exclude_zero", False)
    x_options, x_size, x_color = _options(x_settings, excluding=excluded_numbers[0])
    try:
        y_options, y_size, y_color = _options(y_settings, excluding=excluded_numbers[1])
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
