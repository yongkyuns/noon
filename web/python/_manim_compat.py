"""ManimCE-compatible public authoring foundation for the browser Python frontend.

Python owns public class shape and argument coercion. Shared Rust operations own
semantic geometry, mutation, layout and execution.
"""

from __future__ import annotations

import copy
import math
from typing import Any, Callable, Iterator

import noon as _base

_BaseMobject = _base.Mobject
_BaseScene = _base.Scene
_ir = _base._ir

OUT = (0.0, 0.0, 1.0)
IN = (0.0, 0.0, -1.0)

_INSTALLED = False


# Pinned ManimCE v0.21.0 Cairo presentation contract. Cairo converts
# VMobject stroke widths to scene units with this multiplier and AUTO
# leaves its native miter-join / butt-cap defaults in effect.
MANIM_CAIRO_LINE_WIDTH_MULTIPLE = 0.01
MANIM_DEFAULT_STROKE_WIDTH = 4.0


def _manim_stroke_width(value: object) -> float:
    width = _base._ir._finite_number("stroke width", value)
    if width < 0.0:
        raise ValueError("stroke width must be non-negative")
    return width * MANIM_CAIRO_LINE_WIDTH_MULTIPLE


def _opacity(name: str, value: object) -> float:
    return _base._ir._unit_interval(name, value)


def _as_color(name: str, value: object) -> _base.Color:
    if isinstance(value, _base.Color):
        return value
    if isinstance(value, (str, int)) and not isinstance(value, bool):
        try:
            return _base.color_from_hex(value)
        except (TypeError, ValueError) as error:
            raise ValueError(f"invalid {name}") from error
    raise TypeError(f"{name} must be a Color or #RRGGBB value")



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
        from _manim_semantic_handles import _copy_mobject
        return _copy_mobject(self)

    def set_color(self, color: object, family: bool = True) -> VMobject:
        from _manim_updaters import _canonical_vmobject_set_color
        return _canonical_vmobject_set_color(self, color, family=family)

    def set_fill(self, color: object = None, opacity: float | None = None, family: bool = True) -> VMobject:
        from _manim_updaters import _canonical_vmobject_set_fill
        return _canonical_vmobject_set_fill(self, color=color, opacity=opacity, family=family)

    def set_stroke(
        self,
        color: object = None,
        width: float | None = None,
        opacity: float | None = None,
        family: bool = True,
    ) -> VMobject:
        from _manim_updaters import _canonical_vmobject_set_stroke
        return _canonical_vmobject_set_stroke(self, color=color, width=width, opacity=opacity, family=family)

    def set_opacity(self, opacity: float, family: bool = True) -> VMobject:
        from _manim_updaters import _canonical_vmobject_set_opacity
        return _canonical_vmobject_set_opacity(self, opacity, family=family)

    def get_fill_opacity(self) -> float:
        from _manim_semantic_handles import _get_fill_opacity
        return _get_fill_opacity(self)

    def get_stroke_opacity(self) -> float:
        from _manim_semantic_handles import _get_stroke_opacity
        return _get_stroke_opacity(self)


class Circle(VMobject):
    def __init__(
        self,
        radius: float = 1.0,
        *,
        color: _base.Color | None = None,
        **kwargs: Any,
    ) -> None:
        from _manim_semantic_handles import _circle_init
        _circle_init(self, radius, color=color, **kwargs)


class Rectangle(VMobject):
    def __init__(
        self,
        width: float = 4.0,
        height: float = 2.0,
        *,
        color: _base.Color | None = None,
        **kwargs: Any,
    ) -> None:
        from _manim_semantic_handles import _rectangle_init
        _rectangle_init(self, width, height, color=color, **kwargs)


class Square(Rectangle):
    def __init__(
        self,
        side_length: float = 2.0,
        *,
        color: _base.Color | None = None,
        **kwargs: Any,
    ) -> None:
        from _manim_semantic_handles import _square_init
        _square_init(self, side_length, color=color, **kwargs)


class Line(VMobject):
    def __init__(
        self,
        start: object = None,
        end: object = None,
        *,
        color: _base.Color | None = None,
        **kwargs: Any,
    ) -> None:
        from _manim_semantic_handles import _line_init
        _line_init(self, start, end, color=color, **kwargs)

    def get_start(self) -> _base.Vec2:
        from _manim_geometry import _line_get_start
        return _line_get_start(self)

    def get_end(self) -> _base.Vec2:
        from _manim_geometry import _line_get_end
        return _line_get_end(self)


class Path(VMobject):
    def __init__(
        self,
        path: _base.VectorPath,
        *,
        color: _base.Color | None = None,
        **kwargs: Any,
    ) -> None:
        from _manim_semantic_handles import _path_init
        _path_init(self, path, color=color, **kwargs)


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
    return _base._semantic_operations()._compat_bounds_for(value)


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
    """Python identities and ergonomics over a shared Rust semantic family."""

    def __init__(self, *mobjects: object) -> None:
        from _manim_semantic_handles import _group_init
        _group_init(self, *mobjects)

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
        from _manim_semantic_handles import _group_add
        return _group_add(self, *mobjects)

    def remove(self, *mobjects: object) -> Group:
        from _manim_semantic_handles import _group_remove
        return _group_remove(self, *mobjects)

    def copy(self) -> Group:
        from _manim_semantic_handles import _group_copy
        return _group_copy(self)

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
        return _base._semantic_operations()._group_shift(self, direction)

    def move_to(
        self,
        point_or_mobject: object,
        aligned_edge: object = _base.ORIGIN,
        coor_mask: object = (1.0, 1.0, 1.0),
    ) -> Group:
        return _base._semantic_operations()._group_move_to(self, point_or_mobject, aligned_edge, coor_mask)

    def center(self) -> Group:
        return self.move_to(_base.ORIGIN)

    def set_x(self, x: float) -> Group:
        center = self.get_center()
        return self.shift(_base.Vec2(float(x) - center.x, 0.0))

    def set_y(self, y: float) -> Group:
        center = self.get_center()
        return self.shift(_base.Vec2(0.0, float(y) - center.y))

    def scale(self, factor: float | tuple[float, float]) -> Group:
        from _manim_semantic_handles import _group_scale
        return _group_scale(self, factor)

    def rotate(
        self,
        angle: float,
        axis: object = OUT,
        *,
        about_point: object | None = None,
        about_edge: object | None = None,
        **kwargs: Any,
    ) -> Group:
        from _manim_semantic_handles import _group_rotate
        return _group_rotate(self, angle, axis, about_point=about_point, about_edge=about_edge, **kwargs)

    def set_color(self, color: object) -> Group:
        from _manim_semantic_handles import _group_set_color
        return _group_set_color(self, color)

    def set_fill(self, color: object = None, opacity: float | None = None) -> Group:
        from _manim_semantic_handles import _group_set_fill
        return _group_set_fill(self, color, opacity)

    def set_stroke(self, color: object = None, width: float | None = None,
                   opacity: float | None = None) -> Group:
        from _manim_semantic_handles import _group_set_stroke
        return _group_set_stroke(self, color, width, opacity)

    def set_opacity(self, opacity: float) -> Group:
        from _manim_semantic_handles import _group_set_opacity
        return _group_set_opacity(self, opacity)

    def next_to(
        self,
        mobject_or_point: object,
        direction: object = _base.RIGHT,
        buff: float = _base.DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
        aligned_edge: object = _base.ORIGIN,
        submobject_to_align: object | None = None,
        index_of_submobject_to_align: int | None = None,
        coor_mask: object = (1.0, 1.0, 1.0),
    ) -> _base.Mobject | Group:
        return _base._semantic_operations()._next_to(self, mobject_or_point, direction, buff, aligned_edge, submobject_to_align, index_of_submobject_to_align, coor_mask)

    def align_to(self, mobject_or_point: object, direction: object = _base.ORIGIN) -> Group:
        return _base._semantic_operations()._group_align_to(self, mobject_or_point, direction)

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
        direction: object = _base.RIGHT,
        buff: float = _base.DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
        center: bool = True,
        **kwargs: Any,
    ) -> Group:
        return _base._semantic_operations()._group_arrange(self, direction, buff, center, **kwargs)

    def arrange_in_grid(
        self,
        rows: int | None = None,
        cols: int | None = None,
        buff: float | tuple[float, float] = _base.MED_SMALL_BUFF,
    ) -> Group:
        from _manim_semantic_handles import _group_arrange_in_grid
        return _group_arrange_in_grid(self, rows, cols, buff)

    @property
    def animate(self) -> _GroupAnimationBuilder:
        return _GroupAnimationBuilder(self)

    def _copy_for_animate_target(self) -> Group:
        return _base._semantic_operations()._group_copy(self)

    def __deepcopy__(self, memo):
        return deepcopy_semantic_wrapper(self, memo)

    def get_color(self) -> _base.Color:
        from _manim_geometry import _group_get_color
        return _group_get_color(self)


class VGroup(Group):
    pass


class Scene(_BaseScene):
    """Manim-style lifecycle and membership ergonomics over the shared Rust host."""

    def __init__(self) -> None:
        super().__init__()
        # Explicit retained/export-only wrappers stay deletion-owned by #959.
        # Ordinary membership and painter order are derived from shared Rust.
        self._compat_top_level: list[object] = []

    def setup(self) -> None:
        pass

    def construct(self) -> None:
        pass

    def tear_down(self) -> None:
        pass

    def _register_top_level(self, value: object) -> None:
        if (
            getattr(value, "_semantic_handle", None) is not None
            or getattr(value, "_semantic_family_handle", None) is not None
        ):
            _base._scene_operations()._register_membership_wrappers(self, value)
            return
        if not any(existing is value for existing in self._compat_top_level):
            self._compat_top_level.append(value)

    @property
    def mobjects(self) -> list[object]:
        return _base._scene_operations()._canonical_scene_mobjects(self)

    def _edit_membership(self, kind: str, values: tuple[object, ...] = (), *, key=None) -> None:
        _base._scene_operations()._canonical_edit_membership(self, kind, values, key=key)

    def add(self, *mobjects: object, key: str | None = None) -> _BaseMobject | Scene:
        if not mobjects:
            return self
        self._edit_membership("add", mobjects, key=key)

        # Preserve Noon's established one-object return as a backwards-compatible
        # extension. Typical Manim source ignores Scene.add's return value.
        leaves = [member for value in mobjects for member in _leaf_mobjects(value)]
        return leaves[0] if len(leaves) == 1 else self

    def remove(self, *mobjects: object) -> Scene:
        self._edit_membership("remove", mobjects)
        return self

    def clear(self) -> Scene:
        self._edit_membership("clear")
        return self

    def replace(self, old_mobject: object, new_mobject: object) -> Scene:
        self._edit_membership("replace", (old_mobject, new_mobject))
        return self





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






def _state_target(
    self: _BaseMobject,
    mobject: _BaseMobject,
    *,
    match_height: bool,
    match_width: bool,
    match_depth: bool,
    match_center: bool,
    stretch: bool,
) -> _BaseMobject:
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
    previous = getattr(self, "target", None)
    # A target must not recursively clone the previously generated target chain.
    # Preserve the wrapper's previous target if shared capture is rejected.
    self.target = None
    try:
        target = factory() if callable(factory) else self.copy()
    except BaseException:
        self.target = previous
        raise
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
    raise RuntimeError("Mobject become requires the shared Rust authoring host")


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
    _BaseMobject.rotate_about_origin = _mobject_rotate_about_origin
    _BaseMobject.generate_target = _mobject_generate_target
    _BaseMobject.save_state = _mobject_save_state
    _BaseMobject.restore = _mobject_restore

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


_FAMILY_COPY_METADATA = object()


def deepcopy_semantic_wrapper(self, memo):
    """Participate in a metadata preparation pass without copying engine handles."""
    allocate = memo.get(_FAMILY_COPY_METADATA)
    if allocate is not None:
        return allocate(self)
    clone = self.copy()
    memo[id(self)] = clone
    return clone


def prepare_family_wrapper_copy(source: Group, excluded_fields):
    """Prepare wrapper metadata, including saved-state references, before commit.

    No constructors or semantic operations run here. Rust copies all referenced
    nodes in one transaction after the fallible host metadata pass has completed.
    """
    pairs = []
    memo = {}

    def allocate(value):
        existing = memo.get(id(value))
        if existing is not None:
            return existing
        clone = object.__new__(type(value))
        memo[id(value)] = clone
        pairs.append((value, clone))
        if isinstance(value, Group):
            clone.submobjects = [allocate(member) for member in value.submobjects]
        return clone

    memo[_FAMILY_COPY_METADATA] = allocate
    root = allocate(source)
    # Deepcopy can discover saved states or wrappers nested in arbitrary metadata
    # containers. Their hooks enqueue metadata work instead of touching Rust.
    index = 0
    while index < len(pairs):
        original, clone = pairs[index]
        index += 1
        excluded = excluded_fields(original)
        copy_wrapper_attributes(original, clone, memo, excluded | {"submobjects"})
    return root, pairs


def copy_wrapper_attributes(source, target, memo=None, excluded=()):
    """Copy host-language attributes using an optional family identity memo."""
    for name, value in source.__dict__.items():
        if name not in excluded:
            setattr(target, name, copy.deepcopy(value, memo))


def _shift_group_members(self: Group, direction: object) -> Group:
    # Existing per-member callback fallback; shared callback family operations
    # in #955 own its retirement. Ordinary typed family shifts stay in Rust.
    offset = _as_vec2(direction)
    for member in self.submobjects:
        member.shift(offset)
    return self
