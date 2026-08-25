#!/usr/bin/env python3
"""Differential semantic probes against a pinned ManimCE reference.

This suite intentionally compares small, renderer-independent observables.  It is
not a screenshot test and it does not require constructing a Manim Scene.  Each
fixture runs the equivalent operation through Noon and ManimCE, normalizes the
result to JSON-like data, and reports a structural diff on mismatch.

Add new probes only for behavior Noon claims to support.  Unsupported behavior
belongs in ``UNSUPPORTED`` below until an implementation PR promotes it to a
gated fixture.  That distinction prevents the compatibility suite from turning
missing API surface into an accidental semantic specification.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable

REPO_ROOT = Path(__file__).resolve().parents[1]
WEB_PYTHON = REPO_ROOT / "web" / "python"
if str(WEB_PYTHON) not in sys.path:
    sys.path.insert(0, str(WEB_PYTHON))

# Import Noon before Manim so the local module is unambiguous.
import noon as noon  # noqa: E402
import _manim_compat as _manim_compat  # noqa: E402

# The browser worker installs this facade before user code runs. Differential
# fixtures must exercise that same public Manim-compatible surface rather than
# the lower-level authoring primitives that happen to back it.
_manim_compat.install()

try:
    import manim as manim  # noqa: E402
except ImportError as exc:  # pragma: no cover - exercised by the CI environment
    raise SystemExit(
        "ManimCE is required for the differential suite. "
        "Install the version pinned by .github/workflows/manim-differential.yml."
    ) from exc

PINNED_MANIM_VERSION = "0.21.0"


@dataclass(frozen=True)
class Fixture:
    name: str
    noon_probe: Callable[[], Any]
    manim_probe: Callable[[], Any]
    tolerance: float = 1e-6


def _round_float(value: float) -> float:
    value = float(value)
    if abs(value) < 1e-12:
        return 0.0
    return value


def _object_observation(obj: Any) -> dict[str, Any]:
    center = obj.get_center()
    return {
        "center": [_round_float(center[0]), _round_float(center[1])],
        "width": _round_float(obj.width),
        "height": _round_float(obj.height),
    }


def _point_observation(point: Any) -> list[float]:
    return [_round_float(point[0]), _round_float(point[1])]


def _members_observation(group: Any) -> dict[str, Any]:
    members = list(group.submobjects)
    center = group.get_center()
    return {
        "center": [_round_float(center[0]), _round_float(center[1])],
        "members": [_object_observation(member) for member in members],
    }


def _noon_circle_dimensions() -> Any:
    return _object_observation(noon.Circle(radius=0.75))


def _manim_circle_dimensions() -> Any:
    return _object_observation(manim.Circle(radius=0.75))


def _noon_rectangle_dimensions() -> Any:
    return _object_observation(noon.Rectangle(width=3.0, height=1.25))


def _manim_rectangle_dimensions() -> Any:
    return _object_observation(manim.Rectangle(width=3.0, height=1.25))


def _noon_shifted_circle() -> Any:
    obj = noon.Circle(radius=0.5).shift(noon.RIGHT * 2.25 + noon.UP * 1.5)
    return _object_observation(obj)


def _manim_shifted_circle() -> Any:
    obj = manim.Circle(radius=0.5).shift(manim.RIGHT * 2.25 + manim.UP * 1.5)
    return _object_observation(obj)


def _noon_moved_rectangle() -> Any:
    obj = noon.Rectangle(width=2.0, height=0.75).move_to(noon.LEFT * 1.75 + noon.DOWN * 0.6)
    return _object_observation(obj)


def _manim_moved_rectangle() -> Any:
    obj = manim.Rectangle(width=2.0, height=0.75).move_to(manim.LEFT * 1.75 + manim.DOWN * 0.6)
    return _object_observation(obj)


def _noon_scaled_square() -> Any:
    obj = noon.Square(side_length=1.2).scale(1.75)
    return _object_observation(obj)


def _manim_scaled_square() -> Any:
    obj = manim.Square(side_length=1.2).scale(1.75)
    return _object_observation(obj)


def _noon_rotated_rectangle() -> Any:
    obj = noon.Rectangle(width=3.0, height=1.0).rotate(math.pi / 2.0)
    return _object_observation(obj)


def _manim_rotated_rectangle() -> Any:
    obj = manim.Rectangle(width=3.0, height=1.0).rotate(math.pi / 2.0)
    return _object_observation(obj)


def _noon_next_to() -> Any:
    left = noon.Circle(radius=0.6).shift(noon.LEFT * 1.0)
    right = noon.Square(side_length=0.8).next_to(left, noon.RIGHT, buff=0.3)
    return {"left": _object_observation(left), "right": _object_observation(right)}


def _manim_next_to() -> Any:
    left = manim.Circle(radius=0.6).shift(manim.LEFT * 1.0)
    right = manim.Square(side_length=0.8).next_to(left, manim.RIGHT, buff=0.3)
    return {"left": _object_observation(left), "right": _object_observation(right)}


def _noon_align_to_top() -> Any:
    target = noon.Rectangle(width=2.0, height=1.5).shift(noon.RIGHT * 0.8 + noon.UP * 0.5)
    obj = noon.Circle(radius=0.4).shift(noon.LEFT * 2.0).align_to(target, noon.UP)
    return {"target": _object_observation(target), "object": _object_observation(obj)}


def _manim_align_to_top() -> Any:
    target = manim.Rectangle(width=2.0, height=1.5).shift(manim.RIGHT * 0.8 + manim.UP * 0.5)
    obj = manim.Circle(radius=0.4).shift(manim.LEFT * 2.0).align_to(target, manim.UP)
    return {"target": _object_observation(target), "object": _object_observation(obj)}


def _noon_to_edge() -> Any:
    obj = noon.Square(side_length=1.0).to_edge(noon.LEFT, buff=0.4)
    return _object_observation(obj)


def _manim_to_edge() -> Any:
    obj = manim.Square(side_length=1.0).to_edge(manim.LEFT, buff=0.4)
    return _object_observation(obj)


def _noon_to_corner() -> Any:
    obj = noon.Circle(radius=0.5).to_corner(noon.UR, buff=0.25)
    return _object_observation(obj)


def _manim_to_corner() -> Any:
    obj = manim.Circle(radius=0.5).to_corner(manim.UR, buff=0.25)
    return _object_observation(obj)


def _noon_arrange() -> Any:
    group = noon.VGroup(
        noon.Circle(radius=0.25),
        noon.Square(side_length=0.6),
        noon.Rectangle(width=0.8, height=0.4),
    ).arrange(noon.RIGHT, buff=0.2)
    return _members_observation(group)


def _manim_arrange() -> Any:
    group = manim.VGroup(
        manim.Circle(radius=0.25),
        manim.Square(side_length=0.6),
        manim.Rectangle(width=0.8, height=0.4),
    ).arrange(manim.RIGHT, buff=0.2)
    return _members_observation(group)


def _noon_set_xy() -> Any:
    obj = noon.Circle(radius=0.4).set_x(1.25).set_y(-0.75)
    return _object_observation(obj)


def _manim_set_xy() -> Any:
    obj = manim.Circle(radius=0.4).set_x(1.25).set_y(-0.75)
    return _object_observation(obj)


def _noon_next_to_point() -> Any:
    point = noon.RIGHT * 1.5 + noon.UP * 0.2
    obj = noon.Square(side_length=0.6).next_to(point, noon.UP, buff=0.15)
    return _object_observation(obj)


def _manim_next_to_point() -> Any:
    point = manim.RIGHT * 1.5 + manim.UP * 0.2
    obj = manim.Square(side_length=0.6).next_to(point, manim.UP, buff=0.15)
    return _object_observation(obj)


def _noon_vgroup_add_remove() -> Any:
    first = noon.Circle(radius=0.25).shift(noon.LEFT)
    removed = noon.Square(side_length=0.5)
    last = noon.Rectangle(width=0.8, height=0.4).shift(noon.RIGHT)
    group = noon.VGroup(first, removed).add(last).remove(removed)
    return _members_observation(group)


def _manim_vgroup_add_remove() -> Any:
    first = manim.Circle(radius=0.25).shift(manim.LEFT)
    removed = manim.Square(side_length=0.5)
    last = manim.Rectangle(width=0.8, height=0.4).shift(manim.RIGHT)
    group = manim.VGroup(first, removed).add(last).remove(removed)
    return _members_observation(group)


def _noon_vgroup_shift() -> Any:
    group = noon.VGroup(
        noon.Circle(radius=0.25).shift(noon.LEFT),
        noon.Square(side_length=0.5).shift(noon.RIGHT),
    ).shift(noon.UP * 0.75 + noon.LEFT * 0.2)
    return _members_observation(group)


def _manim_vgroup_shift() -> Any:
    group = manim.VGroup(
        manim.Circle(radius=0.25).shift(manim.LEFT),
        manim.Square(side_length=0.5).shift(manim.RIGHT),
    ).shift(manim.UP * 0.75 + manim.LEFT * 0.2)
    return _members_observation(group)


def _noon_mobject_copy_independence() -> Any:
    source = noon.Rectangle(width=1.2, height=0.6).shift(noon.LEFT * 0.8)
    clone = source.copy().shift(noon.RIGHT * 2.0)
    return {"source": _object_observation(source), "clone": _object_observation(clone)}


def _manim_mobject_copy_independence() -> Any:
    source = manim.Rectangle(width=1.2, height=0.6).shift(manim.LEFT * 0.8)
    clone = source.copy().shift(manim.RIGHT * 2.0)
    return {"source": _object_observation(source), "clone": _object_observation(clone)}


def _noon_vgroup_copy_independence() -> Any:
    source = noon.VGroup(
        noon.Circle(radius=0.25),
        noon.Square(side_length=0.5),
    ).arrange(noon.RIGHT, buff=0.3)
    clone = source.copy().shift(noon.UP * 1.1)
    return {"source": _members_observation(source), "clone": _members_observation(clone)}


def _manim_vgroup_copy_independence() -> Any:
    source = manim.VGroup(
        manim.Circle(radius=0.25),
        manim.Square(side_length=0.5),
    ).arrange(manim.RIGHT, buff=0.3)
    clone = source.copy().shift(manim.UP * 1.1)
    return {"source": _members_observation(source), "clone": _members_observation(clone)}


def _noon_vgroup_arrange_grid() -> Any:
    group = noon.VGroup(*(noon.Square(side_length=0.5) for _ in range(4))).arrange_in_grid(
        rows=2, cols=2, buff=0.3
    )
    return _members_observation(group)


def _manim_vgroup_arrange_grid() -> Any:
    group = manim.VGroup(*(manim.Square(side_length=0.5) for _ in range(4))).arrange_in_grid(
        rows=2, cols=2, buff=0.3
    )
    return _members_observation(group)


def _noon_vgroup_scale() -> Any:
    group = noon.VGroup(
        noon.Circle(radius=0.25),
        noon.Circle(radius=0.25),
    ).arrange(noon.RIGHT, buff=0.4).scale(1.5)
    return _members_observation(group)


def _manim_vgroup_scale() -> Any:
    group = manim.VGroup(
        manim.Circle(radius=0.25),
        manim.Circle(radius=0.25),
    ).arrange(manim.RIGHT, buff=0.4).scale(1.5)
    return _members_observation(group)


def _noon_vgroup_rotate() -> Any:
    group = noon.VGroup(
        noon.Rectangle(width=0.8, height=0.4),
        noon.Rectangle(width=0.8, height=0.4),
    ).arrange(noon.RIGHT, buff=0.25).rotate(math.pi / 2.0)
    return _members_observation(group)


def _manim_vgroup_rotate() -> Any:
    group = manim.VGroup(
        manim.Rectangle(width=0.8, height=0.4),
        manim.Rectangle(width=0.8, height=0.4),
    ).arrange(manim.RIGHT, buff=0.25).rotate(math.pi / 2.0)
    return _members_observation(group)


def _noon_generate_target() -> Any:
    source = noon.Circle(radius=0.4).shift(noon.LEFT * 0.6)
    target = source.generate_target().shift(noon.RIGHT * 1.5).scale(1.5)
    return {"source": _object_observation(source), "target": _object_observation(target)}


def _manim_generate_target() -> Any:
    source = manim.Circle(radius=0.4).shift(manim.LEFT * 0.6)
    target = source.generate_target().shift(manim.RIGHT * 1.5).scale(1.5)
    return {"source": _object_observation(source), "target": _object_observation(target)}


def _noon_save_restore() -> Any:
    obj = noon.Rectangle(width=1.2, height=0.6).shift(noon.LEFT * 0.5)
    obj.save_state().shift(noon.RIGHT * 2.0).scale(1.75).restore()
    return _object_observation(obj)


def _manim_save_restore() -> Any:
    obj = manim.Rectangle(width=1.2, height=0.6).shift(manim.LEFT * 0.5)
    obj.save_state().shift(manim.RIGHT * 2.0).scale(1.75).restore()
    return _object_observation(obj)


def _noon_become() -> Any:
    source = noon.Circle(radius=0.4).shift(noon.LEFT)
    target = noon.Rectangle(width=1.6, height=0.8).shift(noon.RIGHT * 1.25 + noon.UP * 0.4)
    source.become(target)
    return _object_observation(source)


def _manim_become() -> Any:
    source = manim.Circle(radius=0.4).shift(manim.LEFT)
    target = manim.Rectangle(width=1.6, height=0.8).shift(manim.RIGHT * 1.25 + manim.UP * 0.4)
    source.become(target)
    return _object_observation(source)


def _noon_replace_width() -> Any:
    source = noon.Circle(radius=0.25)
    target = noon.Rectangle(width=2.0, height=1.0).shift(noon.RIGHT * 0.8 + noon.DOWN * 0.3)
    source.replace(target)
    return _object_observation(source)


def _manim_replace_width() -> Any:
    source = manim.Circle(radius=0.25)
    target = manim.Rectangle(width=2.0, height=1.0).shift(manim.RIGHT * 0.8 + manim.DOWN * 0.3)
    source.replace(target)
    return _object_observation(source)


def _noon_replace_stretch() -> Any:
    source = noon.Circle(radius=0.25)
    target = noon.Rectangle(width=2.0, height=1.0).shift(noon.LEFT * 0.7 + noon.UP * 0.2)
    source.replace(target, stretch=True)
    return _object_observation(source)


def _manim_replace_stretch() -> Any:
    source = manim.Circle(radius=0.25)
    target = manim.Rectangle(width=2.0, height=1.0).shift(manim.LEFT * 0.7 + manim.UP * 0.2)
    source.replace(target, stretch=True)
    return _object_observation(source)



def _noon_critical_points() -> Any:
    obj = noon.Rectangle(width=2.0, height=1.0).shift(noon.RIGHT * 0.7 + noon.UP * 0.3)
    return {
        "left": _point_observation(obj.get_left()),
        "right": _point_observation(obj.get_right()),
        "top": _point_observation(obj.get_top()),
        "bottom": _point_observation(obj.get_bottom()),
        "corner": _point_observation(obj.get_corner(noon.UR)),
        "x_left": _round_float(obj.get_x(noon.LEFT)),
        "y_top": _round_float(obj.get_y(noon.UP)),
    }


def _manim_critical_points() -> Any:
    obj = manim.Rectangle(width=2.0, height=1.0).shift(manim.RIGHT * 0.7 + manim.UP * 0.3)
    return {
        "left": _point_observation(obj.get_left()),
        "right": _point_observation(obj.get_right()),
        "top": _point_observation(obj.get_top()),
        "bottom": _point_observation(obj.get_bottom()),
        "corner": _point_observation(obj.get_corner(manim.UR)),
        "x_left": _round_float(obj.get_x(manim.LEFT)),
        "y_top": _round_float(obj.get_y(manim.UP)),
    }


def _noon_set_coord_direction() -> Any:
    obj = noon.Square(side_length=1.0).shift(noon.RIGHT * 0.25)
    obj.set_coord(-1.5, 0, noon.LEFT).set_coord(1.25, 1, noon.UP)
    return {"object": _object_observation(obj), "left": _point_observation(obj.get_left()), "top": _point_observation(obj.get_top())}


def _manim_set_coord_direction() -> Any:
    obj = manim.Square(side_length=1.0).shift(manim.RIGHT * 0.25)
    obj.set_coord(-1.5, 0, manim.LEFT).set_coord(1.25, 1, manim.UP)
    return {"object": _object_observation(obj), "left": _point_observation(obj.get_left()), "top": _point_observation(obj.get_top())}


def _noon_scale_to_fit_width() -> Any:
    return _object_observation(noon.Rectangle(width=2.0, height=1.0).scale_to_fit_width(3.0))


def _manim_scale_to_fit_width() -> Any:
    return _object_observation(manim.Rectangle(width=2.0, height=1.0).scale_to_fit_width(3.0))


def _noon_stretch_to_fit_height() -> Any:
    return _object_observation(noon.Rectangle(width=2.0, height=1.0).stretch_to_fit_height(2.5))


def _manim_stretch_to_fit_height() -> Any:
    return _object_observation(manim.Rectangle(width=2.0, height=1.0).stretch_to_fit_height(2.5))


def _noon_match_xy() -> Any:
    target = noon.Rectangle(width=1.5, height=0.8).shift(noon.RIGHT * 1.2 + noon.DOWN * 0.6)
    obj = noon.Circle(radius=0.3).match_x(target, noon.RIGHT).match_y(target, noon.DOWN)
    return {"target": _object_observation(target), "object": _object_observation(obj), "right": _point_observation(obj.get_right()), "bottom": _point_observation(obj.get_bottom())}


def _manim_match_xy() -> Any:
    target = manim.Rectangle(width=1.5, height=0.8).shift(manim.RIGHT * 1.2 + manim.DOWN * 0.6)
    obj = manim.Circle(radius=0.3).match_x(target, manim.RIGHT).match_y(target, manim.DOWN)
    return {"target": _object_observation(target), "object": _object_observation(obj), "right": _point_observation(obj.get_right()), "bottom": _point_observation(obj.get_bottom())}


def _noon_match_width() -> Any:
    target = noon.Rectangle(width=2.4, height=0.6)
    return _object_observation(noon.Circle(radius=0.4).match_width(target))


def _manim_match_width() -> Any:
    target = manim.Rectangle(width=2.4, height=0.6)
    return _object_observation(manim.Circle(radius=0.4).match_width(target))


def _noon_match_height_stretch() -> Any:
    target = noon.Rectangle(width=0.5, height=1.8)
    return _object_observation(noon.Rectangle(width=1.4, height=0.7).match_height(target, stretch=True))


def _manim_match_height_stretch() -> Any:
    target = manim.Rectangle(width=0.5, height=1.8)
    return _object_observation(manim.Rectangle(width=1.4, height=0.7).match_height(target, stretch=True))


def _noon_dimension_properties() -> Any:
    obj = noon.Rectangle(width=2.0, height=1.0)
    obj.width = 3.0
    obj.height = 1.5
    return _object_observation(obj)


def _manim_dimension_properties() -> Any:
    obj = manim.Rectangle(width=2.0, height=1.0)
    obj.width = 3.0
    obj.height = 1.5
    return _object_observation(obj)


def _noon_rotate_about_origin() -> Any:
    obj = noon.Rectangle(width=1.2, height=0.6).shift(noon.RIGHT * 1.5 + noon.UP * 0.5)
    obj.rotate_about_origin(math.pi / 2.0)
    return _object_observation(obj)


def _manim_rotate_about_origin() -> Any:
    obj = manim.Rectangle(width=1.2, height=0.6).shift(manim.RIGHT * 1.5 + manim.UP * 0.5)
    obj.rotate_about_origin(math.pi / 2.0)
    return _object_observation(obj)



def _member_centers(group: Any) -> list[list[float]]:
    return [_point_observation(member.get_center()) for member in group.submobjects]


def _noon_vgroup_duplicate_add() -> Any:
    first = noon.Circle(radius=0.2).shift(noon.LEFT)
    group = noon.VGroup(first).add(first, first)
    return {"length": len(group), "centers": _member_centers(group)}


def _manim_vgroup_duplicate_add() -> Any:
    first = manim.Circle(radius=0.2).shift(manim.LEFT)
    group = manim.VGroup(first).add(first, first)
    return {"length": len(group), "centers": _member_centers(group)}


def _noon_vgroup_insert() -> Any:
    first = noon.Circle(radius=0.2).shift(noon.LEFT)
    second = noon.Square(side_length=0.4).shift(noon.RIGHT)
    group = noon.VGroup(first, second)
    result = group.insert(1, first)
    return {"return_is_self": result is group, "centers": _member_centers(group)}


def _manim_vgroup_insert() -> Any:
    first = manim.Circle(radius=0.2).shift(manim.LEFT)
    second = manim.Square(side_length=0.4).shift(manim.RIGHT)
    group = manim.VGroup(first, second)
    result = group.insert(1, first)
    return {"return_is_self": result is group, "centers": _member_centers(group)}


def _noon_vgroup_add_to_back() -> Any:
    first = noon.Circle(radius=0.2).shift(noon.LEFT)
    second = noon.Square(side_length=0.4)
    third = noon.Rectangle(width=0.4, height=0.2).shift(noon.RIGHT)
    group = noon.VGroup(first, second, third).add_to_back(third, first)
    return _member_centers(group)


def _manim_vgroup_add_to_back() -> Any:
    first = manim.Circle(radius=0.2).shift(manim.LEFT)
    second = manim.Square(side_length=0.4)
    third = manim.Rectangle(width=0.4, height=0.2).shift(manim.RIGHT)
    group = manim.VGroup(first, second, third).add_to_back(third, first)
    return _member_centers(group)


def _noon_vgroup_slice() -> Any:
    group = noon.VGroup(
        noon.Circle(radius=0.2).shift(noon.LEFT),
        noon.Square(side_length=0.4),
        noon.Rectangle(width=0.4, height=0.2).shift(noon.RIGHT),
    )
    subset = group[1:]
    return {"type": type(subset).__name__, "centers": _member_centers(subset)}


def _manim_vgroup_slice() -> Any:
    group = manim.VGroup(
        manim.Circle(radius=0.2).shift(manim.LEFT),
        manim.Square(side_length=0.4),
        manim.Rectangle(width=0.4, height=0.2).shift(manim.RIGHT),
    )
    subset = group[1:]
    return {"type": type(subset).__name__, "centers": _member_centers(subset)}


def _noon_nested_family_alias() -> Any:
    shared = noon.Circle(radius=0.2).shift(noon.LEFT)
    inner = noon.VGroup(shared)
    outer = noon.Group(inner, shared)
    inner.shift(noon.RIGHT * 0.5)
    return {
        "outer_length": len(outer),
        "inner_length": len(inner),
        "shared": _point_observation(shared.get_center()),
        "outer": _members_observation(outer),
    }


def _manim_nested_family_alias() -> Any:
    shared = manim.Circle(radius=0.2).shift(manim.LEFT)
    inner = manim.VGroup(shared)
    outer = manim.Group(inner, shared)
    inner.shift(manim.RIGHT * 0.5)
    return {
        "outer_length": len(outer),
        "inner_length": len(inner),
        "shared": _point_observation(shared.get_center()),
        "outer": _members_observation(outer),
    }


def _noon_vgroup_duplicate_slice() -> Any:
    first = noon.Circle(radius=0.2).shift(noon.LEFT)
    second = noon.Square(side_length=0.4).shift(noon.RIGHT)
    group = noon.VGroup(first, second)
    group.insert(1, first)
    subset = group[:2]
    return {"length": len(subset), "centers": _member_centers(subset)}


def _manim_vgroup_duplicate_slice() -> Any:
    first = manim.Circle(radius=0.2).shift(manim.LEFT)
    second = manim.Square(side_length=0.4).shift(manim.RIGHT)
    group = manim.VGroup(first, second)
    group.insert(1, first)
    subset = group[:2]
    return {"length": len(subset), "centers": _member_centers(subset)}


FIXTURES = [
    Fixture("circle_dimensions", _noon_circle_dimensions, _manim_circle_dimensions),
    Fixture("rectangle_dimensions", _noon_rectangle_dimensions, _manim_rectangle_dimensions),
    Fixture("shifted_circle", _noon_shifted_circle, _manim_shifted_circle),
    Fixture("moved_rectangle", _noon_moved_rectangle, _manim_moved_rectangle),
    Fixture("scaled_square", _noon_scaled_square, _manim_scaled_square),
    Fixture("rotated_rectangle", _noon_rotated_rectangle, _manim_rotated_rectangle),
    Fixture("next_to", _noon_next_to, _manim_next_to),
    Fixture("align_to_top", _noon_align_to_top, _manim_align_to_top),
    Fixture("to_edge", _noon_to_edge, _manim_to_edge),
    Fixture("to_corner", _noon_to_corner, _manim_to_corner),
    Fixture("vgroup_arrange", _noon_arrange, _manim_arrange),
    Fixture("set_xy", _noon_set_xy, _manim_set_xy),
    Fixture("next_to_point", _noon_next_to_point, _manim_next_to_point),
    Fixture("vgroup_add_remove", _noon_vgroup_add_remove, _manim_vgroup_add_remove),
    Fixture("vgroup_duplicate_add", _noon_vgroup_duplicate_add, _manim_vgroup_duplicate_add),
    Fixture("vgroup_insert", _noon_vgroup_insert, _manim_vgroup_insert),
    Fixture("vgroup_add_to_back", _noon_vgroup_add_to_back, _manim_vgroup_add_to_back),
    Fixture("vgroup_slice", _noon_vgroup_slice, _manim_vgroup_slice),
    Fixture("vgroup_duplicate_slice", _noon_vgroup_duplicate_slice, _manim_vgroup_duplicate_slice),
    Fixture("nested_family_alias", _noon_nested_family_alias, _manim_nested_family_alias),
    Fixture("vgroup_shift", _noon_vgroup_shift, _manim_vgroup_shift),
    Fixture(
        "mobject_copy_independence",
        _noon_mobject_copy_independence,
        _manim_mobject_copy_independence,
    ),
    Fixture(
        "vgroup_copy_independence",
        _noon_vgroup_copy_independence,
        _manim_vgroup_copy_independence,
    ),
    Fixture("vgroup_arrange_grid", _noon_vgroup_arrange_grid, _manim_vgroup_arrange_grid),
    Fixture("vgroup_scale", _noon_vgroup_scale, _manim_vgroup_scale),
    Fixture("vgroup_rotate", _noon_vgroup_rotate, _manim_vgroup_rotate),
    Fixture("generate_target", _noon_generate_target, _manim_generate_target),
    Fixture("save_restore", _noon_save_restore, _manim_save_restore),
    Fixture("become", _noon_become, _manim_become),
    Fixture("replace_width", _noon_replace_width, _manim_replace_width),
    Fixture("replace_stretch", _noon_replace_stretch, _manim_replace_stretch),
    Fixture("critical_points", _noon_critical_points, _manim_critical_points),
    Fixture("set_coord_direction", _noon_set_coord_direction, _manim_set_coord_direction),
    Fixture("scale_to_fit_width", _noon_scale_to_fit_width, _manim_scale_to_fit_width),
    Fixture("stretch_to_fit_height", _noon_stretch_to_fit_height, _manim_stretch_to_fit_height),
    Fixture("match_xy", _noon_match_xy, _manim_match_xy),
    Fixture("match_width", _noon_match_width, _manim_match_width),
    Fixture("match_height_stretch", _noon_match_height_stretch, _manim_match_height_stretch),
    Fixture("dimension_properties", _noon_dimension_properties, _manim_dimension_properties),
    Fixture("rotate_about_origin", _noon_rotate_about_origin, _manim_rotate_about_origin),
]

# Explicitly tracked but not yet differential-gated.  Keep this list close to the
# harness so unsupported behavior is never silently treated as a mismatch.
UNSUPPORTED = {
    "z_index": "Noon does not yet expose the 2.5D/z semantic model (#62)",
    "updater_frame_semantics": "host/native updater phase semantics are being defined in #56",
    "animation_lifecycle": "requires a reference Scene/animation-state probe, to be added incrementally",
    "stroke_scaling": "semantic stroke-width/scaling mode is being defined in #62",
    "style_channels": "fill/stroke/object opacity integration is being migrated onto the #62 semantic style contract",
}


def _compare(expected: Any, actual: Any, tolerance: float, path: str = "$") -> list[str]:
    errors: list[str] = []
    if isinstance(expected, bool) or isinstance(actual, bool):
        if expected != actual:
            errors.append(f"{path}: Noon={expected!r}, Manim={actual!r}")
        return errors
    if isinstance(expected, (int, float)) and isinstance(actual, (int, float)):
        if not math.isclose(float(expected), float(actual), rel_tol=tolerance, abs_tol=tolerance):
            errors.append(f"{path}: Noon={expected!r}, Manim={actual!r}")
        return errors
    if isinstance(expected, dict) and isinstance(actual, dict):
        if expected.keys() != actual.keys():
            errors.append(
                f"{path}: key mismatch Noon={sorted(expected.keys())}, Manim={sorted(actual.keys())}"
            )
            return errors
        for key in expected:
            errors.extend(_compare(expected[key], actual[key], tolerance, f"{path}.{key}"))
        return errors
    if isinstance(expected, (list, tuple)) and isinstance(actual, (list, tuple)):
        if len(expected) != len(actual):
            errors.append(f"{path}: length mismatch Noon={len(expected)}, Manim={len(actual)}")
            return errors
        for index, (lhs, rhs) in enumerate(zip(expected, actual)):
            errors.extend(_compare(lhs, rhs, tolerance, f"{path}[{index}]"))
        return errors
    if expected != actual:
        errors.append(f"{path}: Noon={expected!r}, Manim={actual!r}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true", help="emit machine-readable results")
    args = parser.parse_args()

    if manim.__version__ != PINNED_MANIM_VERSION:
        raise SystemExit(
            f"expected ManimCE {PINNED_MANIM_VERSION}, found {manim.__version__}; "
            "update the pin and compatibility target intentionally"
        )

    results: list[dict[str, Any]] = []
    failures = 0
    for fixture in FIXTURES:
        noon_value = fixture.noon_probe()
        manim_value = fixture.manim_probe()
        differences = _compare(noon_value, manim_value, fixture.tolerance)
        status = "pass" if not differences else "mismatch"
        failures += bool(differences)
        results.append(
            {
                "fixture": fixture.name,
                "status": status,
                "noon": noon_value,
                "manim": manim_value,
                "differences": differences,
            }
        )

    payload = {
        "manim_version": manim.__version__,
        "fixtures": results,
        "unsupported": UNSUPPORTED,
    }
    if args.json:
        print(json.dumps(payload, indent=2, sort_keys=True))
    else:
        for result in results:
            marker = "PASS" if result["status"] == "pass" else "FAIL"
            print(f"[{marker}] {result['fixture']}")
            for difference in result["differences"]:
                print(f"  {difference}")
        print(f"\n{len(FIXTURES) - failures}/{len(FIXTURES)} supported fixtures match ManimCE {manim.__version__}")
        if UNSUPPORTED:
            print(f"{len(UNSUPPORTED)} explicitly unsupported/deferred semantic areas")

    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
