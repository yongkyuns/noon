"""Thin Manim moving-camera authoring adapter over Noon's shared Rust camera semantics."""

from __future__ import annotations

from typing import Any

import noon as _base
import _manim_compat as _compat
import _manim_family_creation as _family_creation
import _manim_retained_family_fade_batch as _retained_family_fade_batch


class _CameraFrame(_compat.Rectangle):
    """Invisible semantic frame object consumed by the Rust execution pipeline."""

    def __init__(self, scene: _compat.Scene) -> None:
        self.width_value = float(_base.DEFAULT_FRAME_WIDTH)
        self.height_value = float(_base.DEFAULT_FRAME_HEIGHT)
        scene._bind_camera_frame(self)

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

    def to_document(self) -> dict[str, Any]:
        document = super().to_document()
        document["camera_object"] = self.camera.frame.id
        return document


def install() -> None:
    """Install final authoring wrappers and expose the moving-camera name."""

    # Camera is the final compatibility module installed by the browser bootstrap.
    # Install semantic-family creation first, then the retained family-fade batch
    # coordinator above that transaction so it can reuse the existing leaf scheduler.
    _family_creation.install()
    _retained_family_fade_batch.install()
    _base.MovingCameraScene = MovingCameraScene
    if "MovingCameraScene" not in _base.__all__:
        _base.__all__.append("MovingCameraScene")
