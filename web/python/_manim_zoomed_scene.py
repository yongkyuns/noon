"""Thin Manim ZoomedScene facade over Noon's shared retained inset relation."""

from __future__ import annotations

from typing import Any

import noon as _base
import _manim_camera as _camera
import _manim_compat as _compat
import _manim_semantic_handles as _semantic


class _ZoomedCameraFrame(_compat.Rectangle):
    def __init__(self, width: float, height: float) -> None:
        _semantic._initialize_shared_wrapper(self)
        self.width_value = float(width)
        self.height_value = float(height)

    def move_to(self, point: object) -> _ZoomedCameraFrame:
        if isinstance(point, (_base.Mobject, _compat.Group)):
            point = point.get_center()
        super().move_to(point)
        return self


class _ZoomedCamera:
    def __init__(self, frame: _ZoomedCameraFrame) -> None:
        self.frame = frame


class _ZoomedDisplay(_compat.Rectangle):
    def __init__(self, width: float, height: float, camera: _ZoomedCamera) -> None:
        _semantic._initialize_shared_wrapper(self)
        self.width_value = float(width)
        self.height_value = float(height)
        self.camera = camera
        # Noon represents the display image and its border with one ordinary
        # semantic rectangle; the renderer inserts the retained inset inside it.
        self.display_frame = self


class ZoomedScene(_camera.MovingCameraScene):
    """Manim-compatible retained inset backed by one Rust semantic relation."""

    def __init__(
        self,
        camera_class: type | None = None,
        zoomed_display_height: float = 3,
        zoomed_display_width: float = 3,
        zoomed_display_center: object | None = None,
        zoomed_display_corner: object = _base.UR,
        zoomed_display_corner_buff: float = _base.DEFAULT_MOBJECT_TO_EDGE_BUFFER,
        zoomed_camera_config: dict[str, Any] | None = None,
        zoomed_camera_image_mobject_config: dict[str, Any] | None = None,
        zoomed_camera_frame_starting_position: object = _base.ORIGIN,
        zoom_factor: float = 0.15,
        image_frame_stroke_width: float = 3,
        zoom_activated: bool = False,
        **kwargs: Any,
    ) -> None:
        if camera_class is not None:
            raise NotImplementedError("ZoomedScene camera_class overrides are not supported")
        camera_options = dict(zoomed_camera_config or {})
        frame_stroke_width = float(camera_options.pop("default_frame_stroke_width", 2))
        background_opacity = float(camera_options.pop("background_opacity", 1))
        if camera_options:
            raise NotImplementedError(
                f"unsupported zoomed_camera_config option(s): {sorted(camera_options)}"
            )
        if background_opacity != 1.0:
            raise NotImplementedError("zoomed camera background_opacity must be 1")
        image_options = dict(zoomed_camera_image_mobject_config or {})
        capture_own_display = bool(
            image_options.pop("allow_cameras_to_capture_their_own_display", False)
        )
        if image_options:
            raise NotImplementedError(
                "unsupported zoomed_camera_image_mobject_config option(s): "
                f"{sorted(image_options)}"
            )
        if zoom_activated:
            raise NotImplementedError(
                "construct the ZoomedScene first, then call activate_zooming()"
            )

        self.zoomed_display_height = float(zoomed_display_height)
        self.zoomed_display_width = float(zoomed_display_width)
        self.zoomed_display_center = (
            None
            if zoomed_display_center is None
            else _base._as_vec2(zoomed_display_center)
        )
        self.zoomed_display_corner = _base._as_vec2(zoomed_display_corner)
        self.zoomed_display_corner_buff = float(zoomed_display_corner_buff)
        self.zoomed_camera_config = {
            "default_frame_stroke_width": frame_stroke_width,
            "background_opacity": background_opacity,
        }
        self.zoomed_camera_image_mobject_config = {}
        self.zoomed_camera_frame_starting_position = _base._as_vec2(
            zoomed_camera_frame_starting_position
        )
        self.zoom_factor = float(zoom_factor)
        self.image_frame_stroke_width = float(image_frame_stroke_width)
        self.zoom_activated = False
        self._capture_own_display = capture_own_display
        self._zoomed_view_handle = None
        super().__init__(**kwargs)

    def setup(self) -> None:
        import _manim_scene as _scene
        super().setup()
        frame = _ZoomedCameraFrame(
            self.zoomed_display_width, self.zoomed_display_height
        )
        zoomed_camera = _ZoomedCamera(frame)
        display = _ZoomedDisplay(
            self.zoomed_display_width, self.zoomed_display_height, zoomed_camera
        )
        self._zoomed_view_handle = _scene._bind_zoomed_view(
            self,
            frame,
            display,
            display_height=self.zoomed_display_height,
            display_width=self.zoomed_display_width,
            display_center=self.zoomed_display_center,
            display_corner=self.zoomed_display_corner,
            display_corner_buff=self.zoomed_display_corner_buff,
            camera_frame_start=self.zoomed_camera_frame_starting_position,
            zoom_factor=self.zoom_factor,
            camera_frame_stroke_width=self.zoomed_camera_config[
                "default_frame_stroke_width"
            ],
            image_frame_stroke_width=self.image_frame_stroke_width,
            capture_own_display=self._capture_own_display,
        )
        self.zoomed_camera = zoomed_camera
        self.zoomed_display = display

    def activate_zooming(self, animate: bool = False) -> None:
        import _manim_scene as _scene
        if animate:
            raise NotImplementedError("animated ZoomedScene activation is not supported")
        if self._zoomed_view_handle is None:
            raise RuntimeError("ZoomedScene.setup() must run before activation")
        _scene._activate_zoomed_view(
            self,
            self._zoomed_view_handle,
            self.zoomed_camera.frame,
            self.zoomed_display,
        )
        self.zoom_activated = True

    def get_zoom_factor(self) -> float:
        if self._zoomed_view_handle is None:
            raise RuntimeError("ZoomedScene.setup() must run before querying zoom")
        import _manim_scene as _scene
        return _scene._zoomed_view_factor(self, self._zoomed_view_handle)

    def get_zoomed_display_pop_out_animation(self, **kwargs: Any) -> object:
        """Stretch the display to the camera frame, then Transform it back.

        This mirrors Manim v0.21's saved-state/``replace(..., stretch=True)``
        sequence while leaving interpolation and publication to shared Transform.
        """
        if self._zoomed_view_handle is None:
            raise RuntimeError("ZoomedScene.setup() must run before pop-out animation")
        display = self.zoomed_display
        display.save_state()
        display.replace(self.zoomed_camera.frame, stretch=True)
        return _compat.Restore(display, **kwargs)



__all__ = ["ZoomedScene"]
