"""Typed indexed-mesh authoring on Noon's ordinary Scene and Runtime.

These native mesh conventions are explicit: opaque fills, no mesh strokes,
quaternion poses, z-axis cylinders/cones from zero to height, and UV cell
resolution. They do not claim Manim Cairo surface shading or class defaults.
Python evaluates an explicitly supplied sampling callback at construction;
Rust owns sampling coordinates, topology, normals, transforms and playback.
"""
from __future__ import annotations

from array import array
from operator import index as _integer_index

import noon as _base
from _noon_errors import engine_call
from _manim_semantic_handles import (
    _attach_shared_handle, _handle_for, _live_constructor_context,
    _live_mutation_context,
)

try:
    from js import (
        noonAuthoringMeshOptions as _mesh_options,
        noonSurfaceSamplingPlan as _surface_plan,
        noonCreateAuthoringMeshHandle as _create_mesh,
    )
except ImportError:
    _mesh_options = _surface_plan = _create_mesh = None


def _vector(value, length=3):
    values = tuple(float(component) for component in value)
    if len(values) != length:
        raise ValueError(f"expected {length} numeric components")
    return values


def _bulk(values):
    from pyodide.ffi import to_js
    return to_js(memoryview(values if isinstance(values, array) and values.typecode == "d" else array("d", values)))


def _pose(value):
    if hasattr(value, "to_py"):
        value = value.to_py()
    return tuple(float(component) for component in value)


def _count(value):
    if isinstance(value, bool):
        raise TypeError("mesh sample counts require integers")
    value = _integer_index(value)
    if not 0 <= value <= 0xffffffff:
        raise ValueError("mesh sample count is outside the WASM integer range")
    return value


def _resolution(value):
    values = tuple(value)
    if len(values) != 2:
        raise ValueError("surface resolution requires two cell counts")
    return tuple(_count(component) for component in values)


def _mesh_arguments(options):
    unknown = options.keys() - {"color", "point_lit"}
    if unknown:
        raise TypeError(f"unsupported mesh argument: {sorted(unknown)[0]}")
    if not isinstance(options.get("point_lit", False), bool):
        raise TypeError("point_lit requires a boolean")


class _WorldMobject(_base.Mobject):
    """Motion routes through shared Rust authored/effective world operations."""

    def _world_call(self, authored, live, *args):
        handle = _handle_for(self)
        if handle is None:
            raise NotImplementedError("world edits require an ordinary typed authoring context")
        context = _live_mutation_context(self)
        if context is None:
            return engine_call(getattr(handle, authored), *args)
        return engine_call(getattr(context, live), handle, *args)

    @property
    def world_transform(self):
        return _pose(self._world_call("worldTransform", "effectiveWorldTransform"))

    def get_center(self):
        return self.world_transform[:3]

    @property
    def animate(self):
        raise NotImplementedError("spatial playback uses WorldTransformTo with an explicit world endpoint")

    def move_to(self, point):
        point = _vector(point)
        center = self.get_center()
        return self.shift(tuple(target - current for target, current in zip(point, center)))

    def shift(self, vector):
        self._world_call("shiftWorld", "shiftWorld", *_vector(vector))
        return self

    def rotate(self, angle, axis=(0, 0, 1), about_point=None):
        pivot = _bulk(()) if about_point is None else _bulk(_vector(about_point))
        self._world_call("rotateWorld", "rotateWorld", *_vector(axis), float(angle), pivot)
        return self

    def scale(self, factor, about_point=None):
        pivot = _bulk(()) if about_point is None else _bulk(_vector(about_point))
        self._world_call("scaleWorld", "scaleWorld", float(factor), pivot)
        return self


class Mesh3D(_WorldMobject):
    """An immutable indexed mesh with an independent ordinary semantic pose."""

    def __init__(self, candidate, *, color=_base.BLUE, point_lit=False):
        from _manim_compat import _as_color
        try:
            _mesh_arguments({"color": color, "point_lit": point_lit})
            fill = _as_color("mesh color", color)
            engine_call(candidate.setColor, fill.red, fill.green, fill.blue, fill.alpha)
            engine_call(candidate.setPointLit, point_lit)
            context = _live_constructor_context("mesh")
        except BaseException:
            candidate.free()
            raise
        handle = (engine_call(_create_mesh, candidate) if context is None
                  else engine_call(context.createMesh, candidate))
        _attach_shared_handle(self, handle)
        if context is not None:
            self._canonical_live_target_context = context

    @classmethod
    def _solid(cls, kind, values, options):
        _mesh_arguments(options)
        if _mesh_options is None:
            raise RuntimeError("mesh construction requires the shared Rust authoring host")
        return cls(engine_call(getattr(_mesh_options, kind), *values), **options)

    @classmethod
    def sphere(cls, radius=1, resolution=(32, 16), **options):
        return cls._solid("sphere", (float(radius), *_resolution(resolution)), options)

    @classmethod
    def cube(cls, size=2, **options):
        return cls._solid("cube", (float(size),), options)

    @classmethod
    def prism(cls, dimensions=(2, 2, 2), **options):
        return cls._solid("prism", _vector(dimensions), options)

    @classmethod
    def cylinder(cls, radius=1, height=2, segments=32, **options):
        return cls._solid("cylinder", (float(radius), float(height), _count(segments)), options)

    @classmethod
    def cone(cls, radius=1, height=2, segments=32, **options):
        return cls._solid("cone", (float(radius), float(height), _count(segments)), options)

    @classmethod
    def torus(cls, major_radius=3, minor_radius=1, resolution=(32, 16), **options):
        return cls._solid("torus", (float(major_radius), float(minor_radius), *_resolution(resolution)), options)

    @classmethod
    def parametric(cls, function, *, u_range=(0, 1), v_range=(0, 1),
                   resolution=(32, 32), normal=None, **options):
        _mesh_arguments(options)
        if _surface_plan is None:
            raise RuntimeError("surface sampling requires the shared Rust authoring host")
        plan = engine_call(_surface_plan, *u_range, *v_range, *_resolution(resolution))
        try:
            parameters = _pose(engine_call(plan.parameters))
            points = array("d")
            normals = array("d")
            for index in range(0, len(parameters), 2):
                u, v = parameters[index:index + 2]
                points.extend(_vector(function(u, v)))
                if normal is not None:
                    normals.extend(_vector(normal(u, v)))
            candidate = engine_call(plan.finishMesh, _bulk(points), _bulk(normals))
        finally:
            plan.free()
        return cls(candidate, **options)


class WorldTransformTo:
    """Inert world endpoint; Rust captures the source at segment activation."""

    def __init__(self, mobject, translation=(0, 0, 0), rotation=(1, 0, 0, 0),
                 scale=(1, 1, 1), **kwargs):
        if not isinstance(mobject, _WorldMobject):
            raise TypeError("WorldTransformTo requires a spatial Mobject")
        self.mobject = mobject
        self.endpoint = (*_vector(translation), *_vector(rotation, 4), *_vector(scale))
        self.anim_args = dict(kwargs)


class SpatialScene(_base.Scene):
    """One Scene/canvas with an explicit perspective camera and indexed meshes."""

    def __init__(self, *, position=(0, 0, 5), rotation=(1, 0, 0, 0),
                 vertical_fov=1, near=0.1, far=100):
        super().__init__()
        from _manim_scene import _context, _reserve_typed_binding, _commit_typed_binding
        camera = object.__new__(_WorldMobject)
        from _manim_semantic_handles import _initialize_shared_wrapper
        _initialize_shared_wrapper(camera)
        context = _context(self)
        # The context creates and binds the one camera before scene content.
        object_id = self._next_object_id
        handle = engine_call(context.createCamera3D, str(object_id), _bulk(_vector(position)),
                             _bulk(_vector(rotation, 4)), float(vertical_fov), float(near), float(far))
        _attach_shared_handle(camera, handle)
        reservation = _reserve_typed_binding(camera, self, handle, None, object_id=object_id)
        _commit_typed_binding(camera, self, reservation, handle)
        self.camera = camera

    def point_light(self, position=(4, -3, 6), color=_base.WHITE, intensity=1):
        from _manim_compat import _as_color
        from _manim_scene import _context
        fill = _as_color("light color", color)
        handle = engine_call(_context(self).createPointLight3D, _bulk(_vector(position)),
                             _bulk((fill.red, fill.green, fill.blue, fill.alpha)), float(intensity))
        light = object.__new__(_WorldMobject)
        _attach_shared_handle(light, handle)
        return light
