"""Thin VectorScene and LinearTransformationScene composition helpers.

These classes compose the existing shared NumberPlane, Arrow, Group and
ordinary Transform/ApplyMatrix operations. Python retains wrapper references
only; Rust remains the authority for geometry and animation state.
"""

from __future__ import annotations

import inspect
from typing import Any

import noon as _base
import _manim_compat as _compat


class VectorScene(_base.Scene):
    """A small VectorScene surface over ordinary retained objects."""

    def add_plane(self, **kwargs: Any):
        from _manim_number_plane import NumberPlane

        plane = NumberPlane(**kwargs)
        self.add(plane)
        return plane

    def add_vector(
        self,
        vector: object,
        color: object | None = None,
        animate: bool = True,
        **kwargs: Any,
    ):
        from _manim_arrow import Arrow, Vector
        if kwargs:
            unknown = ", ".join(sorted(kwargs))
            raise TypeError(f"unsupported VectorScene.add_vector option(s): {unknown}")
        result = vector if isinstance(vector, Arrow) else Vector(
            vector,
            color=color if color is not None else _base.color_from_hex("#FFFF00"),
        )
        if animate:
            from _manim_growing import GrowArrow

            completion = self.play(GrowArrow(result))
            if inspect.isawaitable(completion):
                async def after_growth():
                    await completion
                    return result

                return after_growth()
        else:
            self.add(result)
        return result


class LinearTransformationScene(VectorScene):
    """LTS defaults composed from retained planes and regenerable Arrow targets.

    Coordinate labels are Rust-positioned NumberPlane labels. Ghosts are faded
    semantic copies authored into the scene at each matrix transform.
    """

    def __init__(
        self,
        include_background_plane: bool = True,
        include_foreground_plane: bool = True,
        background_plane_kwargs: dict[str, Any] | None = None,
        foreground_plane_kwargs: dict[str, Any] | None = None,
        show_coordinates: bool = False,
        show_basis_vectors: bool = True,
        basis_vector_stroke_width: float = 6.0,
        leave_ghost_vectors: bool = False,
        i_hat_color: object | None = None,
        j_hat_color: object | None = None,
        **kwargs: Any,
    ) -> None:
        super().__init__()
        self.include_background_plane = bool(include_background_plane)
        self.include_foreground_plane = bool(include_foreground_plane)
        self.show_basis_vectors = bool(show_basis_vectors)
        self.basis_vector_stroke_width = float(basis_vector_stroke_width)
        self.background_plane_kwargs = dict(background_plane_kwargs or {})
        self.foreground_plane_kwargs = dict(foreground_plane_kwargs or {})
        self.i_hat_color = i_hat_color or _base.GREEN
        self.j_hat_color = j_hat_color or _base.RED
        self._linear_transformation_kwargs = dict(kwargs)
        self.background_plane = None
        self.foreground_plane = None
        self.i_hat = None
        self.j_hat = None
        self.moving_vectors: list[object] = []
        self.transformable_mobjects: list[object] = []
        self.leave_ghost_vectors = bool(leave_ghost_vectors)
        self.show_coordinates = bool(show_coordinates)

    def setup(self) -> None:
        super().setup()
        from _manim_arrow import Vector
        from _manim_number_plane import NumberPlane

        kwargs = self._linear_transformation_kwargs
        if kwargs:
            unknown = ", ".join(sorted(kwargs))
            raise TypeError(f"unsupported LinearTransformationScene option(s): {unknown}")
        frame_width = float(_base.DEFAULT_FRAME_WIDTH)
        if self.include_background_plane:
            defaults = {
                "axis_config": {"color": "#888888"},
                "background_line_style": {
                    "stroke_color": "#888888",
                    "stroke_width": 1.0,
                },
            }
            self.background_plane = NumberPlane(
                **_merge_options(defaults, self.background_plane_kwargs)
            )
            self.add(self.background_plane)
            if self.show_coordinates:
                self.background_plane.add_coordinates()
        if self.include_foreground_plane:
            defaults = {
                "x_range": (-frame_width, frame_width, 1.0),
                "y_range": (-frame_width, frame_width, 1.0),
                "faded_line_ratio": 1,
            }
            self.foreground_plane = NumberPlane(
                **_merge_options(defaults, self.foreground_plane_kwargs)
            )
            self.add(self.foreground_plane)
        if self.show_basis_vectors:
            self.i_hat = Vector(
                _base.RIGHT,
                color=self.i_hat_color,
                stroke_width=self.basis_vector_stroke_width,
            )
            self.j_hat = Vector(
                _base.UP,
                color=self.j_hat_color,
                stroke_width=self.basis_vector_stroke_width,
            )
            self.add(self.i_hat, self.j_hat)
            self.moving_vectors.extend((self.i_hat, self.j_hat))

    def add_vector(
        self,
        vector: object,
        color: object | None = None,
        animate: bool = False,
        **kwargs: Any,
    ):
        result = super().add_vector(
            vector, color=color, animate=animate, **kwargs
        )
        if inspect.isawaitable(result):
            async def after_growth():
                completed = await result
                self.moving_vectors.append(completed)
                return completed

            return after_growth()
        self.moving_vectors.append(result)
        return result

    def add_transformable_mobject(self, *mobjects: object) -> None:
        """Add mobjects that should follow each matrix transform.

        This retains wrapper references only; semantic family membership and
        all geometry remain owned by Rust.
        """
        from _manim_semantic_handles import _family_wrapper_key
        from _manim_arrow import Arrow

        known = {
            _family_wrapper_key(mobject)
            for mobject in (*self.transformable_mobjects, *self.moving_vectors)
        }
        additions = []
        ordinary = []
        arrows = []
        for mobject in mobjects:
            key = _family_wrapper_key(mobject)
            if key in known:
                continue
            known.add(key)
            additions.append(mobject)
            if isinstance(mobject, Arrow) or getattr(mobject, "_semantic_arrow_handle", None) is not None:
                arrows.append(mobject)
            else:
                ordinary.append(mobject)
        if additions:
            self.add(*additions)
        self.transformable_mobjects.extend(ordinary)
        self.moving_vectors.extend(arrows)

    def apply_matrix(self, matrix: object, **kwargs: Any):
        """Animate the grid and vectors in one shared family Transform."""
        from _manim_animate import ApplyMatrix, Transform, _matrix_arguments
        from _manim_arrow import Arrow
        from _noon_errors import engine_call

        about_point = kwargs.pop("about_point", _base.ORIGIN)
        about = _base._as_vec2(about_point)
        requested_path_arc = kwargs.pop("path_arc", None)
        run_time = float(kwargs.get("run_time", 3.0))
        rows, columns, values = _matrix_arguments(matrix)
        if requested_path_arc is None:
            from js import noonLinearTransformationPathArc

            path_arc = float(
                engine_call(
                    noonLinearTransformationPathArc,
                    values,
                    len(rows),
                    columns,
                    operation="LinearTransformationScene.pathArc",
                )
            )
        else:
            path_arc = float(requested_path_arc)
        source_parts = []
        target_parts = []
        if self.foreground_plane is not None:
            plane_animation = ApplyMatrix(
                rows,
                self.foreground_plane,
                about_point=about,
                path_arc=path_arc,
                run_time=run_time,
            )
            target_plane = self.foreground_plane._copy_for_animate_target()
            from _manim_scene import _apply_matrix_target

            _apply_matrix_target(target_plane, plane_animation)
            source_parts.append(self.foreground_plane)
            target_parts.append(target_plane)
        for mobject in self.transformable_mobjects:
            if mobject is self.foreground_plane:
                continue
            animation = ApplyMatrix(
                rows,
                mobject,
                about_point=about,
                path_arc=path_arc,
                run_time=run_time,
            )
            target = mobject._copy_for_animate_target()
            from _manim_scene import _apply_matrix_target

            _apply_matrix_target(target, animation)
            source_parts.append(mobject)
            target_parts.append(target)
        source_vectors = []
        target_vectors = []
        for vector in self.moving_vectors:
            aggregate = getattr(vector, "_semantic_arrow_handle", None)
            transform = getattr(aggregate, "matrixTransformedEndpoints", None)
            if transform is None:
                raise RuntimeError("moving vector requires the shared Arrow matrix query")
            endpoints = engine_call(
                transform,
                values,
                len(rows),
                columns,
                float(about.x),
                float(about.y),
                operation="ApplyMatrix.vectorEndpoints",
            )
            start = (float(endpoints[0]), float(endpoints[1]))
            end = (float(endpoints[2]), float(endpoints[3]))
            target = Arrow(
                start,
                end,
                buff=0.0,
                color=vector.get_color(),
                stroke_width=float(vector.get_stroke_width()),
            )
            source_vectors.append(vector)
            target_vectors.append(target)
        if source_vectors:
            source_vector_group = _compat.Group(*source_vectors)
            source_parts.append(source_vector_group)
            target_parts.append(_compat.Group(*target_vectors))
        else:
            source_vector_group = None
        if not source_parts:
            return None
        source = _compat.Group(*source_parts)
        target = _compat.Group(*target_parts)
        # Pinned VectorSpaceScene only leaves ghosts for pieces routed through
        # get_piece_movement (vectors, moving mobjects, and labels). Generic
        # ApplyPointwiseFunction transformables and the plane do not ghost.
        ghost = (
            source_vector_group.copy().fade(0.7)
            if self.leave_ghost_vectors and source_vector_group is not None
            else None
        )
        kwargs.setdefault("run_time", 3.0)
        if ghost is not None:
            from _manim_composition import Add, AnimationGroup

            ghost_additions = [
                Add(leaf, run_time=0.0)
                for leaf in _compat._leaf_mobjects(ghost)
            ]
            transform = Transform(source, target, path_arc=path_arc, run_time=run_time)
            return self.play(AnimationGroup(*ghost_additions, transform), **kwargs)
        return self.play(
            Transform(source, target, path_arc=path_arc, run_time=run_time),
            **kwargs,
        )


__all__ = ["LinearTransformationScene", "VectorScene"]


def _merge_options(defaults: dict[str, Any], overrides: dict[str, Any]) -> dict[str, Any]:
    result = dict(defaults)
    for key, value in overrides.items():
        if isinstance(value, dict) and isinstance(result.get(key), dict):
            result[key] = _merge_options(result[key], value)
        else:
            result[key] = value
    return result
