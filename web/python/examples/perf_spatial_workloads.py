"""Bounded Phase D spatial workloads for the existing performance corpus.

Select one case through the corpus context's ``workload`` key. These are
measurement fixtures, not a claim of device-level frame-rate qualification.
"""

import math

from noon import *


_WORKLOADS = {
    "camera-only",
    "moving-mesh-many-static",
    "dense-surface-family",
    "moving-point-light",
    "mixed-depth-text-hud",
}
_ALLOWED_CONTEXT_KEYS = {
    "workload", "duration", "static_mesh_count", "surface_resolution",
}
_DEFAULT_DURATION = 10.0


def _validated_workload_context(raw):
    """Validate all caller-controlled sizes before any scene objects are built."""
    if not isinstance(raw, dict):
        raise TypeError("performance workload context must be a dictionary")
    unknown = set(raw) - _ALLOWED_CONTEXT_KEYS
    if unknown:
        raise ValueError(f"unsupported performance context key: {sorted(unknown)[0]}")

    workload = raw.get("workload", "camera-only")
    if not isinstance(workload, str) or workload not in _WORKLOADS:
        raise ValueError(f"workload must be one of {sorted(_WORKLOADS)}")

    duration = raw.get("duration", _DEFAULT_DURATION)
    if isinstance(duration, bool) or not isinstance(duration, (int, float)):
        raise TypeError("duration must be a finite number")
    duration = float(duration)
    if not math.isfinite(duration) or not 8.0 <= duration <= 10.0:
        raise ValueError("duration must be between 8 and 10 seconds")

    static_mesh_count = raw.get("static_mesh_count", 600)
    if isinstance(static_mesh_count, bool) or not isinstance(static_mesh_count, int):
        raise TypeError("static_mesh_count must be an integer")
    if not 1 <= static_mesh_count <= 600:
        raise ValueError("static_mesh_count must be between 1 and 600")

    surface_resolution = raw.get("surface_resolution", 24)
    if isinstance(surface_resolution, bool) or not isinstance(surface_resolution, int):
        raise TypeError("surface_resolution must be an integer")
    if not 8 <= surface_resolution <= 32:
        raise ValueError("surface_resolution must be between 8 and 32")

    return {
        "workload": workload,
        "duration": duration,
        "static_mesh_count": static_mesh_count,
        "surface_resolution": surface_resolution,
    }


class PhaseDSpatialPerformance(ThreeDScene):
    """Context-selected, size-bounded spatial cadence and locality workload."""

    def __init__(self):
        super().__init__(near=0.1, far=80)

    async def construct(self):
        config = _validated_workload_context(globals().get("context", {}))
        self.set_camera_orientation(
            phi=0.82,
            theta=-1.08,
            gamma=0.0,
            focal_distance=8.0,
            zoom=0.9,
            frame_center=(0, 0, 0),
        )
        workload = config["workload"]
        if workload == "camera-only":
            await self._camera_only(config["duration"])
        elif workload == "moving-mesh-many-static":
            await self._moving_mesh_many_static(
                config["duration"], config["static_mesh_count"]
            )
        elif workload == "dense-surface-family":
            await self._dense_surface_family(
                config["duration"], config["surface_resolution"]
            )
        elif workload == "moving-point-light":
            await self._moving_point_light(
                config["duration"], config["surface_resolution"]
            )
        else:
            await self._mixed_depth_text_hud(config["duration"])

    async def _camera_only(self, duration):
        axes = ThreeDAxes()
        marker = Mesh3D.sphere(radius=0.18, resolution=(8, 8), color=RED)
        marker.shift((1.2, -0.5, 1.0))
        self.add_world_mobjects(axes, marker)
        await self.move_camera(
            phi=1.08,
            theta=-0.42,
            gamma=0.08,
            zoom=1.02,
            frame_center=(0.25, 0, 0),
            run_time=duration,
            rate_func=linear,
        )

    async def _moving_mesh_many_static(self, duration, static_count):
        prototype = Mesh3D.cube(size=0.1, color=BLUE)
        columns = math.ceil(math.sqrt(static_count * 16.0 / 9.0))
        rows = math.ceil(static_count / columns)
        spacing = 0.24
        static_meshes = []
        for index in range(static_count):
            column = index % columns
            row = index // columns
            x = (column - (columns - 1) / 2.0) * spacing
            y = ((rows - 1) / 2.0 - row) * spacing
            mesh = prototype.copy()
            mesh.shift((x, y, 0.0))
            static_meshes.append(mesh)

        moving = prototype.copy()
        moving.shift((0.0, 0.0, 0.65))
        self.set_camera_orientation(zoom=0.72)
        self.add_world_mobjects(*static_meshes, moving)
        await self.play(
            WorldTransformTo(
                moving,
                translation=(0.0, 0.0, 1.8),
                rotation=(math.cos(0.45), 0.0, math.sin(0.45), 0.0),
            ),
            run_time=duration,
            rate_func=linear,
        )

    async def _dense_surface_family(self, duration, resolution):
        surface = Surface(
            lambda u, v: (u, v, 0.16 * math.sin(2 * u) * math.cos(2 * v)),
            u_range=(-2.2, 2.2),
            v_range=(-1.6, 1.6),
            resolution=(resolution, resolution),
            checkerboard_colors=(BLUE_D, BLUE_E),
            fill_color=Color(0.16, 0.56, 0.86),
            fill_opacity=1.0,
            stroke_width=0.0,
            shade_in_3d=False,
        )
        self.add_world_mobjects(surface)
        # Surface is a retained family of UV cells, so this workload keeps
        # the dense geometry/material fixed and measures its camera-only path.
        await self.move_camera(
            phi=0.96,
            theta=-0.78,
            gamma=0.05,
            focal_distance=8.0,
            zoom=0.98,
            frame_center=(0.15, 0.0, 0.0),
            run_time=duration,
            rate_func=linear,
        )

    async def _moving_point_light(self, duration, resolution):
        surface = Mesh3D.parametric(
            lambda u, v: (u, v, 0.22 * (u * u - v * v)),
            u_range=(-1.8, 1.8),
            v_range=(-1.4, 1.4),
            resolution=(resolution, resolution),
            color=Color(0.18, 0.52, 0.82),
            point_lit=True,
        )
        light = self.point_light(
            position=(3.8, -3.2, 5.0),
            color=Color(1.0, 0.9, 0.7),
            intensity=1.0,
        )
        self.add_world_mobjects(surface, light)
        await self.play(
            WorldTransformTo(light, translation=(-3.8, 3.2, 5.0)),
            run_time=duration,
            rate_func=linear,
        )

    async def _mixed_depth_text_hud(self, duration):
        rear = Mesh3D.cube(size=1.0, color=BLUE)
        rear.shift((0.0, 0.0, -0.5))
        front = Mesh3D.cube(size=0.72, color=RED)
        front.shift((0.0, 0.0, 1.0))
        world_text = Text("World depth", color=WHITE)
        world_text.move_to((-2.4, 1.7))
        self.shift_world(world_text, (0.0, 0.0, 0.25))
        hud = Text("WORLD / DEPTH / HUD", color=YELLOW)
        hud.move_to((-3.6, -3.45))
        self.add_world_mobjects(rear, front, world_text)
        self.add_fixed_in_frame_mobjects(hud)
        await self.play(
            WorldTransformTo(front, translation=(0.0, 0.0, -1.2)),
            run_time=duration,
            rate_func=linear,
        )


result = PhaseDSpatialPerformance()
