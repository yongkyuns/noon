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


def _leaf(handle, context=None):
    wrapper = object.__new__(_compat.Rectangle)
    _shared._attach_shared_handle(wrapper, handle)
    if context is not None:
        wrapper._canonical_live_target_context = context
    return wrapper


def _family_group(handle, wrapper=None, context=None):
    if wrapper is None:
        wrapper = object.__new__(_compat.Group)
    if context is None:
        context = getattr(wrapper, "_canonical_live_target_context", None)
    old_members = getattr(wrapper, "_semantic_member_wrappers", {})
    keys = list(engine_call(handle.memberKeys, operation="SampleSpace.parts"))
    members = {}
    for index, key in enumerate(keys):
        key = str(key)
        if bool(engine_call(handle.memberIsFamily, index, operation="SampleSpace.parts")):
            member_handle = engine_call(handle.memberFamily, index, operation="SampleSpace.parts")
            member = old_members.get(key)
            if not isinstance(member, _compat.Group):
                member = object.__new__(_compat.Group)
            _family_group(member_handle, member, context)
        else:
            member_handle = engine_call(handle.memberMobject, index, operation="SampleSpace.parts")
            member = old_members.get(key)
            if not isinstance(member, _base.Mobject):
                member = object.__new__(_compat.Rectangle)
            _shared._attach_shared_handle(member, member_handle)
            if context is not None:
                member._canonical_live_target_context = context
        members[key] = member
    wrapper._semantic_family_handle = handle
    wrapper._semantic_member_wrappers = members
    if context is not None:
        wrapper._canonical_live_target_context = context
    return wrapper


def _constructor_context():
    if _sample_space_options is None or _create_sample_space is None:
        raise RuntimeError("SampleSpace requires the shared Rust authoring host")
    return _shared._live_constructor_context("SampleSpace")


def _partition_context(sample_space):
    context = _shared._live_mutation_context(sample_space)
    return context if context is not None else _shared._live_constructor_context("SampleSpace partition")


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
        context = _constructor_context()
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
        handle = engine_call(
            context.liveCreateSampleSpace if context is not None else _create_sample_space,
            options,
            operation="SampleSpace",
        )

        rectangle = _leaf(
            engine_call(handle.rectangle, operation="SampleSpace.rectangle"), context
        )
        family = engine_call(handle.family, operation="SampleSpace.family")
        self._semantic_family_handle = family
        self._semantic_member_wrappers = {
            _shared._family_wrapper_key(rectangle): rectangle,
        }
        self._sample_space_handle = handle
        if context is not None:
            self._canonical_live_target_context = context
        self.default_label_scale_val = float(default_label_scale_val)

    def complete_p_list(self, p_list: float | Iterable[float]) -> list[float]:
        values = _probabilities(p_list)
        return [float(value) for value in engine_call(
            self._sample_space_handle.completePList,
            _array(values),
            operation="SampleSpace.complete_p_list",
        )]

    def _parts(self, kind: str, p_list, colors):
        context = _partition_context(self)
        probabilities = _probabilities(p_list)
        color_values = _colors(colors)
        if context is not None:
            live_kind = "liveGetVerticalDivision" if kind == "getVerticalDivision" else "liveGetHorizontalDivision"
            family = engine_call(
                getattr(context, live_kind),
                self._sample_space_handle,
                _array(probabilities),
                _array(color_values),
                operation=f"SampleSpace.{kind}",
            )
            return _family_group(family, context=context)
        family = engine_call(
            getattr(self._sample_space_handle, kind),
            _array(probabilities),
            _array(color_values),
            operation=f"SampleSpace.{kind}",
        )
        return _family_group(family)

    def _divide(self, kind: str, p_list, colors):
        context = _partition_context(self)
        probabilities = _probabilities(p_list)
        color_values = _colors(colors)
        if context is not None:
            live_kind = "liveDivideVertically" if kind == "divideVertically" else "liveDivideHorizontally"
            family = engine_call(
                getattr(context, live_kind),
                self._sample_space_handle,
                _array(probabilities),
                _array(color_values),
                operation=f"SampleSpace.{kind}",
            )
        else:
            family = engine_call(
                getattr(self._sample_space_handle, kind),
                _array(probabilities),
                _array(color_values),
                operation=f"SampleSpace.{kind}",
            )
        key = _family_key(family)
        wrapper = self._semantic_member_wrappers.get(key)
        wrapper = _family_group(family, wrapper, context)
        self._semantic_member_wrappers[key] = wrapper
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
        wrapper = self._semantic_member_wrappers.get(key)
        wrapper = _family_group(family, wrapper, _partition_context(self))
        self._semantic_member_wrappers[key] = wrapper
        return wrapper

    def _rehydrate_semantic_family_handle(self):
        """Rebind a copied SampleSpace to its copied Rust family graph."""
        self._sample_space_handle = engine_call(
            self._semantic_family_handle.asSampleSpace,
            operation="SampleSpace.copy",
        )
        _family_group(self._semantic_family_handle, self)

    def _rebind_copied_semantic_handle(self):
        # Group copies already map the rectangle member to the corresponding
        # copied Rust object. Its wrapper identity must remain stable.
        return None

    def get_subdivision_braces_and_labels(
        self,
        parts: _compat.Group,
        labels: list[str | _base.Mobject],
        direction: object,
        buff: float = _base.SMALL_BUFF,
        min_num_quads: int = 1,
    ) -> _compat.Group:
        """Build Manim's brace and MathTex annotations for a partition family.

        Pinned Manim v0.21 forwards ``min_num_quads`` into ``Brace`` even though
        that constructor does not accept it, so the upstream default currently
        raises ``TypeError: Mobject.__init__() got an unexpected keyword
        argument 'min_num_quads'``. Noon repairs that broken default behavior
        for ``1``; other values remain unsupported.
        """
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
