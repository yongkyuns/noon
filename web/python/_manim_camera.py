"""Thin Manim moving-camera authoring adapter over Noon's shared Rust camera semantics."""

from __future__ import annotations

from typing import Any

import noon as _base
import _manim_compat as _compat
import _manim_semantic_handles as _semantic_handles


class _CameraFrame(_compat.Rectangle):
    """Invisible semantic frame object consumed by the Rust execution pipeline."""

    def __init__(self, scene: _compat.Scene) -> None:
        _semantic_handles._initialize_shared_wrapper(self)
        self.width_value = float(_base.DEFAULT_FRAME_WIDTH)
        self.height_value = float(_base.DEFAULT_FRAME_HEIGHT)
        scene._bind_camera_frame(self)

    def set_view_color_profile(self, profile: str) -> _CameraFrame:
        """Select compatibility output or explicit linear HDR on this camera."""
        from _manim_updaters import canonical_callback_phase_active
        from _noon_errors import engine_call
        if canonical_callback_phase_active():
            raise NotImplementedError("callback view profile edits require staged publication")
        if not isinstance(profile, str):
            raise TypeError("view color profile must be a string")
        handle = _semantic_handles._handle_for(self)
        context = _semantic_handles._live_mutation_context(self)
        if context is None:
            engine_call(handle.setViewColorProfile, profile)
        else:
            engine_call(context.setViewColorProfile, handle, profile)
        return self

    def move_to(self, point: object) -> _CameraFrame:
        if isinstance(point, (_base.Mobject, _compat.Group)):
            point = point.get_center()
        super().move_to(point)
        return self


class _MovingCamera:
    def __init__(self, frame: _CameraFrame) -> None:
        self.frame = frame


class MovingCameraScene(_compat.Scene):
    """Manim facade that only authors a shared semantic camera-frame object."""

    def __init__(self, **kwargs: Any) -> None:
        if kwargs:
            unsupported = ", ".join(sorted(kwargs))
            raise NotImplementedError(
                f"unsupported MovingCameraScene option(s): {unsupported}"
            )
        super().__init__()
        frame = _CameraFrame(self)
        self.camera = _MovingCamera(frame)
