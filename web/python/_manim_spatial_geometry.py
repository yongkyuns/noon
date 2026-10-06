"""Bounded Manim-shaped constructors over Noon retained indexed meshes.

Surface uses Rust-owned cell families, checkerboard roles, strokes, and distinct
point-lit/Cairo materials. Cube and Prism use six-face Rust semantic families;
their default Cairo appearance is retained per face. Other wrappers expose
bounded mesh profiles and reject unsupported decorations and orientations.
"""
from __future__ import annotations

import math
from array import array
from operator import index as _index

import noon as _base
import _manim_compat as _compat
from _noon_spatial import Mesh3D as _Mesh3D, _WorldMobject
from _manim_compat import _as_color, _manim_stroke_width, _opacity
from _noon_errors import engine_call
from _manim_semantic_handles import (
    _attach_shared_family, _attach_shared_handle,
    _group_target_context, _initialize_shared_wrapper,
    _live_constructor_context, _live_mutation_context,
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


def _initialize_translucent_prism_family(
    self, dimensions, fill_color, fill_opacity, stroke_width, shade_in_3d
):
    from _noon_spatial import _create_mesh_family, _prism_face_family_options

    if not isinstance(shade_in_3d, bool):
        raise TypeError("shade_in_3d requires a boolean")
    if float(stroke_width) != 0.0:
        raise NotImplementedError("mesh edge strokes are not supported")
    fill_opacity = _opacity("fill_opacity", fill_opacity)
    fill = _as_color("fill_color", fill_color)
    context = _live_constructor_context("mesh")
    candidate = _prism_face_family_options(dimensions, shade_in_3d)
    try:
        engine_call(
            candidate.setFill,
            fill.red,
            fill.green,
            fill.blue,
            fill.alpha,
            fill_opacity,
        )
        handle = (
            engine_call(_create_mesh_family, candidate)
            if context is None
            else engine_call(context.createMeshFamily, candidate)
        )
    except BaseException:
        candidate.free()
        raise
    _initialize_shared_wrapper(self)
    _attach_shared_family(self, handle, context, _compat.VMobject)
    if context is not None:
        self._canonical_live_target_context = context


class Cube(_WorldMobject, _compat.Group):
    """Six retained face meshes with Manim's Cairo shading defaults."""

    def __init__(self, side_length=2, fill_opacity=0.75, fill_color=_base.BLUE,
                 stroke_width=0, shade_in_3d=True, **kwargs):
        _reject(kwargs, ())
        side_length = float(side_length)
        _initialize_translucent_prism_family(
            self, (side_length, side_length, side_length), fill_color,
            fill_opacity, stroke_width, shade_in_3d,
        )


class Prism(_WorldMobject, _compat.Group):
    """Six retained face meshes with Manim's Cairo shading defaults."""

    def __init__(self, dimensions=(3, 2, 1), fill_opacity=0.75, fill_color=_base.BLUE,
                 stroke_width=0, shade_in_3d=True, **kwargs):
        _reject(kwargs, ())
        dimensions = tuple(float(value) for value in dimensions)
        _initialize_translucent_prism_family(
            self, dimensions, fill_color, fill_opacity, stroke_width, shade_in_3d,
        )


class Cylinder(_Mesh3D):
    """Oriented cylinder with optional end caps.

    Rust owns the finite nonzero direction, centered placement, and bounded
    partial azimuth mesh. The pinned Manim default includes both end caps.
    """

    def __init__(self, radius=1, height=2, direction=(0, 0, 1),
                 v_range=(0, 2 * math.pi), show_ends=True, resolution=(24, 24),
                 fill_color=_base.BLUE_D, fill_opacity=1,
                 checkerboard_colors=_DEFAULT_CHECKERBOARD, stroke_width=0.5,
                 shade_in_3d=True, **kwargs):
        _reject(kwargs, ())
        if not isinstance(show_ends, bool):
            raise TypeError("show_ends requires a boolean")
        v_range = _angular_sweep("Cylinder v_range", v_range)
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
                                show_ends=show_ends,
                                v_range=v_range,
                                direction=direction, axial_offset=-float(height) / 2,
                                color=_color(fill_color))
        _take_mesh(self, mesh)


class Cone(_Mesh3D):
    """Oriented cone with a bounded azimuth sweep and optional base."""

    def __init__(self, base_radius=1, height=1, direction=(0, 0, 1),
                 show_base=False, v_range=(0, 2 * math.pi), u_min=0,
                 checkerboard_colors=False, resolution=32,
                 fill_color=_base.BLUE_D, fill_opacity=1, stroke_width=0.5,
                 shade_in_3d=True, **kwargs):
        _reject(kwargs, ())
        if not isinstance(show_base, bool):
            raise TypeError("show_base requires a boolean")
        v_range = _angular_sweep("Cone v_range", v_range)
        if float(u_min) != 0.0:
            raise NotImplementedError("Cone u_min profiles are unsupported")
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
                            show_base=show_base,
                            v_range=v_range,
                            direction=direction, axial_offset=-float(height),
                            color=_color(fill_color))
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


class Surface(_Mesh3D, _compat.Group):
    """Sampled surface; callback values feed Rust-owned UV topology.

    Cairo-compatible shading retains its sampled control points as mesh
    appearance metadata. ``point_lit`` remains Noon-native lighting.
    """

    def __init__(self, func, u_range=(0, 1), v_range=(0, 1), resolution=32,
                 fill_color=_base.BLUE_D, fill_opacity=1,
                 checkerboard_colors=_DEFAULT_CHECKERBOARD,
                 stroke_color=_DEFAULT_SURFACE_STROKE, stroke_width=0.5,
                 stroke_opacity=1, normal=None, shade_in_3d=True,
                 point_lit=False, _analytic_factory=None, **kwargs):
        from _noon_spatial import _bulk, _vector, _surface_plan, _mesh_family_options, _create_mesh_family
        _reject(kwargs, ())
        if not isinstance(shade_in_3d, bool):
            raise TypeError("shade_in_3d requires a boolean")
        if not isinstance(point_lit, bool):
            raise TypeError("point_lit requires a boolean")
        if shade_in_3d and point_lit:
            raise ValueError("shade_in_3d and point_lit select distinct Surface materials")
        if shade_in_3d and normal is not None:
            raise NotImplementedError(
                "Cairo Surface shading derives its appearance from sampled geometry; "
                "custom normal callbacks require shade_in_3d=False"
            )
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
            if _analytic_factory is not None and normal is None:
                # Analytic solids share Surface's fully validated opt-out
                # profile, but keep their typed Rust geometry factory.
                mesh = _analytic_factory(
                    color=fill, point_lit=point_lit, resolution=resolution,
                    u_range=u_range, v_range=v_range,
                )
            else:
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
            parameters = engine_call(
                plan.cairoParameters if shade_in_3d else plan.parameters
            )
            if hasattr(parameters, "to_py"):
                parameters = parameters.to_py()
            points = array("d")
            normals = array("d")
            for index in range(0, len(parameters), 2):
                u, v = float(parameters[index]), float(parameters[index + 1])
                points.extend(_vector(func(u, v)))
                if normal is not None:
                    normals.extend(_vector(normal(u, v)))
            candidate = (engine_call(plan.finishCairoCells, _bulk(points))
                         if shade_in_3d else
                         engine_call(plan.finishCells, _bulk(points), _bulk(normals)))
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
            engine_call(candidate.setCairoSurface, shade_in_3d)
            handle = (engine_call(_create_mesh_family, candidate) if context is None
                      else engine_call(context.createMeshFamily, candidate))
        except BaseException:
            candidate.free()
            raise
        _initialize_shared_wrapper(self)
        _attach_shared_family(self, handle, context, _compat.VMobject)
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
        family = getattr(self, "_semantic_family_handle", None)
        if family is None:
            raise NotImplementedError("single-mesh Surface does not support family checkerboard edits")
        context = _group_target_context(self)
        if context is None:
            engine_call(family.setFillByCheckerboard, first, second, opacity)
        else:
            engine_call(context.setSurfaceCheckerboard, family, first, second, opacity)
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
        from _manim_semantic_handles import _set_style
        return _set_style(
            self, fill_color=fill_color, fill_opacity=fill_opacity,
            stroke_color=stroke_color, stroke_width=stroke_width,
            stroke_opacity=stroke_opacity, family=family,
        )

    def copy(self):
        if getattr(self, "_semantic_family_handle", None) is not None:
            return _compat.Group.copy(self)
        return _base.Mobject.copy(self)

    def set_fill(self, color=None, opacity=None):
        return self.set_style(fill_color=color, fill_opacity=opacity)

    def set_stroke(self, color=None, width=None, opacity=None):
        return self.set_style(stroke_color=color, stroke_width=width,
                              stroke_opacity=opacity)


class Sphere(Surface):
    """Surface-sampled sphere with the pinned Manim azimuth/polar mapping."""

    def __init__(self, center=(0, 0, 0), radius=1, resolution=(24, 12),
                 u_range=(0, 2 * math.pi), v_range=(0, math.pi), **kwargs):
        from _noon_spatial import _vector

        radius = _base._ir._finite_number("radius", radius)
        if radius <= 0.0:
            raise ValueError("Sphere radius must be positive")
        center = _vector(center)
        if not all(math.isfinite(component) for component in center):
            raise ValueError("Sphere center must be finite")
        u_range = _surface_range("Sphere u_range", u_range)
        v_range = _surface_range("Sphere v_range", v_range)
        if u_range[0] < 0 or u_range[1] > 2 * math.pi or u_range[0] >= u_range[1]:
            raise ValueError("Sphere u_range must be increasing within [0, 2π]")
        if v_range[0] < 0 or v_range[1] > math.pi or v_range[0] >= v_range[1]:
            raise ValueError("Sphere v_range must be increasing within [0, π]")

        self.radius = radius

        def point(u, v):
            sin_v, cos_v = math.sin(v), math.cos(v)
            sin_u, cos_u = math.sin(u), math.cos(u)
            return (radius * cos_u * sin_v,
                    radius * sin_u * sin_v,
                    -radius * cos_v)

        def analytic_factory(*, color, point_lit, resolution, u_range, v_range):
            return _Mesh3D.sphere(
                radius, resolution=resolution, u_range=u_range, v_range=v_range,
                color=color, point_lit=point_lit,
            )

        super().__init__(point, u_range=u_range, v_range=v_range,
                         resolution=resolution, _analytic_factory=analytic_factory,
                         **kwargs)
        # Translate the parameterized origin as Manim does; this also works for
        # partial patches whose bounding-box center is not the sphere center.
        self.shift(center)


class Dot3D(Sphere):
    """Uniform-color sphere family with Manim's small-dot defaults."""

    def __init__(self, point=(0, 0, 0), radius=0.08, color=_base.WHITE,
                 resolution=(8, 8), stroke_width=0.5, shade_in_3d=True, **kwargs):
        # Dot3D's set_color after Sphere construction makes every cell uniform.
        kwargs.pop("checkerboard_colors", None)
        super().__init__(center=point, radius=radius, resolution=resolution,
                         fill_color=color, checkerboard_colors=False,
                         stroke_width=stroke_width, shade_in_3d=shade_in_3d,
                         **kwargs)


class Torus(Surface):
    """Surface-sampled torus using Manim's pinned orientation and defaults."""

    def __init__(self, major_radius=3, minor_radius=1,
                 u_range=(0, 2 * math.pi), v_range=(0, 2 * math.pi),
                 resolution=(24, 24), **kwargs):
        major_radius = _base._ir._finite_number("major_radius", major_radius)
        minor_radius = _base._ir._finite_number("minor_radius", minor_radius)
        if major_radius <= 0.0 or minor_radius <= 0.0 or minor_radius >= major_radius:
            raise ValueError("Torus radii must satisfy major_radius > minor_radius > 0")
        u_range = _surface_range("Torus u_range", u_range)
        v_range = _surface_range("Torus v_range", v_range)
        if u_range[0] >= u_range[1] or v_range[0] >= v_range[1]:
            raise ValueError("Torus ranges must be increasing")

        self.R = major_radius
        self.r = minor_radius

        def point(u, v):
            radial = major_radius - minor_radius * math.cos(v)
            return (radial * math.cos(u),
                    radial * math.sin(u),
                    -minor_radius * math.sin(v))

        # The typed native torus factory covers the complete periodic surface;
        # partial ranges still use the checked Surface callback sampler.
        analytic_factory = None
        if (u_range == (0.0, 2 * math.pi) and
                v_range == (0.0, 2 * math.pi)):
            def analytic_factory(*, color, point_lit, resolution, u_range, v_range):
                return _Mesh3D.torus(
                    major_radius, minor_radius, resolution=resolution,
                    color=color, point_lit=point_lit,
                )

        super().__init__(point, u_range=u_range, v_range=v_range,
                         resolution=resolution, _analytic_factory=analytic_factory,
                         **kwargs)


def _surface_range(name, value):
    try:
        values = tuple(_base._ir._finite_number(name, item) for item in value)
    except TypeError as error:
        raise TypeError(f"{name} requires two numeric endpoints") from error
    if len(values) != 2:
        raise ValueError(f"{name} requires two endpoints")
    return values


def _angular_sweep(name, value):
    values = _surface_range(name, value)
    if values[0] < 0 or values[0] >= values[1] or values[1] > 2 * math.pi:
        raise ValueError(f"{name} must be increasing within [0, 2π]")
    return values


__all__ = ["Cone", "Cube", "Cylinder", "Dot3D", "Line3D", "Prism", "Sphere", "Surface", "Torus"]
