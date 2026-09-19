"""Thin Manim Brace adapters backed by shared Rust retained geometry."""

from __future__ import annotations

from typing import Any

from _noon_errors import engine_call

import noon as _base
import _manim_compat as _compat
import _manim_semantic_handles as _shared


_DEFAULT_FONT_SIZE = 48.0
_BRACE_TIP_ANCHOR_INDEX = 7


def _require_geometry_host(name: str) -> None:
    if _shared._create_geometry_handle is None:
        raise RuntimeError(f"{name} requires the shared Rust authoring host")


def _brace_target_handle(mobject: object):
    if not isinstance(mobject, _base.Mobject):
        raise TypeError("Brace target must be a Mobject")
    if isinstance(mobject, _compat.Group):
        handle = getattr(mobject, "_semantic_family_handle", None)
    else:
        handle = _shared._handle_for(mobject)
    if handle is None or not hasattr(handle, "beginBrace"):
        raise NotImplementedError(
            "Brace target requires a current shared semantic object/family handle"
        )
    return handle


def _brace_constructor_options(
    options: dict[str, Any],
    *,
    stroke_width: float,
    fill_opacity: float,
    background_stroke_width: float,
) -> dict[str, Any]:
    background_width = _shared._ir._finite_number(
        "background_stroke_width", background_stroke_width
    )
    if background_width != 0.0:
        raise NotImplementedError(
            "Brace background stroke is not represented by Noon's ordinary retained path style"
        )
    # background_stroke_color is visually inert while background_stroke_width == 0.
    options.pop("background_stroke_color", None)
    options["stroke_width"] = stroke_width
    options["fill_opacity"] = fill_opacity
    return options


def _finish_candidate(
    owner: _base.Mobject,
    candidate: object,
    name: str,
    options: dict[str, Any],
) -> None:
    color = options.pop("color", None)
    _shared._apply_shared_constructor_options(candidate, options)
    if color is not None:
        parsed = _shared._compat._as_color("color", color)
        _shared._apply_constructor_color(candidate, parsed)
    _shared._attach_geometry_options(owner, candidate, name)


def _required_text_constructor(name: str, feature: str):
    try:
        constructor = getattr(_base, name)
    except AttributeError as error:
        raise NotImplementedError(
            f"{feature} requires Manim-compatible {name} support from the retained text layer"
        ) from error
    if not callable(constructor):
        raise TypeError(f"{name} must be callable")
    return constructor


def _explicit_label_constructor(label_constructor: object | None):
    if label_constructor is None:
        return _required_text_constructor("MathTex", "BraceLabel default label construction")
    if not callable(label_constructor):
        raise TypeError("label_constructor must be callable")
    return label_constructor


def _new_label(
    label_constructor: object,
    text: object,
    font_size: float,
    extra_kwargs: dict[str, Any] | None = None,
):
    constructor = label_constructor
    kwargs = {} if extra_kwargs is None else dict(extra_kwargs)
    if isinstance(text, (tuple, list)):
        label = constructor(*text, font_size=font_size, **kwargs)
    else:
        label = constructor(str(text), font_size=font_size, **kwargs)
    if not isinstance(label, _base.Mobject):
        raise TypeError("label_constructor must return a Mobject")
    return label


class Brace(_compat.VMobject):
    """ManimCE v0.21 Brace whose path/layout policy is owned by shared Rust."""

    def __init__(
        self,
        mobject: _base.Mobject,
        direction: object = _base.DOWN,
        buff: float = 0.2,
        sharpness: float = 2.0,
        stroke_width: float = 0.0,
        fill_opacity: float = 1.0,
        background_stroke_width: float = 0.0,
        background_stroke_color: object = _base.BLACK,
        **kwargs: Any,
    ) -> None:
        _require_geometry_host("Brace")
        target = _brace_target_handle(mobject)
        direction_value = _base._as_vec2(direction)
        buff_value = _shared._ir._finite_number("buff", buff)
        sharpness_value = _shared._ir._finite_number("sharpness", sharpness)
        stroke_value = _shared._ir._finite_number("stroke_width", stroke_width)
        fill_value = _shared._compat._opacity("fill_opacity", fill_opacity)
        options = dict(kwargs)
        options["background_stroke_color"] = background_stroke_color
        options = _brace_constructor_options(
            options,
            stroke_width=stroke_value,
            fill_opacity=fill_value,
            background_stroke_width=background_stroke_width,
        )
        candidate = engine_call(
            target.beginBrace,
            direction_value.x,
            direction_value.y,
            buff_value,
            sharpness_value,
            operation="Brace",
        )
        _finish_candidate(self, candidate, "Brace", options)
        self.buff = buff_value
        self.sharpness = sharpness_value
        self.direction = direction_value

    def get_tip(self) -> _base.Vec2:
        """Return the pinned ManimCE v0.21 brace-tip anchor."""
        # Cairo's Brace.get_tip() returns points[28] == the eighth anchor in
        # the canonical cubic path. Noon's anchors come from the Rust-owned
        # retained VectorPath query, so Python owns only the class-specific index.
        anchors = self.get_anchors()
        if len(anchors) <= _BRACE_TIP_ANCHOR_INDEX:
            raise RuntimeError("Brace path does not contain the canonical tip anchor")
        return anchors[_BRACE_TIP_ANCHOR_INDEX]

    def get_direction(self) -> _base.Vec2:
        """Return the normalized direction from brace center to brace tip."""
        vector = self.get_tip() - self.get_center()
        return vector.normalized()

    def put_at_tip(
        self,
        mob: _base.Mobject,
        use_next_to: bool = True,
        **kwargs: Any,
    ) -> Brace:
        """Place ``mob`` at the brace tip using ordinary retained layout operations."""
        if not isinstance(mob, _base.Mobject):
            raise TypeError("Brace.put_at_tip expects a Mobject")
        if use_next_to:
            direction = self.get_direction()
            rounded = _base.Vec2(round(direction.x), round(direction.y))
            mob.next_to(self.get_tip(), rounded, **kwargs)
        else:
            mob.move_to(self.get_tip())
            buff = _shared._ir._finite_number(
                "buff", kwargs.get("buff", _base.DEFAULT_MOBJECT_TO_MOBJECT_BUFFER)
            )
            shift_distance = mob.width / 2.0 + buff
            mob.shift(self.get_direction() * shift_distance)
        return self

    def get_text(self, *text: str, **kwargs: Any):
        """Construct upstream ``Tex`` at the tip once retained Tex is available."""
        constructor = _required_text_constructor("Tex", "Brace.get_text")
        label = constructor(*text)
        self.put_at_tip(label, **kwargs)
        return label

    def get_tex(self, *tex: str, **kwargs: Any):
        """Construct upstream ``MathTex`` at the tip once retained MathTex is available."""
        constructor = _required_text_constructor("MathTex", "Brace.get_tex")
        label = constructor(*tex)
        self.put_at_tip(label, **kwargs)
        return label


class BraceBetweenPoints(Brace):
    """Brace between two points without allocating a temporary semantic Line."""

    def __init__(
        self,
        point_1: object,
        point_2: object,
        direction: object = _base.ORIGIN,
        **kwargs: Any,
    ) -> None:
        _require_geometry_host("BraceBetweenPoints")
        first = _base._as_vec2(point_1)
        second = _base._as_vec2(point_2)
        direction_value = _base._as_vec2(direction)
        options = dict(kwargs)
        buff_value = _shared._ir._finite_number("buff", options.pop("buff", 0.2))
        sharpness_value = _shared._ir._finite_number(
            "sharpness", options.pop("sharpness", 2.0)
        )
        stroke_value = _shared._ir._finite_number(
            "stroke_width", options.pop("stroke_width", 0.0)
        )
        fill_value = _shared._compat._opacity(
            "fill_opacity", options.pop("fill_opacity", 1.0)
        )
        background_width = options.pop("background_stroke_width", 0.0)
        options = _brace_constructor_options(
            options,
            stroke_width=stroke_value,
            fill_opacity=fill_value,
            background_stroke_width=background_width,
        )
        candidate = engine_call(
            _shared._geometry_options.braceBetweenPoints,
            first.x,
            first.y,
            second.x,
            second.y,
            direction_value.x,
            direction_value.y,
            buff_value,
            sharpness_value,
            operation="BraceBetweenPoints",
        )
        _finish_candidate(self, candidate, "BraceBetweenPoints", options)
        self.buff = buff_value
        self.sharpness = sharpness_value
        self.direction = direction_value


class BraceLabel(_compat.VGroup):
    """Brace plus label as an ordinary semantic family.

    Noon's retained B4 layer does not yet expose ManimCE ``MathTex``. Therefore
    the upstream default constructor is fail-closed until that dependency lands;
    callers may already use an explicit retained label constructor such as ``Text``.
    """

    def __init__(
        self,
        obj: _base.Mobject,
        text: object,
        brace_direction: object = _base.DOWN,
        label_constructor: object | None = None,
        font_size: float = _DEFAULT_FONT_SIZE,
        buff: float = 0.2,
        brace_config: dict[str, Any] | None = None,
        **kwargs: Any,
    ) -> None:
        constructor = _explicit_label_constructor(label_constructor)
        font_size_value = _shared._ir._positive_number("font_size", font_size)
        group_options = dict(kwargs)
        z_index = _shared._ir._finite_number("z_index", group_options.pop("z_index", 0.0))
        if group_options:
            unsupported = ", ".join(sorted(group_options))
            raise NotImplementedError(
                f"unsupported BraceLabel composite option(s): {unsupported}"
            )
        if brace_config is None:
            brace_options: dict[str, Any] = {}
        elif isinstance(brace_config, dict):
            brace_options = dict(brace_config)
        else:
            raise TypeError("brace_config must be a dict or None")

        self.label_constructor = constructor
        self.brace_direction = _base._as_vec2(brace_direction)
        self.brace = Brace(
            obj,
            direction=self.brace_direction,
            buff=buff,
            **brace_options,
        )
        self.label = _new_label(constructor, text, font_size_value)
        self.brace.put_at_tip(self.label)
        super().__init__(self.brace, self.label, z_index=z_index)

    def creation_anim(self, label_anim=None, brace_anim=None):
        if label_anim is None:
            label_anim = getattr(_base, "FadeIn")
        if brace_anim is None:
            brace_anim = getattr(_base, "GrowFromCenter")
        animation_group = getattr(_base, "AnimationGroup")
        return animation_group(brace_anim(self.brace), label_anim(self.label))

    def shift_brace(self, obj: _base.Mobject, **kwargs: Any) -> BraceLabel:
        if isinstance(obj, list):
            obj = _compat.VGroup(*obj)
        old_brace = self.brace
        self.remove(old_brace, self.label)
        self.brace = Brace(obj, direction=self.brace_direction, **kwargs)
        self.brace.put_at_tip(self.label)
        self.add(self.brace, self.label)
        return self

    def change_label(self, *text: str, **kwargs: Any) -> BraceLabel:
        old_label = self.label
        self.remove(old_label)
        label = self.label_constructor(*text, **kwargs)
        if not isinstance(label, _base.Mobject):
            raise TypeError("label_constructor must return a Mobject")
        self.label = label
        self.brace.put_at_tip(self.label)
        self.add(self.label)
        return self

    def change_brace_label(
        self,
        obj: _base.Mobject,
        *text: str,
        **kwargs: Any,
    ) -> BraceLabel:
        self.shift_brace(obj)
        self.change_label(*text, **kwargs)
        return self


class BraceText(BraceLabel):
    """Brace plus retained native ``Text`` as an ordinary semantic family."""

    def __init__(
        self,
        obj: _base.Mobject,
        text: str,
        label_constructor: object | None = None,
        **kwargs: Any,
    ) -> None:
        constructor = (
            _required_text_constructor("Text", "BraceText")
            if label_constructor is None
            else label_constructor
        )
        super().__init__(
            obj,
            text,
            label_constructor=constructor,
            **kwargs,
        )
