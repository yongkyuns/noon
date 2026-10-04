"""Thin VectorScene and LinearTransformationScene composition helpers.

These classes compose the existing shared NumberPlane, Arrow, Group and
ordinary Transform/ApplyMatrix operations. Python retains wrapper references
only; Rust remains the authority for geometry and animation state.
"""

from __future__ import annotations

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
        animate: bool = False,
        **kwargs: Any,
    ):
        from _manim_arrow import Arrow, Vector

        if animate:
            raise NotImplementedError("VectorScene animated add_vector is not included")
        if kwargs:
            unknown = ", ".join(sorted(kwargs))
            raise TypeError(f"unsupported VectorScene.add_vector option(s): {unknown}")
        result = vector if isinstance(vector, Arrow) else Vector(
            vector,
            color=color if color is not None else _base.color_from_hex("#FFFF00"),
        )
        self.add(result)
        return result


class LinearTransformationScene(VectorScene):
    """LTS defaults composed from retained planes and regenerable Arrow targets.

    Coordinate labels and ghost histories are explicit unsupported options in
    this initial thin facade. Callers can add ordinary Text labels themselves.
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
        if show_coordinates:
            raise NotImplementedError(
                "LinearTransformationScene coordinate labels are not included; add ordinary Text labels"
            )
        if leave_ghost_vectors:
            raise NotImplementedError(
                "LinearTransformationScene ghost-vector history is not included"
            )
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
        self.moving_vectors.append(result)
        return result

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
            source_parts.append(_compat.Group(*source_vectors))
            target_parts.append(_compat.Group(*target_vectors))
        if not source_parts:
            return None
        source = _compat.Group(*source_parts)
        target = _compat.Group(*target_parts)
        kwargs.setdefault("run_time", 3.0)
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
