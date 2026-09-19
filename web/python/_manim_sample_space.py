"""Manim SampleSpace syntax over shared Rust rectangles, families, and layout."""

from __future__ import annotations

from collections.abc import Iterable, Sequence

from _noon_errors import engine_call

import noon as _base
import _manim_compat as _compat
import _manim_semantic_handles as _shared

try:
    from js import (
        noonAuthoringSampleSpaceOptions as _sample_space_options,
        noonCreateAuthoringSampleSpaceHandle as _create_sample_space,
    )
    from pyodide.ffi import to_js as _to_js
except ImportError:
    _sample_space_options = _create_sample_space = _to_js = None


_DEFAULT_FILL = _base.color_from_hex("#525252")
_DEFAULT_STROKE = _base.color_from_hex("#BBBBBB")
_DEFAULT_HORIZONTAL_COLORS = (_base.GREEN_E, _base.BLUE_E)
_DEFAULT_VERTICAL_COLORS = (_base.color_from_hex("#EC92AB"), _base.YELLOW)


def _array(values):
    if _to_js is None:
        raise RuntimeError("SampleSpace requires the shared Rust authoring host")
    return _to_js([float(value) for value in values])


def _probabilities(values):
    if isinstance(values, (int, float)) and not isinstance(values, bool):
        return [float(values)]
    if isinstance(values, (str, bytes)):
        raise TypeError("probabilities must be a number or an iterable of numbers")
    try:
        return [float(value) for value in values]
    except TypeError as error:
        raise TypeError("probabilities must be a number or an iterable of numbers") from error


def _colors(values):
    parsed = [_compat._as_color("color", value) for value in values]
    if not parsed:
        raise ValueError("SampleSpace partitions require at least one color")
    return [component for color in parsed
            for component in (color.red, color.green, color.blue, color.alpha)]


def _leaf(handle):
    wrapper = object.__new__(_compat.Rectangle)
    _shared._attach_shared_handle(wrapper, handle)
    return wrapper


def _family_group(handle):
    wrapper = object.__new__(_compat.Group)
    count = int(engine_call(lambda: handle.memberCount, operation="SampleSpace.parts"))
    members = [
        _leaf(engine_call(handle.memberMobject, index, operation="SampleSpace.parts"))
        for index in range(count)
    ]
    wrapper._semantic_family_handle = handle
    wrapper._semantic_member_wrappers = {
        _shared._family_wrapper_key(member): member for member in members
    }
    return wrapper


def _constructor_context():
    if _sample_space_options is None or _create_sample_space is None:
        raise RuntimeError("SampleSpace requires the shared Rust authoring host")
    context = _shared._live_constructor_context("SampleSpace")
    if context is not None:
        raise NotImplementedError(
            "SampleSpace construction after live execution starts is not yet exposed"
        )


def _partition_context(sample_space):
    if _shared._live_mutation_context(sample_space) is not None:
        raise NotImplementedError(
            "SampleSpace partitions after live execution starts are not yet exposed"
        )


class SampleSpace(_compat.Group):
    """A two-dimensional sampling space composed from retained rectangles.

    Rust owns the rectangle bounds, probability completion, color gradient,
    partition geometry, family order, and semantic identities. Python retains
    only Manim's public call shape and wrapper objects.
    """

    def __init__(
        self,
        height: float = 3.0,
        width: float = 3.0,
        fill_color: object = _DEFAULT_FILL,
        fill_opacity: float = 1.0,
        stroke_width: float = 0.5,
        stroke_color: object = _DEFAULT_STROKE,
        default_label_scale_val: float = 1.0,
    ) -> None:
        _constructor_context()
        fill = _compat._as_color("fill_color", fill_color)
        stroke = _compat._as_color("stroke_color", stroke_color)
        options = engine_call(
            _sample_space_options.new,
            float(width),
            float(height),
            operation="SampleSpace",
        )
        engine_call(options.setFillColor, fill.red, fill.green, fill.blue,
                    operation="SampleSpace")
        engine_call(options.setFillOpacity, float(fill_opacity), operation="SampleSpace")
        engine_call(options.setStrokeColor, stroke.red, stroke.green, stroke.blue,
                    operation="SampleSpace")
        engine_call(options.setStrokeWidth, float(stroke_width), operation="SampleSpace")
        handle = engine_call(_create_sample_space, options, operation="SampleSpace")

        rectangle = _leaf(engine_call(handle.rectangle, operation="SampleSpace.rectangle"))
        family = engine_call(handle.family, operation="SampleSpace.family")
        self._semantic_family_handle = family
        self._semantic_member_wrappers = {
            _shared._family_wrapper_key(rectangle): rectangle,
        }
        self._sample_space_handle = handle
        self._sample_space_part_wrappers = {}
        self.default_label_scale_val = float(default_label_scale_val)

    def complete_p_list(self, p_list: float | Iterable[float]) -> list[float]:
        values = _probabilities(p_list)
        return [float(value) for value in engine_call(
            self._sample_space_handle.completePList,
            _array(values),
            operation="SampleSpace.complete_p_list",
        )]

    def _parts(self, kind: str, p_list, colors):
        _partition_context(self)
        probabilities = _probabilities(p_list)
        color_values = _colors(colors)
        family = engine_call(
            getattr(self._sample_space_handle, kind),
            _array(probabilities),
            _array(color_values),
            operation=f"SampleSpace.{kind}",
        )
        key = _family_key(family)
        existing = self._sample_space_part_wrappers.get(key)
        if existing is not None:
            wrapper = existing
        else:
            wrapper = _family_group(family)
            self._sample_space_part_wrappers[key] = wrapper
        return wrapper

    def _divide(self, kind: str, p_list, colors):
        _partition_context(self)
        probabilities = _probabilities(p_list)
        color_values = _colors(colors)
        family = engine_call(
            getattr(self._sample_space_handle, kind),
            _array(probabilities),
            _array(color_values),
            operation=f"SampleSpace.{kind}",
        )
        wrapper = self._sample_space_part_wrappers.get(_family_key(family))
        if wrapper is None:
            wrapper = _family_group(family)
            self._sample_space_part_wrappers[_family_key(family)] = wrapper
        self._semantic_member_wrappers[_family_key(family)] = wrapper
        # The property reads the latest Rust-owned partition identity. This
        # assignment only records wrapper identity for Scene membership.
        return self

    def get_horizontal_division(
        self,
        p_list: float | Iterable[float],
        colors: Sequence[object] = _DEFAULT_HORIZONTAL_COLORS,
        vect: object = _base.DOWN,
    ) -> _compat.Group:
        _validate_direction(vect, vertical=False)
        return self._parts("getHorizontalDivision", p_list, colors)

    def get_vertical_division(
        self,
        p_list: float | Iterable[float],
        colors: Sequence[object] = _DEFAULT_VERTICAL_COLORS,
        vect: object = _base.RIGHT,
    ) -> _compat.Group:
        _validate_direction(vect, vertical=True)
        return self._parts("getVerticalDivision", p_list, colors)

    def divide_horizontally(
        self,
        p_list: float | Iterable[float],
        colors: Sequence[object] = _DEFAULT_HORIZONTAL_COLORS,
        vect: object = _base.DOWN,
    ) -> SampleSpace:
        _validate_direction(vect, vertical=False)
        return self._divide("divideHorizontally", p_list, colors)

    def divide_vertically(
        self,
        p_list: float | Iterable[float],
        colors: Sequence[object] = _DEFAULT_VERTICAL_COLORS,
        vect: object = _base.RIGHT,
    ) -> SampleSpace:
        _validate_direction(vect, vertical=True)
        return self._divide("divideVertically", p_list, colors)

    @property
    def horizontal_parts(self):
        family = engine_call(
            self._sample_space_handle.horizontalParts,
            operation="SampleSpace.horizontal_parts",
        )
        return None if family is None else self._wrap_parts_query(family)

    @property
    def vertical_parts(self):
        family = engine_call(
            self._sample_space_handle.verticalParts,
            operation="SampleSpace.vertical_parts",
        )
        return None if family is None else self._wrap_parts_query(family)

    def _wrap_parts_query(self, family):
        key = _family_key(family)
        wrapper = self._sample_space_part_wrappers.get(key)
        if wrapper is None:
            wrapper = _family_group(family)
            self._sample_space_part_wrappers[key] = wrapper
        self._semantic_member_wrappers[key] = wrapper
        return wrapper

    def get_subdivision_braces_and_labels(
        self,
        parts: _compat.Group,
        labels: list[str | _base.Mobject],
        direction: object,
        buff: float = _base.SMALL_BUFF,
        min_num_quads: int = 1,
    ) -> _compat.Group:
        """Build Manim's brace and MathTex annotations for a partition family."""
        _partition_context(self)
        if not isinstance(parts, _compat.Group):
            raise TypeError("parts must be a SampleSpace partition family")
        if not isinstance(labels, list):
            raise TypeError("labels must be a list of strings or Mobjects")
        if isinstance(min_num_quads, bool) or min_num_quads != 1:
            raise NotImplementedError("SampleSpace subdivision braces support min_num_quads=1")
        direction_value = _base._as_vec2(direction)
        buff_value = _shared._ir._finite_number("buff", buff)

        braces = []
        label_mobs = []
        for label, part in zip(labels, parts, strict=False):
            brace = _base.Brace(part, direction_value, buff=buff_value)
            if isinstance(label, _base.Mobject):
                label_mob = label
            elif isinstance(label, str):
                label_mob = _base.MathTex(label)
                label_mob.scale(self.default_label_scale_val)
            else:
                raise TypeError("labels must contain strings or Mobjects")
            label_mob.next_to(brace, direction_value, buff_value)
            braces.append(brace)
            label_mobs.append(label_mob)

        parts.braces = _compat.VGroup(*braces)
        parts.labels = _compat.VGroup(*label_mobs)
        parts.label_kwargs = {
            "labels": parts.labels.copy(),
            "direction": direction_value,
            "buff": buff_value,
        }
        return _compat.VGroup(parts.braces, parts.labels)

    def get_side_braces_and_labels(
        self,
        labels: list[str | _base.Mobject],
        direction: object = _base.LEFT,
        **kwargs,
    ) -> _compat.Group:
        parts = self.horizontal_parts
        if parts is None:
            raise AssertionError("divide_horizontally must be called first")
        return self.get_subdivision_braces_and_labels(parts, labels, direction, **kwargs)

    def get_top_braces_and_labels(
        self, labels: list[str | _base.Mobject], **kwargs
    ) -> _compat.Group:
        parts = self.vertical_parts
        if parts is None:
            raise AssertionError("divide_vertically must be called first")
        return self.get_subdivision_braces_and_labels(parts, labels, _base.UP, **kwargs)

    def get_bottom_braces_and_labels(
        self, labels: list[str | _base.Mobject], **kwargs
    ) -> _compat.Group:
        parts = self.vertical_parts
        if parts is None:
            raise AssertionError("divide_vertically must be called first")
        return self.get_subdivision_braces_and_labels(parts, labels, _base.DOWN, **kwargs)

    def add_braces_and_labels(self) -> SampleSpace:
        _partition_context(self)
        for parts in (self.horizontal_parts, self.vertical_parts):
            if parts is None:
                continue
            for name in ("braces", "labels"):
                annotated = getattr(parts, name, None)
                if annotated is not None:
                    self.add(annotated)
        return self

    def __getitem__(self, index):
        parts = self.horizontal_parts
        if parts is not None:
            return parts[index]
        parts = self.vertical_parts
        if parts is not None:
            return parts[index]
        return super().__getitem__(index)


def _family_key(family) -> str:
    return f"{int(family.semanticSlot)}:{int(family.semanticGeneration)}"


def _validate_direction(value, *, vertical: bool):
    direction = _base._as_vec2(value)
    valid = direction.y == 0.0 and direction.x > 0.0 if vertical else (
        direction.x == 0.0 and direction.y < 0.0
    )
    if not valid:
        label = "RIGHT" if vertical else "DOWN"
        raise NotImplementedError(f"SampleSpace partition direction must be {label}")


__all__ = ["SampleSpace"]
