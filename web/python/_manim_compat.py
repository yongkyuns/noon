"""ManimCE-compatible public authoring foundation for the browser Python frontend.

This module deliberately changes only Python authoring semantics. Objects still lower to
Noon's existing semantic snapshots/tracks and analytic/path renderer representations.
"""

from __future__ import annotations

import copy
import math
from typing import Any, Callable, Iterator

import noon as _base

_BaseMobject = _base.Mobject
_BaseScene = _base.Scene
_NATIVE_MOBJECT_ROTATE = _BaseMobject.rotate
_ir = _base._ir

OUT = (0.0, 0.0, 1.0)
IN = (0.0, 0.0, -1.0)

_INSTALLED = False


def _manim_vmobject_kwargs(
    kwargs: dict[str, Any], *, default_color: _base.Color = _base.WHITE
) -> dict[str, Any]:
    """Apply ManimCE VMobject defaults without changing native Noon IR defaults."""
    result = dict(kwargs)
    result.setdefault(
        "fill",
        _base.Color(default_color.red, default_color.green, default_color.blue, 0.0),
    )
    result.setdefault("stroke", default_color)
    result.setdefault("stroke_width", 4.0)
    result.setdefault("stroke_join", "miter")
    result.setdefault("stroke_cap", "butt")
    return result


def _as_vec2(value: object) -> _base.Vec2:
    """Accept Noon's Vec2 plus common Manim 2D/3D vector inputs.

    Manim commonly represents 2D directions as three-component NumPy vectors. Noon
    remains 2D internally, so z=0 is accepted and non-zero z is rejected explicitly.
    """

    if isinstance(value, _base.Vec2):
        return value

    try:
        length = len(value)  # type: ignore[arg-type]
    except (TypeError, AttributeError):
        length = None

    if length in (2, 3):
        try:
            x = float(value[0])  # type: ignore[index]
            y = float(value[1])  # type: ignore[index]
            if length == 3:
                z = float(value[2])  # type: ignore[index]
                if not math.isclose(z, 0.0, abs_tol=1e-12):
                    raise NotImplementedError(
                        "Noon currently supports 2D Manim vectors only; z must be 0"
                    )
            return _base.Vec2(x, y)
        except (TypeError, ValueError, IndexError) as error:
            raise TypeError("expected a two- or three-component numeric vector") from error

    raise TypeError("expected a two- or three-component vector")


class _CompatAnimationBuilder:
    """Generic Manim-style ``mobject.animate`` target-state proxy.

    The proxy runs authoring-time mutator methods on a detached copy, then Noon lowers
    the final source/target pair to one deterministic Transform track.
    """

    def __init__(self, source: _BaseMobject) -> None:
        if source._scene is None or source._object is None:
            raise ValueError("animate requires a Mobject that belongs to a Scene")
        self.source = source
        self.target = source.copy()

    def __getattr__(self, name: str) -> Callable[..., _CompatAnimationBuilder]:
        if name.startswith("_"):
            raise AttributeError(name)
        target_attribute = getattr(self.target, name)
        if not callable(target_attribute):
            raise AttributeError(f"{name} is not an animatable method")

        def invoke(*args: Any, **kwargs: Any) -> _CompatAnimationBuilder:
            result = target_attribute(*args, **kwargs)
            if result is not None and result is not self.target:
                raise TypeError(
                    f"animate.{name} must be a mutating Mobject method returning self or None"
                )
            return self

        return invoke


class VMobject(_BaseMobject):
    """Manim-compatible vector-mobject authoring type over Noon semantic geometry."""

    def copy(self) -> VMobject:
        clone = object.__new__(type(self))
        _BaseMobject.__init__(clone, self._current_raw())
        for name, value in self.__dict__.items():
            if name not in {"_raw", "_scene", "_object"}:
                setattr(clone, name, copy.deepcopy(value))
        return clone


class Circle(VMobject):
    def __init__(
        self,
        radius: float = 1.0,
        *,
        color: _base.Color | None = None,
        **kwargs: Any,
    ) -> None:
        super().__init__(
            _ir.Circle(
                radius,
                **_manim_vmobject_kwargs(kwargs, default_color=_base.RED),
            )
        )
        self.radius = float(radius)
        if color is not None:
            self.set_color(color)


class Rectangle(VMobject):
    def __init__(
        self,
        width: float = 2.0,
        height: float = 1.0,
        *,
        color: _base.Color | None = None,
        **kwargs: Any,
    ) -> None:
        super().__init__(_ir.Rectangle(width, height, **_manim_vmobject_kwargs(kwargs)))
        self.width_value = float(width)
        self.height_value = float(height)
        if color is not None:
            self.set_color(color)


class Square(Rectangle):
    def __init__(
        self,
        side_length: float = 2.0,
        *,
        color: _base.Color | None = None,
        **kwargs: Any,
    ) -> None:
        self.side_length = float(side_length)
        super().__init__(side_length, side_length, color=color, **kwargs)


class Line(VMobject):
    def __init__(
        self,
        start: object = None,
        end: object = None,
        *,
        color: _base.Color | None = None,
        **kwargs: Any,
    ) -> None:
        start_value = _base.LEFT if start is None else _as_vec2(start)
        end_value = _base.RIGHT if end is None else _as_vec2(end)
        super().__init__(_ir.Line(start_value, end_value, **_manim_vmobject_kwargs(kwargs)))
        self.start = start_value
        self.end = end_value
        if color is not None:
            self.set_color(color)


class Path(VMobject):
    def __init__(
        self,
        path: _base.VectorPath,
        *,
        color: _base.Color | None = None,
        **kwargs: Any,
    ) -> None:
        super().__init__(_ir.Path(path, **_manim_vmobject_kwargs(kwargs)))
        self.path = path
        if color is not None:
            self.set_color(color)


def _leaf_mobjects(value: object) -> list[_BaseMobject]:
    if isinstance(value, Group):
        leaves: list[_BaseMobject] = []
        for member in value.submobjects:
            leaves.extend(_leaf_mobjects(member))
        return leaves
    if isinstance(value, _BaseMobject):
        return [value]
    raise TypeError("expected a Mobject or Group")


def _bounds_for(value: object) -> tuple[_base.Vec2, _base.Vec2] | None:
    leaves = _leaf_mobjects(value)
    bounds = [_base._bounds(member._current_raw()) for member in leaves]
    present = [bound for bound in bounds if bound is not None]
    if not present:
        return None
    return (
        _base.Vec2(
            min(bound[0].x for bound in present),
            min(bound[0].y for bound in present),
        ),
        _base.Vec2(
            max(bound[1].x for bound in present),
            max(bound[1].y for bound in present),
        ),
    )


def _critical_for(value: object, direction: _base.Vec2) -> _base.Vec2:
    bounds = _bounds_for(value)
    if bounds is None:
        return _base.ORIGIN
    minimum, maximum = bounds
    center = (minimum + maximum) * 0.5
    return _base.Vec2(
        minimum.x if direction.x < 0 else maximum.x if direction.x > 0 else center.x,
        minimum.y if direction.y < 0 else maximum.y if direction.y > 0 else center.y,
    )


def _rotation_angle_2d(angle: float, axis: object = OUT) -> float:
    try:
        if len(axis) != 3:  # type: ignore[arg-type]
            raise TypeError
        x = float(axis[0])  # type: ignore[index]
        y = float(axis[1])  # type: ignore[index]
        z = float(axis[2])  # type: ignore[index]
    except (TypeError, ValueError, IndexError) as error:
        raise TypeError("rotation axis must be a three-component vector") from error
    if not all(math.isfinite(value) for value in (x, y, z)):
        raise ValueError("rotation axis must be finite")
    if not math.isclose(x, 0.0, abs_tol=1e-12) or not math.isclose(y, 0.0, abs_tol=1e-12):
        raise NotImplementedError("2D authoring supports rotation about the z axis only")
    if math.isclose(z, 0.0, abs_tol=1e-12):
        raise ValueError("rotation axis must be non-zero")
    value = float(angle)
    if not math.isfinite(value):
        raise ValueError("rotation angle must be finite")
    return -value if z < 0.0 else value


def _mobject_rotate(
    self: _BaseMobject,
    angle: float,
    axis: object = OUT,
    *,
    about_point: object | None = None,
    about_edge: object | None = None,
    **kwargs: Any,
) -> _BaseMobject:
    if kwargs:
        unsupported = ", ".join(sorted(kwargs))
        raise NotImplementedError(f"unsupported Manim rotate option(s): {unsupported}")
    signed_angle = _rotation_angle_2d(angle, axis)
    if about_point is not None:
        pivot = _as_vec2(about_point)
    else:
        edge = _base.ORIGIN if about_edge is None else _as_vec2(about_edge)
        pivot = self.get_critical_point(edge)
    center = self.get_center()
    relative = center - pivot
    cosine = math.cos(signed_angle)
    sine = math.sin(signed_angle)
    target_center = pivot + _base.Vec2(
        relative.x * cosine - relative.y * sine,
        relative.x * sine + relative.y * cosine,
    )
    _NATIVE_MOBJECT_ROTATE(self, signed_angle)
    return self.move_to(target_center)


class _GroupAnimationBuilder:
    def __init__(self, source: Group) -> None:
        leaves = _leaf_mobjects(source)
        if any(member._scene is None or member._object is None for member in leaves):
            raise ValueError("animate requires a Group that belongs to a Scene")
        self.source = source
        self.target = source.copy()

    def __getattr__(self, name: str) -> Callable[..., _GroupAnimationBuilder]:
        if name.startswith("_"):
            raise AttributeError(name)
        target_attribute = getattr(self.target, name)
        if not callable(target_attribute):
            raise AttributeError(f"{name} is not an animatable method")

        def invoke(*args: Any, **kwargs: Any) -> _GroupAnimationBuilder:
            result = target_attribute(*args, **kwargs)
            if result is not None and result is not self.target:
                raise TypeError(
                    f"animate.{name} must be a mutating Group method returning self or None"
                )
            return self

        return invoke


class Group(_base.Group, _BaseMobject):
    """Authoring-time Mobject-family group lowered to operations on member objects.

    Noon intentionally keeps runtime hierarchy flat. The group therefore has no single
    serialized object ID; its transforms and animations lower to its leaf members.
    """

    def __init__(self, *mobjects: object) -> None:
        self.submobjects: list[object] = []
        self.add(*mobjects)

    @property
    def id(self) -> int:
        raise AttributeError("Group has no single runtime object id in Noon")

    @property
    def geometry(self) -> dict[str, Any]:
        raise AttributeError("Group has no single runtime geometry in Noon")

    @property
    def transform(self) -> dict[str, Any]:
        raise AttributeError("Group has no single runtime transform in Noon")

    @property
    def style(self) -> dict[str, Any]:
        raise AttributeError("Group has no single runtime style in Noon")

    def __iter__(self) -> Iterator[object]:
        return iter(self.submobjects)

    def __len__(self) -> int:
        return len(self.submobjects)

    def __getitem__(self, index: int) -> object:
        return self.submobjects[index]

    def add(self, *mobjects: object) -> Group:
        for mobject in mobjects:
            if not isinstance(mobject, (_BaseMobject, Group)):
                raise TypeError("Group members must be Mobjects or Groups")
            if mobject is self:
                raise ValueError("Group cannot contain itself")
            self.submobjects.append(mobject)
        return self

    def remove(self, *mobjects: object) -> Group:
        identities = {id(mobject) for mobject in mobjects}
        self.submobjects = [
            mobject for mobject in self.submobjects if id(mobject) not in identities
        ]
        return self

    def copy(self) -> Group:
        return type(self)(*(mobject.copy() for mobject in self.submobjects))

    def get_center(self) -> _base.Vec2:
        bounds = _bounds_for(self)
        if bounds is None:
            return _base.ORIGIN
        return (bounds[0] + bounds[1]) * 0.5

    @property
    def width(self) -> float:
        bounds = _bounds_for(self)
        return 0.0 if bounds is None else bounds[1].x - bounds[0].x

    @property
    def height(self) -> float:
        bounds = _bounds_for(self)
        return 0.0 if bounds is None else bounds[1].y - bounds[0].y

    def shift(self, direction: object) -> Group:
        offset = _as_vec2(direction)
        for member in self.submobjects:
            member.shift(offset)
        return self

    def move_to(self, point: object) -> Group:
        return self.shift(_as_vec2(point) - self.get_center())

    def center(self) -> Group:
        return self.move_to(_base.ORIGIN)

    def set_x(self, x: float) -> Group:
        center = self.get_center()
        return self.shift(_base.Vec2(float(x) - center.x, 0.0))

    def set_y(self, y: float) -> Group:
        center = self.get_center()
        return self.shift(_base.Vec2(0.0, float(y) - center.y))

    def scale(self, factor: float | tuple[float, float]) -> Group:
        if isinstance(factor, (tuple, list, _base.Vec2)):
            scale = _as_vec2(factor)
        else:
            scale = _base.Vec2(float(factor), float(factor))
        center = self.get_center()
        for member in self.submobjects:
            member_center = member.get_center()
            relative = member_center - center
            member.scale(scale)
            member.move_to(
                center + _base.Vec2(relative.x * scale.x, relative.y * scale.y)
            )
        return self

    def rotate(
        self,
        angle: float,
        axis: object = OUT,
        *,
        about_point: object | None = None,
        about_edge: object | None = None,
        **kwargs: Any,
    ) -> Group:
        signed_angle = _rotation_angle_2d(angle, axis)
        if about_point is not None:
            pivot = _as_vec2(about_point)
        else:
            edge = _base.ORIGIN if about_edge is None else _as_vec2(about_edge)
            pivot = _critical_for(self, edge)
        for member in self.submobjects:
            member.rotate(signed_angle, OUT, about_point=pivot, **kwargs)
        return self

    def set_color(self, color: _base.Color) -> Group:
        for member in self.submobjects:
            member.set_color(color)
        return self

    def set_fill(
        self, color: _base.Color | None = None, opacity: float | None = None
    ) -> Group:
        for member in self.submobjects:
            member.set_fill(color, opacity)
        return self

    def set_stroke(
        self, color: _base.Color | None = None, width: float | None = None
    ) -> Group:
        for member in self.submobjects:
            member.set_stroke(color, width)
        return self

    def set_opacity(self, opacity: float) -> Group:
        for member in self.submobjects:
            member.set_opacity(opacity)
        return self

    def next_to(
        self,
        other: object,
        direction: object = None,
        buff: float = _base.DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
    ) -> Group:
        axis = _as_vec2(_base.RIGHT if direction is None else direction).normalized()
        self_point = _critical_for(self, -axis)
        target_point = _critical_for(other, axis) if isinstance(other, (_BaseMobject, Group)) else _as_vec2(other)
        return self.shift(target_point - self_point + axis * float(buff))

    def align_to(self, other: object, direction: object = None) -> Group:
        axis = _as_vec2(_base.ORIGIN if direction is None else direction)
        delta = _critical_for(other, axis) - _critical_for(self, axis)
        return self.shift(
            _base.Vec2(delta.x if axis.x else 0.0, delta.y if axis.y else 0.0)
        )

    def to_edge(
        self,
        edge: object = None,
        buff: float = _base.DEFAULT_MOBJECT_TO_EDGE_BUFFER,
    ) -> Group:
        return self._align_on_frame(_as_vec2(_base.LEFT if edge is None else edge), float(buff))

    def to_corner(
        self,
        corner: object = None,
        buff: float = _base.DEFAULT_MOBJECT_TO_EDGE_BUFFER,
    ) -> Group:
        return self._align_on_frame(_as_vec2(_base.DL if corner is None else corner), float(buff))

    def _align_on_frame(self, direction: _base.Vec2, buff: float) -> Group:
        point = _critical_for(self, direction)
        target = _base.Vec2(
            math.copysign(_base.DEFAULT_FRAME_WIDTH / 2.0, direction.x)
            if direction.x
            else point.x,
            math.copysign(_base.DEFAULT_FRAME_HEIGHT / 2.0, direction.y)
            if direction.y
            else point.y,
        )
        return self.shift(
            _base.Vec2(
                target.x - point.x - (direction.x * buff if direction.x else 0.0),
                target.y - point.y - (direction.y * buff if direction.y else 0.0),
            )
        )

    def arrange(
        self,
        direction: object = None,
        buff: float = _base.DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
        center: bool = True,
    ) -> Group:
        if not self.submobjects:
            return self
        axis = _as_vec2(_base.RIGHT if direction is None else direction)
        for previous, current in zip(self.submobjects, self.submobjects[1:]):
            current.next_to(previous, axis, buff)
        if center:
            self.shift(-self.get_center())
        return self

    def arrange_in_grid(
        self,
        rows: int | None = None,
        cols: int | None = None,
        buff: float | tuple[float, float] = _base.MED_SMALL_BUFF,
    ) -> Group:
        count = len(self.submobjects)
        if count == 0:
            return self
        if rows is None and cols is None:
            cols = math.ceil(math.sqrt(count))
            rows = math.ceil(count / cols)
        elif rows is None:
            assert cols is not None
            rows = math.ceil(count / cols)
        elif cols is None:
            cols = math.ceil(count / rows)
        if rows <= 0 or cols <= 0:
            raise ValueError("rows and cols must be positive")
        gap = _as_vec2(buff) if isinstance(buff, (tuple, list, _base.Vec2)) else _base.Vec2(float(buff), float(buff))
        cell_width = max((member.width for member in self.submobjects), default=0.0) + gap.x
        cell_height = max((member.height for member in self.submobjects), default=0.0) + gap.y
        for index, member in enumerate(self.submobjects):
            row = index // cols
            col = index % cols
            member.move_to(
                _base.Vec2(
                    (col - (cols - 1) / 2.0) * cell_width,
                    ((rows - 1) / 2.0 - row) * cell_height,
                )
            )
        return self

    @property
    def animate(self) -> _GroupAnimationBuilder:
        return _GroupAnimationBuilder(self)


class VGroup(Group):
    pass


class Scene(_BaseScene):
    """Manim-style Scene facade while retaining Noon's compiled scene document."""

    def __init__(self) -> None:
        super().__init__()
        self._compat_top_level: list[object] = []

    def setup(self) -> None:
        pass

    def construct(self) -> None:
        pass

    def tear_down(self) -> None:
        pass

    def _register_top_level(self, value: object) -> None:
        if not any(existing is value for existing in self._compat_top_level):
            self._compat_top_level.append(value)

    def _is_present(self, value: object) -> bool:
        leaves = _leaf_mobjects(value)
        if not leaves:
            return False
        return any(member._is_present_in_scene(self, self._cursor) for member in leaves)

    @property
    def mobjects(self) -> list[object]:
        return [value for value in self._compat_top_level if self._is_present(value)]

    def add(self, *mobjects: object, key: str | None = None) -> _BaseMobject | Scene:
        if not mobjects:
            return self
        leaves = [member for value in mobjects for member in _leaf_mobjects(value)]
        if key is not None and len(leaves) != 1:
            raise ValueError("an explicit key can only be used when adding one Mobject")

        for index, member in enumerate(leaves):
            newly_bound = member._scene is None
            if newly_bound:
                member._bind_to_scene(self, key=key if index == 0 else None)
            elif member._scene is not self:
                raise ValueError("Mobject already belongs to another Scene")

            assert member._object is not None
            tracks = self._ensure_lifecycle_timeline_available(
                member._object, self._cursor, "Scene.add target"
            )
            if newly_bound and self._cursor > 0.0:
                self._add_presence_track(
                    member._object,
                    False,
                    True,
                    self._cursor,
                    key=f"@scene-add:{member._object.id}:{self._cursor:g}",
                )
            elif tracks and not self._presence_at(member._object, self._cursor):
                self._add_presence_track(
                    member._object,
                    False,
                    True,
                    self._cursor,
                    key=f"@scene-add:{member._object.id}:{self._cursor:g}",
                )

        for value in mobjects:
            self._register_top_level(value)

        # Preserve Noon's established one-object return as a backwards-compatible
        # extension. Typical Manim source ignores Scene.add's return value.
        return leaves[0] if len(leaves) == 1 else self

    def remove(self, *mobjects: object) -> Scene:
        leaves = [member for value in mobjects for member in _leaf_mobjects(value)]
        for member in leaves:
            if member._scene is not self or member._object is None:
                continue
            self._ensure_lifecycle_timeline_available(
                member._object, self._cursor, "Scene.remove target"
            )
            if self._presence_at(member._object, self._cursor):
                self._add_presence_track(
                    member._object,
                    True,
                    False,
                    self._cursor,
                    key=f"@scene-remove:{member._object.id}:{self._cursor:g}",
                )
        identities = {id(value) for value in mobjects}
        self._compat_top_level = [
            value for value in self._compat_top_level if id(value) not in identities
        ]
        return self

    def clear(self) -> Scene:
        return self.remove(*list(self._compat_top_level))

    def replace(self, old_mobject: object, new_mobject: object) -> Scene:
        old_index = next(
            (
                index
                for index, value in enumerate(self._compat_top_level)
                if value is old_mobject
            ),
            None,
        )
        self.remove(old_mobject)
        self.add(new_mobject)
        if old_index is not None:
            self._compat_top_level = [
                value for value in self._compat_top_level if value is not new_mobject
            ]
            self._compat_top_level.insert(old_index, new_mobject)
        return self

    def _bind_introducer_target(self, target: object) -> None:
        if isinstance(target, Group):
            for member in _leaf_mobjects(target):
                if member._scene is None:
                    member._bind_to_scene(self)
                elif member._scene is not self:
                    raise ValueError("Mobject already belongs to another Scene")
            self._register_top_level(target)
            return
        if isinstance(target, _BaseMobject):
            if target._scene is None:
                target._bind_to_scene(self)
            elif target._scene is not self:
                raise ValueError("Mobject already belongs to another Scene")
            self._register_top_level(target)

    def _expand_animation(self, animation: object) -> list[object]:
        if isinstance(animation, _GroupAnimationBuilder):
            sources = _leaf_mobjects(animation.source)
            targets = _leaf_mobjects(animation.target)
            if len(sources) != len(targets):
                raise ValueError("group animation must preserve leaf membership")
            return [
                _base.Transform(source, target)
                for source, target in zip(sources, targets)
            ]

        if isinstance(animation, _base.Uncreate) and isinstance(animation.target, Group):
            leaves = _leaf_mobjects(animation.target)
            return [
                type(animation)(
                    member,
                    None if animation.key is None else f"{animation.key}.{index}",
                    reverse_rate_function=animation.reverse_rate_function,
                    remover=animation.remover,
                )
                for index, member in enumerate(leaves)
            ]

        if isinstance(animation, (_base.Create, _base.FadeIn, _base.FadeOut)) and isinstance(
            animation.target, Group
        ):
            leaves = _leaf_mobjects(animation.target)
            return [
                type(animation)(
                    member,
                    None if animation.key is None else f"{animation.key}.{index}",
                )
                for index, member in enumerate(leaves)
            ]
        return [animation]

    def play(
        self,
        *animations: Any,
        duration: float | None = None,
        run_time: float | None = None,
        start_time: float | None = None,
        easing: str | None = None,
        rate_func: object | None = None,
        **kwargs: Any,
    ) -> Scene:
        if kwargs:
            unsupported = ", ".join(sorted(kwargs))
            raise NotImplementedError(
                f"unsupported Manim Scene.play option(s): {unsupported}"
            )
        if rate_func is not None and easing is not None:
            raise ValueError("use either rate_func or the low-level easing alias, not both")
        actual_easing = easing or (
            _easing_from_rate_func(rate_func) if rate_func is not None else "smooth"
        )

        # Manim introducing animations own the lifecycle transition; users do not
        # need to call add() first. Preserve existing pre-bound Noon objects too.
        for animation in animations:
            if isinstance(animation, (_base.Create, _base.FadeIn)):
                self._bind_introducer_target(animation.target)

        expanded = [
            lowered
            for animation in animations
            for lowered in self._expand_animation(animation)
        ]
        return super().play(
            *expanded,
            duration=duration,
            run_time=run_time,
            start_time=start_time,
            easing=actual_easing,
        )



def _mobject_get_critical_point(
    self: _BaseMobject, direction: object
) -> _base.Vec2:
    return _critical_for(self, _as_vec2(direction))


def _mobject_get_edge_center(self: _BaseMobject, direction: object) -> _base.Vec2:
    return self.get_critical_point(direction)


def _mobject_get_corner(self: _BaseMobject, direction: object) -> _base.Vec2:
    return self.get_critical_point(direction)


def _mobject_get_left(self: _BaseMobject) -> _base.Vec2:
    return self.get_critical_point(_base.LEFT)


def _mobject_get_right(self: _BaseMobject) -> _base.Vec2:
    return self.get_critical_point(_base.RIGHT)


def _mobject_get_top(self: _BaseMobject) -> _base.Vec2:
    return self.get_critical_point(_base.UP)


def _mobject_get_bottom(self: _BaseMobject) -> _base.Vec2:
    return self.get_critical_point(_base.DOWN)


def _mobject_get_coord(
    self: _BaseMobject, dim: int, direction: object = _base.ORIGIN
) -> float:
    if dim not in (0, 1):
        raise NotImplementedError("Noon currently exposes x/y authoring coordinates only")
    point = self.get_critical_point(direction)
    return float(point[dim])


def _mobject_get_x(self: _BaseMobject, direction: object = _base.ORIGIN) -> float:
    return self.get_coord(0, direction)


def _mobject_get_y(self: _BaseMobject, direction: object = _base.ORIGIN) -> float:
    return self.get_coord(1, direction)


def _mobject_set_coord(
    self: _BaseMobject,
    value: float,
    dim: int,
    direction: object = _base.ORIGIN,
) -> _BaseMobject:
    if dim not in (0, 1):
        raise NotImplementedError("Noon currently exposes x/y authoring coordinates only")
    delta = float(value) - self.get_coord(dim, direction)
    return self.shift(_base.Vec2(delta, 0.0) if dim == 0 else _base.Vec2(0.0, delta))


def _mobject_set_x(
    self: _BaseMobject, x: float, direction: object = _base.ORIGIN
) -> _BaseMobject:
    return self.set_coord(x, 0, direction)


def _mobject_set_y(
    self: _BaseMobject, y: float, direction: object = _base.ORIGIN
) -> _BaseMobject:
    return self.set_coord(y, 1, direction)


def _mobject_rescale_to_fit(
    self: _BaseMobject,
    length: float,
    dim: int,
    stretch: bool = False,
    **kwargs: Any,
) -> _BaseMobject:
    if kwargs:
        unsupported = ", ".join(sorted(kwargs))
        raise NotImplementedError(
            f"rescale_to_fit anchor option(s) are not yet supported: {unsupported}"
        )
    if dim not in (0, 1):
        raise NotImplementedError("Noon currently exposes width/height fitting only")
    old_length = self.width if dim == 0 else self.height
    if old_length == 0.0:
        return self
    factor = float(length) / old_length
    if stretch:
        return self.scale((factor, 1.0) if dim == 0 else (1.0, factor))
    return self.scale(factor)


def _mobject_scale_to_fit_width(self: _BaseMobject, width: float, **kwargs: Any) -> _BaseMobject:
    return self.rescale_to_fit(width, 0, stretch=False, **kwargs)


def _mobject_scale_to_fit_height(self: _BaseMobject, height: float, **kwargs: Any) -> _BaseMobject:
    return self.rescale_to_fit(height, 1, stretch=False, **kwargs)


def _mobject_stretch_to_fit_width(self: _BaseMobject, width: float, **kwargs: Any) -> _BaseMobject:
    return self.rescale_to_fit(width, 0, stretch=True, **kwargs)


def _mobject_stretch_to_fit_height(self: _BaseMobject, height: float, **kwargs: Any) -> _BaseMobject:
    return self.rescale_to_fit(height, 1, stretch=True, **kwargs)


def _mobject_match_dim_size(
    self: _BaseMobject, mobject: _BaseMobject, dim: int, **kwargs: Any
) -> _BaseMobject:
    if not isinstance(mobject, _BaseMobject):
        raise TypeError("dimension match target must be a Mobject")
    if dim == 0:
        length = mobject.width
    elif dim == 1:
        length = mobject.height
    else:
        raise NotImplementedError("Noon currently exposes width/height matching only")
    return self.rescale_to_fit(length, dim, **kwargs)


def _mobject_match_width(
    self: _BaseMobject, mobject: _BaseMobject, **kwargs: Any
) -> _BaseMobject:
    return self.match_dim_size(mobject, 0, **kwargs)


def _mobject_match_height(
    self: _BaseMobject, mobject: _BaseMobject, **kwargs: Any
) -> _BaseMobject:
    return self.match_dim_size(mobject, 1, **kwargs)


def _mobject_match_coord(
    self: _BaseMobject,
    mobject: _BaseMobject,
    dim: int,
    direction: object = _base.ORIGIN,
) -> _BaseMobject:
    if not isinstance(mobject, _BaseMobject):
        raise TypeError("coordinate match target must be a Mobject")
    return self.set_coord(mobject.get_coord(dim, direction), dim, direction)


def _mobject_match_x(
    self: _BaseMobject,
    mobject: _BaseMobject,
    direction: object = _base.ORIGIN,
) -> _BaseMobject:
    return self.match_coord(mobject, 0, direction)


def _mobject_match_y(
    self: _BaseMobject,
    mobject: _BaseMobject,
    direction: object = _base.ORIGIN,
) -> _BaseMobject:
    return self.match_coord(mobject, 1, direction)


def _mobject_rotate_about_origin(
    self: _BaseMobject, angle: float, axis: object = None
) -> _BaseMobject:
    return self.rotate(
        angle, OUT if axis is None else axis, about_point=_base.ORIGIN
    )


def _set_width_property(self: _BaseMobject, width: float) -> None:
    self.scale_to_fit_width(float(width))


def _set_height_property(self: _BaseMobject, height: float) -> None:
    self.scale_to_fit_height(float(height))


def _state_target(self: _BaseMobject, mobject: _BaseMobject, *, match_height: bool, match_width: bool, match_depth: bool, match_center: bool, stretch: bool) -> _BaseMobject:
    if not isinstance(mobject, _BaseMobject):
        raise TypeError("state target must be a Mobject")
    if match_depth:
        raise NotImplementedError("depth matching requires the shared 2.5D family model")
    if not (match_height or match_width or match_center or stretch):
        return mobject
    target = mobject.copy()
    if stretch:
        if target.width == 0.0 or target.height == 0.0:
            raise ValueError("cannot stretch a zero-width or zero-height target")
        target.scale((self.width / target.width, self.height / target.height))
    else:
        if match_height:
            if target.height == 0.0:
                raise ValueError("cannot match height from a zero-height target")
            target.scale(self.height / target.height)
        if match_width:
            if target.width == 0.0:
                raise ValueError("cannot match width from a zero-width target")
            target.scale(self.width / target.width)
    if match_center:
        target.move_to(self.get_center())
    return target


def _mobject_generate_target(self: _BaseMobject, use_deepcopy: bool = False) -> _BaseMobject:
    """Create the detached target through the installed shared target editor."""
    # Canonical Mobjects install `_copy_for_animate_target`, which delegates target
    # capture to Rust.  This preserves effective-state capture for a live source and
    # avoids using `copy()` as a second target-state model.  Plain compatibility
    # objects retain their existing copy behavior until they have a typed handle.
    del use_deepcopy
    factory = getattr(self, "_copy_for_animate_target", None)
    target = factory() if callable(factory) else self.copy()
    self.target = target
    return target


def _mobject_save_state(self: _BaseMobject) -> _BaseMobject:
    if hasattr(self, "saved_state"):
        self.saved_state = None
    self.saved_state = self.copy()
    return self


def _mobject_restore(self: _BaseMobject) -> _BaseMobject:
    if not hasattr(self, "saved_state") or self.saved_state is None:
        raise Exception("Trying to restore without having saved")
    return self.become(self.saved_state)


def _mobject_become(
    self: _BaseMobject,
    mobject: _BaseMobject,
    match_height: bool = False,
    match_width: bool = False,
    match_depth: bool = False,
    match_center: bool = False,
    stretch: bool = False,
) -> _BaseMobject:
    target = _state_target(
        self,
        mobject,
        match_height=match_height,
        match_width=match_width,
        match_depth=match_depth,
        match_center=match_center,
        stretch=stretch,
    )
    return self._apply(_base._raw_mobject(target._current_raw()))


def _mobject_replace(
    self: _BaseMobject, mobject: _BaseMobject, dim_to_match: int = 0, stretch: bool = False
) -> _BaseMobject:
    if not isinstance(mobject, _BaseMobject):
        raise TypeError("replacement target must be a Mobject")
    if dim_to_match not in (0, 1):
        raise NotImplementedError("replace currently supports width (0) or height (1)")
    if stretch:
        if self.width == 0.0 or self.height == 0.0:
            raise ValueError("cannot stretch-replace an object with zero width or height")
        self.scale((mobject.width / self.width, mobject.height / self.height))
    else:
        source_length = self.width if dim_to_match == 0 else self.height
        target_length = mobject.width if dim_to_match == 0 else mobject.height
        if source_length == 0.0:
            raise ValueError("cannot replace along a zero-length dimension")
        self.scale(target_length / source_length)
    self.move_to(mobject.get_center())
    return self


class MoveToTarget:
    """ManimCE ``MoveToTarget`` over the shared leaf ``TransformTo`` path."""

    def __new__(cls, mobject: object, **kwargs: Any):
        if isinstance(mobject, Group):
            raise NotImplementedError(
                "MoveToTarget(Group/VGroup) requires retained family Transform semantics"
            )
        if not isinstance(mobject, _BaseMobject):
            raise TypeError("MoveToTarget target must be a Mobject")
        if not hasattr(mobject, "target"):
            raise ValueError("MoveToTarget called on mobject without attribute 'target'")
        target = mobject.target
        if not isinstance(target, _BaseMobject) or isinstance(target, Group):
            raise NotImplementedError(
                "MoveToTarget currently requires a leaf Mobject target produced by generate_target()"
            )
        unsupported = sorted(set(kwargs) - {"key"})
        if unsupported:
            raise NotImplementedError(
                "unsupported MoveToTarget option(s): " + ", ".join(unsupported)
            )
        return _base.Transform(mobject, target, key=kwargs.get("key"))


def install() -> None:
    """Install the compatibility surface into the public ``noon`` module."""

    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True

    # Existing Mobject methods resolve _as_vec2 dynamically from noon.py globals,
    # so replacing that helper makes inherited transforms/layout accept z=0 vectors.
    _base._as_vec2 = _as_vec2
    _BaseMobject.animate = property(lambda self: _CompatAnimationBuilder(self))
    _BaseMobject.get_critical_point = _mobject_get_critical_point
    _BaseMobject.get_edge_center = _mobject_get_edge_center
    _BaseMobject.get_corner = _mobject_get_corner
    _BaseMobject.get_left = _mobject_get_left
    _BaseMobject.get_right = _mobject_get_right
    _BaseMobject.get_top = _mobject_get_top
    _BaseMobject.get_bottom = _mobject_get_bottom
    _BaseMobject.get_coord = _mobject_get_coord
    _BaseMobject.get_x = _mobject_get_x
    _BaseMobject.get_y = _mobject_get_y
    _BaseMobject.set_coord = _mobject_set_coord
    _BaseMobject.set_x = _mobject_set_x
    _BaseMobject.set_y = _mobject_set_y
    _BaseMobject.rescale_to_fit = _mobject_rescale_to_fit
    _BaseMobject.scale_to_fit_width = _mobject_scale_to_fit_width
    _BaseMobject.scale_to_fit_height = _mobject_scale_to_fit_height
    _BaseMobject.stretch_to_fit_width = _mobject_stretch_to_fit_width
    _BaseMobject.stretch_to_fit_height = _mobject_stretch_to_fit_height
    _BaseMobject.match_dim_size = _mobject_match_dim_size
    _BaseMobject.match_width = _mobject_match_width
    _BaseMobject.match_height = _mobject_match_height
    _BaseMobject.match_coord = _mobject_match_coord
    _BaseMobject.match_x = _mobject_match_x
    _BaseMobject.match_y = _mobject_match_y
    _BaseMobject.rotate = _mobject_rotate
    _BaseMobject.rotate_about_origin = _mobject_rotate_about_origin
    _BaseMobject.width = property(_BaseMobject.width.fget, _set_width_property)
    _BaseMobject.height = property(_BaseMobject.height.fget, _set_height_property)
    _BaseMobject.generate_target = _mobject_generate_target
    _BaseMobject.save_state = _mobject_save_state
    _BaseMobject.restore = _mobject_restore
    _BaseMobject.become = _mobject_become
    _BaseMobject.replace = _mobject_replace

    public = {
        "VMobject": VMobject,
        "Circle": Circle,
        "Rectangle": Rectangle,
        "Square": Square,
        "Line": Line,
        "Path": Path,
        "Group": Group,
        "VGroup": VGroup,
        "Scene": Scene,
        "MoveToTarget": MoveToTarget,
        "OUT": OUT,
        "IN": IN,
    }
    for name, value in public.items():
        setattr(_base, name, value)

    exports = list(_base.__all__)
    for name in public:
        if name not in exports:
            exports.append(name)
    _base.__all__ = exports
