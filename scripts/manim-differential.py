#!/usr/bin/env python3
"""Differential semantic probes against a pinned ManimCE reference.

This suite intentionally compares small, renderer-independent observables.  It is
not a screenshot test and it does not require constructing a Manim Scene.  Noon probes run in Pyodide with the shared Rust/WASM host; this CPython
comparison reads their observations and executes the equivalent pinned ManimCE
probes. It reports a structural diff on mismatch. Serialized observations are a
test boundary, never engine input.

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


try:
    import manim as manim  # noqa: E402
except ModuleNotFoundError as exc:
    if exc.name != "manim":
        raise
    # Noon probes run in Pyodide with the real Rust host; the pinned reference
    # runs separately in CPython. Neither host emulates the other engine.
    manim = None

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


def _group_slice_observation(module: Any, *, require_shared_family: bool) -> Any:
    first = module.Square(side_length=0.5).shift(module.LEFT * 2.0)
    middle = module.Circle(radius=0.3)
    last = module.Rectangle(width=0.8, height=0.4).shift(module.RIGHT * 2.0)
    family = module.VGroup(first, middle, last)
    whole = family[:]
    selected = family[1:]
    reversed_family = family[::-1]
    empty = family[9:]
    plain = module.Group(first, middle, last)[::2]

    if require_shared_family:
        for sliced in (whole, selected, reversed_family, empty, plain):
            assert sliced._semantic_family_handle is not family._semantic_family_handle
        assert list(selected) == [middle, last]
        assert list(reversed_family) == [last, middle, first]

    integer_identity = family[-2] is middle
    selected.shift(module.UP * 0.75)
    return {
        "whole_type": type(whole).__name__,
        "slice_type": type(selected).__name__,
        "reverse_type": type(reversed_family).__name__,
        "empty_type": type(empty).__name__,
        "plain_type": type(plain).__name__,
        "slice_len": len(selected),
        "reverse_len": len(reversed_family),
        "empty_len": len(empty),
        "plain_len": len(plain),
        "integer_identity": integer_identity,
        "family": _members_observation(family),
        "selected": _members_observation(selected),
        "reverse": _members_observation(reversed_family),
    }


def _noon_vgroup_slicing() -> Any:
    return _group_slice_observation(noon, require_shared_family=True)


def _manim_vgroup_slicing() -> Any:
    return _group_slice_observation(manim, require_shared_family=False)


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


def _grid_options_probe(api, flow, alignment_lists=True):
    members = [api.Rectangle(width=w, height=h).shift(api.RIGHT * 2)
               for w, h in ((2, 1), (1, .5), (.5, 2), (1, 1), (.7, .4))]
    group = api.VGroup(*members)
    options = dict(rows=2, cols=3, buff=(.5, .25), cell_alignment=api.UP + api.LEFT,
                   row_heights=[3, None], col_widths=[None, 2, None], flow_order=flow)
    if alignment_lists:
        options.update(row_alignments="ud", col_alignments="lcr")
    group.arrange_in_grid(**options)
    return _members_observation(group)


def _grid_inferred_and_alias_probe(api):
    a, b = api.Square(side_length=.5), api.Rectangle(width=1, height=.5)
    nested = api.VGroup(a, b)
    group = api.VGroup(nested, a)
    group.arrange_in_grid(row_alignments="u", col_widths=[None, 2],
                          cell_alignment=api.UP + api.LEFT, flow_order="ld")
    return _members_observation(group)


def _path_queries_probe(api):
    shapes = [api.Rectangle(width=2, height=1), api.Circle(radius=.8),
              api.Line(api.LEFT, api.RIGHT + api.UP)]
    result = []
    for shape in shapes:
        shape.stretch_to_fit_width(3).rotate(.37).shift(api.LEFT * .7 + api.UP * .2)
        result.append({
            "start": _point_observation(shape.get_start()),
            "end": _point_observation(shape.get_end()),
            "points": [_point_observation(shape.point_from_proportion(alpha))
                       for alpha in (0, .125, .31, .5, .79, 1)],
            "length": float(shape.get_arc_length()),
            "length_25": float(shape.get_arc_length(25)),
        })
    return result


def _z_index_probe(api):
    a, b = api.Square(z_index=2.5), api.Circle(z_index=2.5)
    nested = api.VGroup(a, b, z_index=2.5)
    family = api.VGroup(a, nested, z_index=-1.25)
    copy = family.copy()
    a.z_index = 4.5
    return [float(node.z_index) for node in
            (family, nested, a, b, copy, copy[0], copy[1])]


def _scale_pivots_probe(api):
    result = []
    for pivot in ({}, {"about_point": [0, 0, 0]}, {"about_edge": api.RIGHT}):
        line = api.Line([1, 1, 0], [3, 2, 0])
        line.scale(1.5, **pivot)
        result.append([_point_observation(line.get_start()), _point_observation(line.get_end())])
        a, b = api.Square().shift(api.LEFT), api.Square().shift(api.RIGHT)
        family = api.VGroup(a, api.VGroup(a, b))
        family.scale(1.5, **pivot)
        result.append([_point_observation(a.get_center()), _point_observation(b.get_center()), float(family.width)])
        line.scale_to_fit_width(2, **pivot)
        family.match_height(line, **pivot)
        result.append([_point_observation(line.get_start()), _point_observation(line.get_end()),
                       _object_observation(family)])
    return result


def _planar_flip_probe(api):
    observations = []
    for axis in (api.RIGHT, api.UP, api.RIGHT + api.UP, api.OUT):
        for pivot in ({}, {"about_point": 2 * api.RIGHT + api.UP}, {"about_edge": api.RIGHT + api.UP}):
            line = api.Line(api.LEFT, api.RIGHT + api.UP).rotate(.37).shift(2 * api.LEFT)
            line.flip(axis, **pivot)
            observations.append([_point_observation(line.get_start()), _point_observation(line.get_end())])
    a = api.Rectangle(width=2, height=1).rotate(.3).shift(api.LEFT)
    b = api.Line(api.ORIGIN, api.RIGHT + api.UP)
    family = api.VGroup(a, api.VGroup(a, b))
    family.flip(api.UP, about_point=api.ORIGIN).rotate_about_origin(.2)
    return {"leaves": observations, "family": _object_observation(family),
            "rectangle": _object_observation(a), "line": _object_observation(b)}


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


def _group_coordinate_dimensions(api, group_class):
    first = api.Rectangle(width=2.0, height=1.0).shift(2 * api.LEFT)
    second = api.Square(side_length=1.0).shift(2 * api.RIGHT)
    nested = group_class(first, second)
    family = group_class(first, nested)
    target = group_class(api.Rectangle(width=0.5, height=2).shift(api.RIGHT + api.UP))
    observations = []
    for operation in (
        lambda: setattr(family, "width", 4.0),
        lambda: setattr(family, "height", 2.0),
        lambda: family.set_x(3.0, api.RIGHT).set_y(-2.0, api.DOWN),
        lambda: family.match_x(target, api.LEFT).match_y(target, api.UP),
    ):
        operation()
        observations.append({
            "family": _object_observation(family),
            "first": _object_observation(first),
            "second": _object_observation(second),
            "target": _object_observation(target),
            "members_preserved": family.submobjects[0] is first
                and family.submobjects[1] is nested
                and nested.submobjects[0] is first
                and nested.submobjects[1] is second,
        })
    return observations


def _family_become(api):
    def make(width, height, distance):
        a = api.Square(side_length=width).shift(distance * api.LEFT)
        b = api.Rectangle(width=width, height=height).shift(distance * api.RIGHT)
        return api.VGroup(a, api.VGroup(a, b))
    source = make(1.0, 2.0, 2.0)
    target = make(2.0, 3.0, 3.0).shift(api.RIGHT + api.UP)
    observations = []
    for options in ({}, {"match_height": True}, {"match_height": True, "match_width": True},
                    {"stretch": True, "match_center": True}):
        value = source.copy()
        first = value[0]
        second = value[1][1]
        value.become(target, **options)
        observations.append({"family": _object_observation(value),
                             "first": _object_observation(first), "second": _object_observation(second),
                             "alias": first is value[1][0], "target": _object_observation(target)})
    source.save_state()
    source.become(target)
    source.restore()
    return {"states": observations, "restored": _object_observation(source)}


def _family_become_cross_alias(api):
    observations = []
    for match_center in (False, True):
        a = api.Square(side_length=1).shift(api.LEFT)
        b = api.Square(side_length=1).shift(api.RIGHT)
        source = api.VGroup(a, b)
        target = api.VGroup(b, a)
        source.become(target, match_center=match_center)
        observations.append([_object_observation(a), _object_observation(b)])
    return observations


def _style_operations(api):
    a = api.Square(side_length=1).shift(api.LEFT)
    b = api.Circle(radius=0.5).shift(api.RIGHT)
    family = api.VGroup(a, api.VGroup(a, b))
    palette = family.copy().set_style(fill_color="#FF0000", fill_opacity=0.3,
                                     stroke_color="#0000FF", stroke_width=6, stroke_opacity=0.6)
    family.match_style(palette)
    family.set_fill().set_stroke().match_style(family)
    a.match_style(a)
    observations = [[m.get_fill_opacity(), m.get_stroke_opacity(), _object_observation(m)] for m in [a, b]]
    a.set_style(fill_opacity=0.7)
    family.match_style(api.VGroup(b, api.VGroup(b, a)))
    observations.append([a.get_fill_opacity(), b.get_fill_opacity()])
    return observations


def _family_membership_order(provider: Any, group_name: str) -> Any:
    a, b, c = [provider.Square(side_length=1.0) for _ in range(3)]
    group_type = getattr(provider, group_name)
    nested = group_type(a, b)
    family = group_type(a, nested, a)
    names = {id(a): "a", id(b): "b", id(c): "c", id(nested): "nested"}
    order = lambda: [names[id(member)] for member in family.submobjects]
    observed = [order()]
    family.add(c, nested, c)
    observed.append(order())
    family.add(a)
    observed.append(order())
    family.remove(nested, nested)
    observed.append(order())
    family.add(nested)
    observed.append(order())
    copied = family.copy()
    return {"order": observed, "nested_identity": nested[0] is a and nested[1] is b,
            "copy_alias": copied[1] is copied[2][0],
            "copy_independent": copied[1] is not a}


def _family_replace(api, source_family, target_family, stretch):
    first = api.Rectangle(width=2.0, height=1.0).shift(2 * api.LEFT)
    second = api.Square(side_length=1.0).shift(2 * api.RIGHT)
    target = api.Rectangle(width=3.0, height=2.0).shift(api.RIGHT + api.UP)
    source = api.VGroup(first, api.VGroup(first, second)) if source_family else first
    target = api.Group(target) if target_family else target
    source.replace(target, stretch=stretch)
    return {"source": _object_observation(source), "target": _object_observation(target),
            "first": _object_observation(first), "second": _object_observation(second)}


def _shared_target_replace(api):
    first = api.Square(side_length=1).shift(api.LEFT)
    second = api.Square(side_length=1).shift(api.RIGHT)
    family = api.VGroup(first, second)
    family.replace(first)
    return {"family": _object_observation(family), "first": _object_observation(first),
            "second": _object_observation(second)}


def _zero_extent_replace(api):
    source = api.Line(api.ORIGIN, 2 * api.UP)
    target = api.Rectangle(width=4, height=6).shift(3 * api.RIGHT + 2 * api.UP)
    source.replace(target, stretch=True)
    return _object_observation(source)


def _paint_rgb(color):
    # Normalize provider color representations only; never alter scene semantics.
    if hasattr(color, "to_rgb"):
        return [float(value) for value in color.to_rgb()]
    return [color.red, color.green, color.blue]


def _paint_queries_gradients(api):
    boxes = [api.Square(side_length=1) for _ in range(5)]
    group = api.VGroup(boxes[0], api.VGroup(*boxes))
    group.set_fill(opacity=0.4).set_stroke(width=6)
    group.set_color_by_gradient("#FF0000", "#00FF00", "#0000FF")
    observations = [[_paint_rgb(obj.get_color()), _paint_rgb(obj.get_fill_color()),
                     _paint_rgb(obj.get_stroke_color()), obj.get_stroke_width(),
                     obj.get_fill_opacity()] for obj in boxes]
    first = boxes[0]
    first.set_fill(color="#FF0000").set_stroke(color="#0000FF")
    observations.append(_paint_rgb(first.get_color()))
    first.set_fill(opacity=0)
    observations.append(_paint_rgb(first.get_color()))
    first.set_color(first.get_color())
    observations.append([first.get_fill_opacity(), first.get_stroke_opacity()])
    return observations


def _arc_geometry(api):
    arc = api.Arc(
        radius=1.25,
        start_angle=-0.3,
        angle=1.8,
        num_components=9,
        arc_center=(-2.0, 0.8, 0.0),
    )
    implicit = api.ArcBetweenPoints(
        (-0.5, -1.5, 0.0),
        (2.5, 1.0, 0.0),
        angle=api.PI / 2,
    )
    negative_radius = api.ArcBetweenPoints(
        (0.5, -2.0, 0.0),
        (3.0, -2.0, 0.0),
        radius=-2.0,
    )
    return {
        "arc": _object_observation(arc),
        "implicit": _object_observation(implicit),
        "negative_radius": _object_observation(negative_radius),
    }


def _path_family_arrangement(api):
    results = []
    for grid in (False, True):
        a = api.Arc(radius=1, start_angle=-0.3, angle=1.8, num_components=3)
        b = a.copy()
        family = api.VGroup(a, b)
        if grid:
            family.arrange_in_grid(rows=1, cols=2, buff=(0.25, 0.25))
        else:
            family.arrange(api.RIGHT, buff=0.25)
        results.append([_object_observation(obj) for obj in (a, b, family)])
    return results


def _canonical_curve_layout(api):
    circle = api.Circle(radius=1).stretch_to_fit_width(4).stretch_to_fit_height(1.5)
    ellipse = api.Ellipse(width=4, height=1.5)
    for obj in (circle, ellipse):
        obj.rotate(api.PI / 6)
    family = api.VGroup(circle, ellipse)
    return [_object_observation(obj) for obj in (circle, ellipse, circle.copy(), family)]


def _path_selection(api):
    point = lambda x, y: api.RIGHT * x + api.UP * y
    source = api.VMobject().start_new_path(point(-3, -1))
    source.add_cubic_bezier_curve_to(point(-3, 2), point(0, 2), point(0, -1))
    source.start_new_path(point(1, -1)).add_line_to(point(3, 1))
    selected = source.copy().pointwise_become_partial(source, 0.2, 0.8)
    def observe(value):
        return [list(value.get_start())[:2], list(value.get_end())[:2], value.get_arc_length(), _object_observation(value)]
    result = [observe(selected)]
    selected.reverse_direction()
    result.append(observe(selected))
    selected.pointwise_become_partial(selected, 0.2, 0.8)
    result.append(observe(selected))
    return result


def _path_construction(api):
    point = lambda x, y: api.RIGHT * x + api.UP * y
    path = api.VMobject()
    path.start_new_path(point(-2, 0))
    observations = [[list(path.get_start())[:2], list(path.get_end())[:2], path.get_arc_length()]]
    path.start_new_path(point(0, 0)).add_line_to(point(2, 0))
    path.start_new_path(point(5, 1))
    observations.append([list(path.get_start())[:2], list(path.get_end())[:2], path.get_arc_length(), list(path.point_from_proportion(1))[:2]])
    path.add_quadratic_bezier_curve_to(point(6, 2), point(7, 1))
    path.add_cubic_bezier_curve_to(point(8, 1), point(8, 0), point(7, 0)).close_path()
    observations.append([_object_observation(path), path.get_arc_length(), list(path.get_end())[:2]])
    path.add_line_to(point(8, 1))
    observations.append([_object_observation(path), path.get_arc_length(), list(path.get_end())[:2]])
    return observations


def _path_editing(api):
    path = api.VMobject(color="#58c4dd").set_points_as_corners([(-2, -1, 0), (0, 1, 0), (2, -1, 0)])
    original = path.copy()
    path.shift(api.RIGHT * 4).rotate(0.6)
    path.set_points_as_corners([(1, -1, 0), (3, -1, 0), (2, 1, 0), (1, -1, 0)])
    return [[_object_observation(obj), list(obj.get_start())[:2], list(obj.get_end())[:2], obj.get_arc_length()] for obj in (path, original)]


def _point_matching(api):
    source = api.Square(side_length=1, color="#58c4dd", fill_opacity=0.3, z_index=3.5)
    target = api.Arc(radius=1.4, start_angle=-0.4, angle=4.7, num_components=9).rotate(0.2)
    source.match_points(target)
    line = api.Line(api.LEFT, api.RIGHT)
    line.match_points(api.Ellipse(width=2, height=1).rotate(0.6))
    return [_object_observation(source), source.z_index, _paint_rgb(source.get_color()),
            _object_observation(line), list(line.get_start())[:2], list(line.get_end())[:2]]


FIXTURES = [
    Fixture("path_selection", lambda: _path_selection(noon), lambda: _path_selection(manim), 1e-5),
    Fixture("path_construction", lambda: _path_construction(noon), lambda: _path_construction(manim), 1e-5),
    Fixture("path_editing", lambda: _path_editing(noon), lambda: _path_editing(manim), 1e-5),
    Fixture("point_matching", lambda: _point_matching(noon), lambda: _point_matching(manim), 1e-5),
    Fixture("canonical_curve_layout", lambda: _canonical_curve_layout(noon), lambda: _canonical_curve_layout(manim), 1e-5),
    Fixture("path_family_arrangement", lambda: _path_family_arrangement(noon), lambda: _path_family_arrangement(manim), 1e-5),
    Fixture("arc_geometry", lambda: _arc_geometry(noon), lambda: _arc_geometry(manim), 1e-5),
    Fixture("z_index", lambda: _z_index_probe(noon), lambda: _z_index_probe(manim)),
    Fixture("scale_pivots", lambda: _scale_pivots_probe(noon), lambda: _scale_pivots_probe(manim)),
    Fixture("path_queries", lambda: _path_queries_probe(noon), lambda: _path_queries_probe(manim)),
    Fixture("planar_flip_pivots", lambda: _planar_flip_probe(noon), lambda: _planar_flip_probe(manim)),

    Fixture("vgroup_become_cross_alias", lambda: _family_become_cross_alias(noon), lambda: _family_become_cross_alias(manim)),
    Fixture("vgroup_become_restore", lambda: _family_become(noon), lambda: _family_become(manim)),
    Fixture("paint_queries_gradients", lambda: _paint_queries_gradients(noon), lambda: _paint_queries_gradients(manim)),
    Fixture("style_operations", lambda: _style_operations(noon), lambda: _style_operations(manim)),


    Fixture("group_membership_order", lambda: _family_membership_order(noon, "Group"), lambda: _family_membership_order(manim, "Group")),
    Fixture("vgroup_membership_order", lambda: _family_membership_order(noon, "VGroup"), lambda: _family_membership_order(manim, "VGroup")),
    *[Fixture(f"replace_family_{source_family}_{target_family}_{stretch}",
              lambda sf=source_family, tf=target_family, st=stretch: _family_replace(noon, sf, tf, st),
              lambda sf=source_family, tf=target_family, st=stretch: _family_replace(manim, sf, tf, st))
      for source_family, target_family, stretch in ((True, False, False), (False, True, True), (True, True, True))],
    Fixture("replace_shared_target", lambda: _shared_target_replace(noon), lambda: _shared_target_replace(manim)),
    Fixture("replace_zero_extent", lambda: _zero_extent_replace(noon), lambda: _zero_extent_replace(manim)),

    Fixture("group_coordinate_dimensions",
            lambda: _group_coordinate_dimensions(noon, noon.Group),
            lambda: _group_coordinate_dimensions(manim, manim.Group)),
    Fixture("vgroup_coordinate_dimensions",
            lambda: _group_coordinate_dimensions(noon, noon.VGroup),
            lambda: _group_coordinate_dimensions(manim, manim.VGroup)),
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
    Fixture("vgroup_shift", _noon_vgroup_shift, _manim_vgroup_shift),
    Fixture("vgroup_slicing", _noon_vgroup_slicing, _manim_vgroup_slicing),
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
    *[Fixture(f"grid_options_{flow}_{aligned}",
              lambda f=flow, a=aligned: _grid_options_probe(noon, f, a),
              lambda f=flow, a=aligned: _grid_options_probe(manim, f, a))
      for flow in ("rd", "dr", "ld", "dl", "ru", "ur", "lu", "ul")
      for aligned in (False, True)],
    Fixture("grid_inferred_alias", lambda: _grid_inferred_and_alias_probe(noon),
            lambda: _grid_inferred_and_alias_probe(manim)),
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
    "family_insert_assignment": "duplicate-edge insert and indexed assignment remain under #74",
    "family_aliasing": "Shared semantic identity exists; exhaustive nested-family mutation/copy parity remains under #74",
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


def noon_observations() -> dict[str, Any]:
    """Observe the existing probes in a host with shared Rust authoring installed."""
    return {fixture.name: fixture.noon_probe() for fixture in FIXTURES}


def load_noon_observations(path: Path) -> dict[str, Any]:
    observations = json.loads(path.read_text())
    expected = {fixture.name for fixture in FIXTURES}
    if not isinstance(observations, dict) or set(observations) != expected:
        raise ValueError("Noon observations must contain exactly the supported fixture names")
    return observations


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true", help="emit machine-readable results")
    parser.add_argument("--noon-observations", type=Path, required=True,
                        help="observations from manim-differential-noon.mjs")
    args = parser.parse_args()
    observations = load_noon_observations(args.noon_observations)

    if manim is None:
        raise SystemExit("Install the pinned ManimCE reference to compare observations")
    if manim.__version__ != PINNED_MANIM_VERSION:
        raise SystemExit(
            f"expected ManimCE {PINNED_MANIM_VERSION}, found {manim.__version__}; "
            "update the pin and compatibility target intentionally"
        )

    results: list[dict[str, Any]] = []
    failures = 0
    for fixture in FIXTURES:
        noon_value = observations[fixture.name]
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
