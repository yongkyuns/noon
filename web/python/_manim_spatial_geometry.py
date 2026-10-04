"""Bounded Manim-shaped constructors over Noon retained indexed meshes.

Surface uses Rust-owned cell families, checkerboard roles, strokes, and point-lit
materials. Other wrappers expose bounded mesh profiles and reject unsupported
Cairo decorations, partial solids, and arbitrary axial orientations.
"""
from __future__ import annotations

import math
from array import array
from operator import index as _index

import noon as _base
from _noon_spatial import Mesh3D as _Mesh3D, _WorldMobject
from _manim_compat import _as_color, _manim_stroke_width, _opacity
from _noon_errors import engine_call
from _manim_semantic_handles import (
    _attach_shared_handle, _live_constructor_context, _live_mutation_context,
)

_DEFAULT_CHECKERBOARD = (_base.BLUE_D, _base.BLUE_E)
_DEFAULT_SURFACE_STROKE = _base.color_from_hex(0xBBBBBB)


def _color(value, name="fill_color"):
    color = _as_color(name, value)
    if color.alpha != 1.0:
        raise NotImplementedError("transparent mesh materials are outside the opaque indexed-mesh profile")
    return color


def _require_opaque(fill_opacity):
    if float(fill_opacity) != 1.0:
        raise NotImplementedError("indexed mesh constructors currently require opaque fills")


def _require_unshaded(shade_in_3d):
    if shade_in_3d is not False:
        raise NotImplementedError("Manim Cairo 3D shading is not emulated by this mesh profile")


def _require_no_checkerboard(checkerboard_colors):
    if checkerboard_colors is not False:
        raise NotImplementedError("Cairo per-cell checkerboard materials are unsupported")


def _grid_resolution(value, name):
    try:
        if isinstance(value, int):
            value = (_sample_count(value, name),) * 2
        else:
            value = tuple(_sample_count(part, name) for part in value)
    except TypeError as error:
        raise TypeError(f"{name} resolution requires one integer or two integer counts") from error
    if len(value) != 2:
        raise ValueError(f"{name} resolution requires two counts")
    return value


def _sample_count(value, name):
    if isinstance(value, bool):
        raise TypeError(f"{name} resolution requires integer counts")
    count = _index(value)
    if not 1 <= count <= 0xffffffff:
        raise ValueError(f"{name} resolution counts must fit the positive WASM integer range")
    return count


def _reject(options, supported):
    unknown = sorted(set(options) - set(supported))
    if unknown:
        raise TypeError(f"unsupported 3D geometry option: {unknown[0]}")


def _take_mesh(self, mesh):
    from _manim_semantic_handles import _attach_shared_handle
    _attach_shared_handle(self, mesh._semantic_handle)
    context = getattr(mesh, "_canonical_live_target_context", None)
    if context is not None:
        self._canonical_live_target_context = context


def _axis_z(direction, name):
    try:
        direction = tuple(float(value) for value in direction)
    except (TypeError, ValueError) as error:
        raise TypeError(f"{name} direction must be a numeric 3-vector") from error
    if len(direction) != 3 or direction != (0.0, 0.0, 1.0):
        raise NotImplementedError(f"{name} currently supports only the +Z direction")


class Sphere(_Mesh3D):
    """Full opaque sphere mesh profile.

    The pinned Surface-derived defaults (checkerboard colors, stroke, and 3D
    shading) remain the signature defaults and raise until explicitly disabled.
    Partial ``u_range``/``v_range`` are unsupported.
    """

    def __init__(self, center=(0, 0, 0), radius=1, resolution=(24, 12),
                 u_range=(0, 2 * math.pi), v_range=(0, math.pi),
                 fill_color=_base.BLUE_D, fill_opacity=1,
                 checkerboard_colors=_DEFAULT_CHECKERBOARD, stroke_width=0.5,
                 shade_in_3d=True, **kwargs):
        _reject(kwargs, ())
        if tuple(u_range) != (0, 2 * math.pi) or tuple(v_range) != (0, math.pi):
            raise NotImplementedError("partial Sphere ranges are not supported by the solid mesh factory")
        _require_opaque(fill_opacity)
        _require_no_checkerboard(checkerboard_colors)
        _require_unshaded(shade_in_3d)
        if float(stroke_width) != 0.0:
            raise NotImplementedError("mesh edge strokes are unsupported")
        mesh = _Mesh3D.sphere(float(radius), resolution=_grid_resolution(resolution, "Sphere"),
                              color=_color(fill_color))
        mesh.move_to(center)
        _take_mesh(self, mesh)


class Dot3D(Sphere):
    """Small retained spherical point with the pinned (8, 8) mesh profile."""

    def __init__(self, point=(0, 0, 0), radius=0.08, color=_base.WHITE,
                 resolution=(8, 8), stroke_width=0.5, shade_in_3d=True, **kwargs):
        # Manim calls set_color after Sphere construction, so its visible default
        # Dot3D color is uniform even though the intermediate Sphere is checkerboarded.
        kwargs.pop("checkerboard_colors", None)
        super().__init__(center=point, radius=radius, resolution=resolution,
                         fill_color=color, checkerboard_colors=False,
                         stroke_width=stroke_width, shade_in_3d=shade_in_3d, **kwargs)


class Cube(_Mesh3D):
    """Flat-faced mesh profile; pinned 0.75 fill opacity is retained and rejected.

    Pass ``fill_opacity=1`` to opt into the opaque indexed-mesh renderer profile.
    The pinned face shading default is also retained and rejected; pass
    ``shade_in_3d=False`` to select the unshaded indexed-mesh profile.
    """

    def __init__(self, side_length=2, fill_opacity=0.75, fill_color=_base.BLUE,
                 stroke_width=0, shade_in_3d=True, **kwargs):
        _reject(kwargs, ())
        _require_opaque(fill_opacity)
        _require_unshaded(shade_in_3d)
        if float(stroke_width) != 0.0:
            raise NotImplementedError("mesh edge strokes are not supported")
        mesh = _Mesh3D.cube(float(side_length), color=_color(fill_color))
        _take_mesh(self, mesh)


class Prism(_Mesh3D):
    """Flat-faced prism with pinned opacity/shading defaults retained and rejected."""

    def __init__(self, dimensions=(3, 2, 1), fill_opacity=0.75, fill_color=_base.BLUE,
                 stroke_width=0, shade_in_3d=True, **kwargs):
        _reject(kwargs, ())
        _require_opaque(fill_opacity)
        _require_unshaded(shade_in_3d)
        if float(stroke_width) != 0.0:
            raise NotImplementedError("mesh edge strokes are not supported")
        mesh = _Mesh3D.prism(dimensions, color=_color(fill_color))
        _take_mesh(self, mesh)


class Torus(_Mesh3D):
    """Full torus with pinned Cairo (24, 24) resolution and Surface defaults.

    The inherited checkerboard, stroke, and shading defaults are rejected unless
    callers explicitly select the supported unshaded solid profile.
    """

    def __init__(self, major_radius=3, minor_radius=1,
                 u_range=(0, 2 * math.pi), v_range=(0, 2 * math.pi),
                 resolution=(24, 24), fill_color=_base.BLUE_D, fill_opacity=1,
                 checkerboard_colors=_DEFAULT_CHECKERBOARD, stroke_width=0.5,
                 shade_in_3d=True, **kwargs):
        _reject(kwargs, ())
        if tuple(u_range) != (0, 2 * math.pi) or tuple(v_range) != (0, 2 * math.pi):
            raise NotImplementedError("partial Torus ranges are unsupported by the solid mesh factory")
        _require_opaque(fill_opacity)
        _require_no_checkerboard(checkerboard_colors)
        _require_unshaded(shade_in_3d)
        if float(stroke_width) != 0.0:
            raise NotImplementedError("mesh edge strokes are not supported")
        mesh = _Mesh3D.torus(float(major_radius), float(minor_radius),
                             resolution=_grid_resolution(resolution, "Torus"),
                             color=_color(fill_color))
        _take_mesh(self, mesh)


class Cylinder(_Mesh3D):
    """Closed +Z cylinder; Surface checkerboard/stroke/shading defaults are rejected.

    Native caps and centered placement are retained; partial sweeps and arbitrary
    directions are unsupported.
    """

    def __init__(self, radius=1, height=2, direction=(0, 0, 1),
                 v_range=(0, 2 * math.pi), show_ends=True, resolution=(24, 24),
                 fill_color=_base.BLUE_D, fill_opacity=1,
                 checkerboard_colors=_DEFAULT_CHECKERBOARD, stroke_width=0.5,
                 shade_in_3d=True, **kwargs):
        _reject(kwargs, ())
        _axis_z(direction, "Cylinder")
        if show_ends is not True:
            raise NotImplementedError("the retained cylinder mesh is closed at both ends")
        if tuple(v_range) != (0, 2 * math.pi):
            raise NotImplementedError("partial Cylinder sweeps are unsupported")
        _require_opaque(fill_opacity)
        _require_no_checkerboard(checkerboard_colors)
        _require_unshaded(shade_in_3d)
        if float(stroke_width) != 0.0:
            raise NotImplementedError("mesh edge strokes are not supported")
        if isinstance(resolution, int):
            segments = _sample_count(resolution, "Cylinder")
        else:
            resolution = tuple(resolution)
            if len(resolution) != 2 or resolution[0] != resolution[1]:
                raise NotImplementedError("Cylinder requires equal UV resolution counts")
            segments = _sample_count(resolution[0], "Cylinder")
        mesh = _Mesh3D.cylinder(float(radius), float(height), segments,
                                color=_color(fill_color))
        mesh.shift((0, 0, -float(height) / 2))
        _take_mesh(self, mesh)


class Cone(_Mesh3D):
    """Capped +Z cone profile; the pinned open-base default is explicitly rejected."""

    def __init__(self, base_radius=1, height=1, direction=(0, 0, 1),
                 show_base=False, v_range=(0, 2 * math.pi), u_min=0,
                 checkerboard_colors=False, resolution=32,
                 fill_color=_base.BLUE_D, fill_opacity=1, stroke_width=0.5,
                 shade_in_3d=True, **kwargs):
        _reject(kwargs, ())
        _axis_z(direction, "Cone")
        if show_base is not True:
            raise NotImplementedError("the pinned open-base Cone profile is not supported; pass show_base=True for a capped mesh")
        if tuple(v_range) != (0, 2 * math.pi) or float(u_min) != 0.0:
            raise NotImplementedError("partial Cone profiles are unsupported")
        _require_no_checkerboard(checkerboard_colors)
        _require_unshaded(shade_in_3d)
        _require_opaque(fill_opacity)
        if float(stroke_width) != 0.0:
            raise NotImplementedError("mesh edge strokes are unsupported")
        if isinstance(resolution, int):
            segments = _sample_count(resolution, "Cone")
        else:
            pair = _grid_resolution(resolution, "Cone")
            if pair[0] != pair[1]:
                raise NotImplementedError("Cone requires equal UV resolution counts")
            segments = pair[0]
        mesh = _Mesh3D.cone(float(base_radius), float(height), segments,
                            color=_color(fill_color))
        # Manim's +Z cone has its apex at z=0 and base at z=-height.
        mesh.shift((0, 0, -float(height)))
        _take_mesh(self, mesh)


class Line3D(_Mesh3D):
    """Capped opaque tube mesh between two points, generated by Rust."""

    def __init__(self, start=(-1, 0, 0), end=(1, 0, 0), thickness=0.02,
                 color=None, resolution=24, checkerboard_colors=_DEFAULT_CHECKERBOARD,
                 stroke_width=0.5, shade_in_3d=True, **kwargs):
        _reject(kwargs, ())
        _require_no_checkerboard(checkerboard_colors)
        _require_unshaded(shade_in_3d)
        if float(stroke_width) != 0.0:
            raise NotImplementedError("mesh edge strokes are unsupported")
        segments = _sample_count(resolution, "Line3D")
        mesh = _Mesh3D.line3d(start, end, thickness=thickness,
                              segments=segments,
                              color=_color(_base.BLUE_D if color is None else color))
        _take_mesh(self, mesh)


class Surface(_Mesh3D):
    """Sampled surface; callback values feed Rust-owned UV topology.

    Python owns only callable sampling and argument coercion. Rust owns UV cell
    roles, family identity, style publication, point-lit material and transforms.
    """

    def __init__(self, func, u_range=(0, 1), v_range=(0, 1), resolution=32,
                 fill_color=_base.BLUE_D, fill_opacity=1,
                 checkerboard_colors=_DEFAULT_CHECKERBOARD,
                 stroke_color=_DEFAULT_SURFACE_STROKE, stroke_width=0.5,
                 stroke_opacity=1, normal=None, shade_in_3d=True,
                 point_lit=False, **kwargs):
        from _noon_spatial import _bulk, _vector, _surface_plan, _mesh_family_options, _create_mesh_family
        _reject(kwargs, ())
        if not isinstance(shade_in_3d, bool):
            raise TypeError("shade_in_3d requires a boolean")
        if shade_in_3d:
            raise NotImplementedError(
                "Manim Cairo Surface shading is unsupported; pass shade_in_3d=False "
                "and opt into Noon lighting with point_lit=True"
            )
        if not isinstance(point_lit, bool):
            raise TypeError("point_lit requires a boolean")
        u_range = _surface_range("u_range", u_range)
        v_range = _surface_range("v_range", v_range)
        resolution = _grid_resolution(resolution, "Surface")
        opacity = _opacity("fill opacity", fill_opacity)
        stroke_opacity = _opacity("stroke opacity", stroke_opacity)
        stroke_width = _manim_stroke_width(stroke_width)
        fill = _color(fill_color)
        stroke = _as_color("stroke_color", stroke_color)
        if checkerboard_colors is not False:
            try:
                checkerboard_colors = tuple(checkerboard_colors)
            except TypeError as error:
                raise TypeError("checkerboard_colors requires two colors or False") from error
            if len(checkerboard_colors) != 2:
                raise ValueError("checkerboard_colors requires exactly two colors")
            checkerboard_colors = tuple(_color(value, "checkerboard color")
                                        for value in checkerboard_colors)
        if checkerboard_colors is False and stroke_width == 0.0 and not shade_in_3d:
            _require_opaque(fill_opacity)
            mesh = _Mesh3D.parametric(
                func, u_range=u_range, v_range=v_range, resolution=resolution,
                normal=normal, color=fill, point_lit=point_lit,
            )
            _take_mesh(self, mesh)
            return
        if _surface_plan is None or _mesh_family_options is None:
            raise RuntimeError("Surface construction requires the shared Rust authoring host")
        context = _live_constructor_context("mesh")
        plan = engine_call(_surface_plan, *u_range, *v_range, *resolution)
        candidate = None
        try:
            parameters = engine_call(plan.parameters)
            if hasattr(parameters, "to_py"):
                parameters = parameters.to_py()
            points = array("d")
            normals = array("d")
            for index in range(0, len(parameters), 2):
                u, v = float(parameters[index]), float(parameters[index + 1])
                points.extend(_vector(func(u, v)))
                if normal is not None:
                    normals.extend(_vector(normal(u, v)))
            candidate = engine_call(plan.finishCells, _bulk(points), _bulk(normals))
        finally:
            plan.free()
        try:
            if checkerboard_colors is False:
                engine_call(candidate.setFill, fill.red, fill.green, fill.blue,
                            fill.alpha, opacity)
            else:
                engine_call(candidate.setCheckerboard,
                            *(_bulk(tuple(component for component in
                                           (color.red, color.green, color.blue, color.alpha)))
                              for color in checkerboard_colors), opacity)
            engine_call(candidate.setStroke, stroke.red, stroke.green, stroke.blue,
                        stroke.alpha, stroke_width, stroke_opacity)
            engine_call(candidate.setPointLit, point_lit)
            handle = (engine_call(_create_mesh_family, candidate) if context is None
                      else engine_call(context.createMeshFamily, candidate))
        except BaseException:
            candidate.free()
            raise
        _attach_shared_handle(self, handle)
        if context is not None:
            self._canonical_live_target_context = context

    def set_fill_by_checkerboard(self, *colors, opacity=1):
        from _noon_spatial import _bulk
        if not colors:
            colors = _DEFAULT_CHECKERBOARD
        elif len(colors) == 1:
            colors = colors[0]
        try:
            colors = tuple(colors)
        except TypeError as error:
            raise TypeError("checkerboard fill requires two colors") from error
        if len(colors) != 2:
            raise ValueError("checkerboard fill requires exactly two colors")
        parsed = tuple(_color(value, "checkerboard color") for value in colors)
        first = _bulk((parsed[0].red, parsed[0].green, parsed[0].blue, parsed[0].alpha))
        second = _bulk((parsed[1].red, parsed[1].green, parsed[1].blue, parsed[1].alpha))
        opacity = _opacity("fill opacity", opacity)
        context = _live_mutation_context(self)
        if context is None:
            engine_call(self._semantic_handle.setFillByCheckerboard, first, second, opacity)
        else:
            engine_call(context.setSurfaceCheckerboard, self._semantic_handle,
                        first, second, opacity)
        return self

    def shift(self, vector):
        return _WorldMobject.shift(self, vector)

    def rotate(self, angle, axis=(0, 0, 1), about_point=None):
        return _WorldMobject.rotate(self, angle, axis=axis, about_point=about_point)

    def scale(self, factor, about_point=None):
        if about_point is not None:
            try:
                about_point = tuple(about_point)
            except TypeError:
                pass
            if isinstance(about_point, tuple) and len(about_point) == 2:
                about_point = (*about_point, 0.0)
        return _WorldMobject.scale(self, factor, about_point=about_point)

    def set_style(self, fill_color=None, fill_opacity=None, stroke_color=None,
                  stroke_width=None, stroke_opacity=None, family=True, **kwargs):
        if kwargs:
            raise TypeError(f"unsupported Surface style option: {sorted(kwargs)[0]}")
        if not isinstance(family, bool):
            raise TypeError("family requires a boolean")
        if not family:
            raise NotImplementedError("Surface style updates apply to its cell family")
        fill = _as_color("fill_color", fill_color) if fill_color is not None else None
        stroke = _as_color("stroke_color", stroke_color) if stroke_color is not None else None
        fill_opacity = None if fill_opacity is None else _opacity("fill opacity", fill_opacity)
        stroke_opacity = None if stroke_opacity is None else _opacity("stroke opacity", stroke_opacity)
        stroke_width = None if stroke_width is None else _manim_stroke_width(stroke_width)
        fill_components = ((fill.red, fill.green, fill.blue, fill.alpha)
                           if fill is not None else (0.0, 0.0, 0.0, 1.0))
        stroke_components = ((stroke.red, stroke.green, stroke.blue, stroke.alpha)
                             if stroke is not None else (0.0, 0.0, 0.0, 1.0))
        arguments = (fill is not None, *fill_components, fill_opacity,
                     stroke is not None, *stroke_components, stroke_width, stroke_opacity)
        context = _live_mutation_context(self)
        if context is None:
            engine_call(self._semantic_handle.setStyle, *arguments)
        else:
            engine_call(context.liveSetFamilyStyle, self._semantic_handle, *arguments)
        return self

    def set_fill(self, color=None, opacity=None):
        return self.set_style(fill_color=color, fill_opacity=opacity)

    def set_stroke(self, color=None, width=None, opacity=None):
        return self.set_style(stroke_color=color, stroke_width=width,
                              stroke_opacity=opacity)


def _surface_range(name, value):
    try:
        values = tuple(_base._ir._finite_number(name, item) for item in value)
    except TypeError as error:
        raise TypeError(f"{name} requires two numeric endpoints") from error
    if len(values) != 2:
        raise ValueError(f"{name} requires two endpoints")
    return values


__all__ = ["Cone", "Cube", "Cylinder", "Dot3D", "Line3D", "Prism", "Sphere", "Surface", "Torus"]
