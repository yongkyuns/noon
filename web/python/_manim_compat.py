"""ManimCE-compatible public authoring foundation for the browser Python frontend.

Python owns public class shape and argument coercion. Shared Rust operations own
semantic geometry, mutation, layout and execution.
"""

from __future__ import annotations

import copy
import math
from typing import Any, Callable, Iterator

import noon as _base

from noon import Mobject, Scene
_ir = _base._ir

OUT = (0.0, 0.0, 1.0)
IN = (0.0, 0.0, -1.0)


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


class VMobject(Mobject):
    """Manim-compatible vector-mobject authoring type over Noon semantic geometry."""

    def set_style(self, fill_color=None, fill_opacity=None, stroke_color=None,
                  stroke_width=None, stroke_opacity=None, family=True, **kwargs):
        from _manim_semantic_handles import _set_style
        return _set_style(self, fill_color, fill_opacity, stroke_color,
                          stroke_width, stroke_opacity, family, **kwargs)

    def match_style(self, vmobject, family=True):
        from _manim_semantic_handles import _match_style
        return _match_style(self, vmobject, family)

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


def _leaf_mobjects(value: object) -> list[Mobject]:
    if isinstance(value, Group):
        leaves: list[Mobject] = []
        for member in value.submobjects:
            leaves.extend(_leaf_mobjects(member))
        return leaves
    if isinstance(value, Mobject):
        return [value]
    raise TypeError("expected a Mobject or Group")


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


class Group(Mobject):
    """Python identities and ergonomics over a shared Rust semantic family."""

    def set_style(self, fill_color=None, fill_opacity=None, stroke_color=None,
                  stroke_width=None, stroke_opacity=None, family=True, **kwargs):
        from _manim_semantic_handles import _set_style
        return _set_style(self, fill_color, fill_opacity, stroke_color,
                          stroke_width, stroke_opacity, family, **kwargs)

    def match_style(self, vmobject, family=True):
        from _manim_semantic_handles import _match_style
        return _match_style(self, vmobject, family)

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

    def __getitem__(self, index: int | slice) -> object:
        if isinstance(index, slice):
            group_class = VGroup if isinstance(self, VGroup) else Group
            return group_class(*self.submobjects[index])
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
        return _base._semantic_operations()._get_center(self)

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

    def set_x(self, x: float, direction: object = _base.ORIGIN) -> Group:
        return self.set_coord(x, 0, direction)

    def set_y(self, y: float, direction: object = _base.ORIGIN) -> Group:
        return self.set_coord(y, 1, direction)

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
        return self._align_on_frame(_base._as_vec2(_base.LEFT if edge is None else edge), float(buff))

    def to_corner(
        self,
        corner: object = None,
        buff: float = _base.DEFAULT_MOBJECT_TO_EDGE_BUFFER,
    ) -> Group:
        return self._align_on_frame(_base._as_vec2(_base.DL if corner is None else corner), float(buff))

    def _align_on_frame(self, direction: _base.Vec2, buff: float) -> Group:
        return _base._semantic_operations()._group_align_on_frame(self, direction, buff)


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
    def animate(self):
        from _manim_animate import _AlignedGroupAnimationBuilder
        return _AlignedGroupAnimationBuilder(self)

    def _copy_for_animate_target(self) -> Group:
        return _base._semantic_operations()._group_copy(self)

    def __deepcopy__(self, memo):
        return deepcopy_semantic_wrapper(self, memo)

    def get_color(self) -> _base.Color:
        from _manim_geometry import _group_get_color
        return _group_get_color(self)


class VGroup(Group):
    pass


def _mobject_generate_target(self: Mobject, use_deepcopy: bool = False) -> Mobject:
    """Capture a detached target through the shared Rust target editor."""
    del use_deepcopy
    previous = getattr(self, "target", None)
    # A target must not recursively clone the previously generated target chain.
    # Preserve the wrapper's previous target if shared capture is rejected.
    self.target = None
    try:
        target = self._copy_for_animate_target()
    except BaseException:
        self.target = previous
        raise
    self.target = target
    return target


def _mobject_save_state(self: Mobject) -> Mobject:
    if hasattr(self, "saved_state"):
        self.saved_state = None
    self.saved_state = self.copy()
    return self


def _mobject_restore(self: Mobject) -> Mobject:
    if not hasattr(self, "saved_state") or self.saved_state is None:
        raise Exception("Trying to restore without having saved")
    return self.become(self.saved_state)


class MoveToTarget:
    """ManimCE ``MoveToTarget`` over the shared leaf ``TransformTo`` path."""

    def __new__(cls, mobject: object, **kwargs: Any):
        if isinstance(mobject, Group):
            raise NotImplementedError(
                "MoveToTarget(Group/VGroup) requires retained family Transform semantics"
            )
        if not isinstance(mobject, Mobject):
            raise TypeError("MoveToTarget target must be a Mobject")
        if not hasattr(mobject, "target"):
            raise ValueError("MoveToTarget called on mobject without attribute 'target'")
        target = mobject.target
        if not isinstance(target, Mobject) or isinstance(target, Group):
            raise NotImplementedError(
                "MoveToTarget currently requires a leaf Mobject target produced by generate_target()"
            )
        unsupported = sorted(set(kwargs) - {"key"})
        if unsupported:
            raise NotImplementedError(
                "unsupported MoveToTarget option(s): " + ", ".join(unsupported)
            )
        return _base.Transform(mobject, target, key=kwargs.get("key"))


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
