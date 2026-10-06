"""Typed indexed-mesh authoring on Noon's ordinary Scene and Runtime.

These native mesh conventions are explicit: opaque fills, quaternion poses,
local +Z cylinders/cones from zero to height, and UV cell resolution. Python
evaluates an explicitly supplied sampling callback at construction; Rust owns
sampling coordinates, topology, normals, transforms, and playback.
"""
from __future__ import annotations

import math
from array import array
from operator import index as _integer_index

import noon as _base
from _noon_errors import engine_call
from _manim_semantic_handles import (
    _attach_shared_handle, _handle_for, _live_constructor_context,
    _live_mutation_context, _group_target_context,
)

try:
    from js import (
        noonAuthoringMeshOptions as _mesh_options,
        noonSurfaceSamplingPlan as _surface_plan,
        noonAuthoringMeshFamilyOptions as _mesh_family_options,
        noonCreateAuthoringMeshHandle as _create_mesh,
        noonCreateAuthoringMeshFamilyHandle as _create_mesh_family,
    )
except ImportError:
    _mesh_options = _surface_plan = _mesh_family_options = None
    _create_mesh = _create_mesh_family = None


def _vector(value, length=3):
    values = tuple(float(component) for component in value)
    if len(values) != length:
        raise ValueError(f"expected {length} numeric components")
    return values


def _angular_range(value, name):
    try:
        values = tuple(float(item) for item in value)
    except (TypeError, ValueError) as error:
        raise TypeError(f"{name} requires two numeric endpoints") from error
    if len(values) != 2 or not all(math.isfinite(item) for item in values):
        raise ValueError(f"{name} requires two finite endpoints")
    if values[0] < 0 or values[0] >= values[1] or values[1] > 2 * math.pi:
        raise ValueError(f"{name} must be increasing within [0, 2π]")
    return values


def _bulk(values):
    from pyodide.ffi import to_js
    return to_js(memoryview(values if isinstance(values, array) and values.typecode == "d" else array("d", values)))


def _pose(value):
    if hasattr(value, "to_py"):
        value = value.to_py()
    return tuple(float(component) for component in value)


def _world_transform_values(translation=(0, 0, 0), rotation=(1, 0, 0, 0), scale=(1, 1, 1)):
    return (*_vector(translation), *_vector(rotation, 4), *_vector(scale))


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


def _prism_face_family_options(size, shade_in_3d=False):
    if _mesh_options is None:
        raise RuntimeError("prism face construction requires the shared Rust authoring host")
    size = _vector(size)
    return engine_call(_mesh_options.prismFaces, *size, shade_in_3d)


class _WorldMobject(_base.Mobject):
    """Motion routes through shared Rust authored/effective world operations."""

    def _world_call(self, authored, live, *args):
        family = getattr(self, "_semantic_family_handle", None)
        handle = family if family is not None else _handle_for(self)
        if handle is None:
            raise NotImplementedError("world edits require an ordinary typed authoring context")
        family_operations = {"shiftWorld", "rotateWorld", "scaleWorld"}
        if family is not None and authored not in family_operations | {"worldFamilyCenter"}:
            raise NotImplementedError("world transforms require one spatial Mobject; family transforms are unavailable")
        context = _group_target_context(self) if family is not None else _live_mutation_context(self)
        if family is not None:
            if context is None:
                if authored == "rotateWorld":
                    args = (args[3], _bulk(args[:3]), *args[4:])
                return engine_call(getattr(handle, authored), *args)
            family_live = {
                "shiftWorld": "shiftFamilyWorld",
                "rotateWorld": "rotateFamilyWorld",
                "scaleWorld": "scaleFamilyWorld",
                "worldFamilyCenter": "effectiveWorldFamilyCenter",
            }
            return engine_call(getattr(context, family_live[authored]), handle, *args)
        if context is None:
            return engine_call(getattr(handle, authored), *args)
        return engine_call(getattr(context, live), handle, *args)

    @property
    def world_transform(self):
        return _pose(self._world_call("worldTransform", "effectiveWorldTransform"))

    def get_center(self):
        family = getattr(self, "_semantic_family_handle", None)
        authored = "worldFamilyCenter" if family is not None else "worldCenter"
        live = "effectiveWorldFamilyCenter" if family is not None else "effectiveWorldCenter"
        return _pose(self._world_call(authored, live))

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
    def _solid(cls, kind, values, options, *, axial_pose=None):
        _mesh_arguments(options)
        if axial_pose is not None:
            direction, offset = axial_pose
            direction, offset = _vector(direction), float(offset)
        if _mesh_options is None:
            raise RuntimeError("mesh construction requires the shared Rust authoring host")
        candidate = engine_call(getattr(_mesh_options, kind), *values)
        if axial_pose is not None:
            try:
                engine_call(candidate.setAxialPose, _bulk(direction), offset)
            except BaseException:
                candidate.free()
                raise
        return cls(candidate, **options)

    @classmethod
    def sphere(cls, radius=1, resolution=(32, 16), *, u_range=(0, 2 * math.pi),
               v_range=(0, math.pi), **options):
        def checked_range(value, name):
            try:
                values = tuple(float(item) for item in value)
            except (TypeError, ValueError) as error:
                raise TypeError(f"{name} requires two numeric endpoints") from error
            if len(values) != 2 or not all(math.isfinite(item) for item in values):
                raise ValueError(f"{name} requires two finite endpoints")
            if values[0] >= values[1]:
                raise ValueError(f"{name} must be increasing")
            return values
        u_range = checked_range(u_range, "sphere u_range")
        v_range = checked_range(v_range, "sphere v_range")
        if u_range[0] < 0 or u_range[1] > 2 * math.pi:
            raise ValueError("sphere u_range must be increasing within [0, 2π]")
        if v_range[0] < 0 or v_range[1] > math.pi:
            raise ValueError("sphere v_range must be increasing within [0, π]")
        return cls._solid("sphere", (float(radius), *_resolution(resolution), *u_range, *v_range), options)

    @classmethod
    def cube(cls, size=2, **options):
        return cls._solid("cube", (float(size),), options)

    @classmethod
    def line3d(cls, start=(0, 0, 0), end=(1, 0, 0), *, thickness=0.02, segments=16, **options):
        return cls._solid("line3D", (_bulk(_vector(start)), _bulk(_vector(end)),
                                    float(thickness), _count(segments)), options)

    @classmethod
    def polyhedron(cls, vertices, triangular_faces, **options):
        _mesh_arguments(options)
        from pyodide.ffi import to_js
        points = array("d", (v for point in vertices for v in _vector(point)))
        faces = array("I")
        for face in triangular_faces:
            face = tuple(face)
            if len(face) != 3:
                raise ValueError("polyhedron faces require three indices")
            faces.extend(_count(index) for index in face)
        return cls._solid("polyhedron", (_bulk(points), to_js(memoryview(faces))), options)

    @classmethod
    def prism(cls, dimensions=(2, 2, 2), **options):
        return cls._solid("prism", _vector(dimensions), options)

    @classmethod
    def cylinder(cls, radius=1, height=2, segments=32, *, direction=(0, 0, 1),
                 axial_offset=0, show_ends=True, v_range=(0, 2 * math.pi), **options):
        """Orient the retained local +Z cylinder after applying a local Z offset."""
        if not isinstance(show_ends, bool):
            raise TypeError("show_ends requires a boolean")
        v_range = _angular_range(v_range, "cylinder v_range")
        return cls._solid("cylinder", (float(radius), float(height), _count(segments), show_ends,
                                        *v_range),
                          options, axial_pose=(direction, axial_offset))

    @classmethod
    def cone(cls, radius=1, height=2, segments=32, *, direction=(0, 0, 1),
             axial_offset=0, show_base=True, v_range=(0, 2 * math.pi), **options):
        """Orient the retained local +Z cone after applying a local Z offset."""
        if not isinstance(show_base, bool):
            raise TypeError("show_base requires a boolean")
        v_range = _angular_range(v_range, "cone v_range")
        return cls._solid("cone", (float(radius), float(height), _count(segments), show_base,
                                    *v_range),
                          options, axial_pose=(direction, axial_offset))

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
        if not isinstance(mobject, _base.Mobject):
            raise TypeError("WorldTransformTo requires a Mobject")
        self.mobject = mobject
        self.endpoint = _world_transform_values(translation, rotation, scale)
        self.anim_args = dict(kwargs)


class SpatialScene(_base.Scene):
    """One Scene/canvas with an explicit perspective camera and indexed meshes."""

    def __init__(self, *, position=(0, 0, 5), rotation=(1, 0, 0, 0),
                 vertical_fov=1, near=0.1, far=100):
        super().__init__()
        self._initialize_camera("createCamera3D", _bulk(_vector(position)),
                                _bulk(_vector(rotation, 4)), float(vertical_fov), float(near), float(far))

    def _initialize_camera(self, method, *args):
        from _manim_scene import _context, _reserve_typed_binding, _commit_typed_binding
        from _manim_semantic_handles import _initialize_shared_wrapper
        camera = object.__new__(_WorldMobject)
        _initialize_shared_wrapper(camera)
        context = _context(self)
        object_id = self._next_object_id
        handle = engine_call(getattr(context, method), str(object_id), *args)
        _attach_shared_handle(camera, handle)
        reservation = _reserve_typed_binding(camera, self, handle, None, object_id=object_id)
        _commit_typed_binding(camera, self, reservation, handle)
        self.camera = camera

    def _edit_membership(self, kind, values=(), *, key=None):
        from _manim_scene import _canonical_edit_membership
        _canonical_edit_membership(self, kind, values, key=key,
                                   spatial_domain="world_default" if kind == "add" else None)

    def point_light(self, position=(4, -3, 6), color=_base.WHITE, intensity=1):
        from _manim_compat import _as_color
        from _manim_scene import _context
        fill = _as_color("light color", color)
        handle = engine_call(_context(self).createPointLight3D, _bulk(_vector(position)),
                             _bulk((fill.red, fill.green, fill.blue, fill.alpha)), float(intensity))
        light = object.__new__(_WorldMobject)
        _attach_shared_handle(light, handle)
        light._semantic_point_light = True
        if hasattr(self, "_manim_camera_light_source") and self._manim_camera_light_source is None:
            self._manim_camera_light_source = light
        return light

    def add_world_mobjects(self, *mobjects):
        from _manim_scene import _canonical_edit_membership
        _canonical_edit_membership(self, "add", mobjects, spatial_domain="world")
        return self

    def add_fixed_in_frame_mobjects(self, *mobjects):
        from _manim_scene import _canonical_edit_membership
        _canonical_edit_membership(self, "add", mobjects, spatial_domain="fixed_frame")
        return self

    def add_fixed_orientation_mobjects(self, *mobjects):
        from _manim_scene import _canonical_edit_membership
        _canonical_edit_membership(self, "add", mobjects, spatial_domain="fixed_orientation")
        return self

    def set_world_transform(self, mobject, *, translation=(0, 0, 0),
                            rotation=(1, 0, 0, 0), scale=(1, 1, 1)):
        """Set one typed spatial Mobject's authored pose without playback."""
        from _manim_scene import _context
        if not isinstance(mobject, _base.Mobject):
            raise TypeError("set_world_transform requires a typed Mobject")
        if getattr(mobject, "_semantic_family_handle", None) is not None:
            raise TypeError("set_world_transform requires one Mobject, not a family")
        handle = _handle_for(mobject)
        if handle is None:
            raise TypeError("set_world_transform requires one active typed Mobject")
        endpoint = _world_transform_values(translation, rotation, scale)
        engine_call(_context(self).setWorldTransform, handle, _bulk(endpoint))
        return self

    def shift_world(self, mobject, vector):
        from _manim_scene import _context
        context = _context(self)
        family = getattr(mobject, "_semantic_family_handle", None)
        if family is not None:
            engine_call(context.shiftFamilyWorld, family, *_vector(vector))
        else:
            handle = _handle_for(mobject)
            if handle is None:
                raise TypeError("world shift requires a typed Mobject or Group")
            engine_call(context.shiftWorld, handle, *_vector(vector))
        return self


class CameraProfileTo:
    """Inert finite camera endpoint sampled by the existing Rust timeline."""

    def __init__(self, mobject, endpoint, **kwargs):
        self.mobject = mobject
        self.endpoint = _vector(endpoint, 9)
        self.anim_args = dict(kwargs)


class ThreeDScene(SpatialScene):
    """Finite perspective, one camera, and explicit camera driver ownership.

    Camera moves use Manim's unwrapped angles. Ambient rotation must stop before
    another camera edit. Exponential projection and behind-camera fallback are
    outside this finite perspective profile.
    """

    def __init__(self, *, near=0.1, far=100):
        _base.Scene.__init__(self)
        self._manim_camera_light_source = None
        self._renderer_proxy = _RendererProxy(self)
        from math import pi
        self._initialize_camera("createCamera3DProfile",
                                _bulk((0, -pi / 2, 0, 20, 1, 8, 0, 0, 0)), float(near), float(far))

    @property
    def renderer(self):
        return self._renderer_proxy

    def _camera_endpoint(self, *, phi=None, theta=None, gamma=None,
                         focal_distance=None, zoom=None, frame_height=None,
                         frame_center=None):
        from _manim_scene import _context
        # Transient argument assembly; Rust remains the camera/profile authority.
        values = list(_pose(engine_call(_context(self).effectiveCameraProfile,
                                        _handle_for(self.camera))))
        for index, value in enumerate((phi, theta, gamma, focal_distance, zoom, frame_height)):
            if value is not None:
                values[index] = float(value)
        if frame_center is not None:
            values[6:9] = _vector(frame_center)
        return tuple(values)

    def set_camera_orientation(self, *, phi=None, theta=None, gamma=None,
                               zoom=None, focal_distance=None, frame_center=None):
        from _manim_scene import _context
        endpoint = self._camera_endpoint(phi=phi, theta=theta, gamma=gamma, zoom=zoom,
                                         focal_distance=focal_distance, frame_center=frame_center)
        engine_call(_context(self).setCameraProfile, _handle_for(self.camera), _bulk(endpoint))
        return self

    def move_camera(self, *, phi=None, theta=None, gamma=None, zoom=None,
                    focal_distance=None, frame_center=None, added_anims=(), **kwargs):
        endpoint = self._camera_endpoint(phi=phi, theta=theta, gamma=gamma, zoom=zoom,
                                         focal_distance=focal_distance, frame_center=frame_center)
        return self.play(CameraProfileTo(self.camera, endpoint), *added_anims, **kwargs)

    def begin_ambient_camera_rotation(self, rate=0.02, about="theta"):
        from _manim_scene import _context
        if about not in {"phi", "theta", "gamma"}:
            raise ValueError("ambient rotation axis must be phi, theta, or gamma")
        engine_call(_context(self).beginAmbientCameraRotation,
                    _handle_for(self.camera), float(rate), about)
        return self

    def stop_ambient_camera_rotation(self):
        from _manim_scene import _context
        engine_call(_context(self).stopAmbientCameraRotation, _handle_for(self.camera))
        return self


    def begin_3dillusion_camera_rotation(self, rate=1, origin_phi=None, origin_theta=None):
        from _manim_scene import _context
        engine_call(_context(self).begin3DIllusionCameraRotation,
                    _handle_for(self.camera), float(rate),
                    None if origin_phi is None else float(origin_phi),
                    None if origin_theta is None else float(origin_theta))
        return self

    def stop_3dillusion_camera_rotation(self):
        from _manim_scene import _context
        engine_call(_context(self).stop3DIllusionCameraRotation, _handle_for(self.camera))
        return self

class _RendererProxy:
    """Thin spelling adapter for Manim's renderer.camera.light_source slot."""

    def __init__(self, scene):
        self.camera = _RendererCameraProxy(scene)


class _RendererCameraProxy:
    def __init__(self, scene):
        self._scene = scene

    @property
    def light_source(self):
        light = self._scene._manim_camera_light_source
        if light is None:
            light = self._scene.point_light(position=(-7, -9, 10))
        # PointLight3D remains an ordinary Rust semantic object. Membership is
        # edited through the same Scene/LiveSession operation as any Mobject.
        self._scene.add_world_mobjects(light)
        return light

    @light_source.setter
    def light_source(self, light):
        if (not isinstance(light, _base.Mobject)
                or not bool(getattr(light, "_semantic_point_light", False))
                or _handle_for(light) is None):
            raise TypeError("renderer.camera.light_source requires a typed point-light Mobject")
        previous = self._scene._manim_camera_light_source
        if previous is not None and previous is not light:
            self._scene._edit_membership("remove", (previous,))
        self._scene._manim_camera_light_source = light
        self._scene.add_world_mobjects(light)
