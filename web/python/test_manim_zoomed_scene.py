import unittest
import sys
from types import ModuleType
from types import SimpleNamespace
from unittest.mock import patch

bridge = ModuleType("js")
bridge.noonResolveAnimationOptions = lambda *args: None
bridge.__getattr__ = lambda name: object()
sys.modules.setdefault("js", bridge)

import noon
import _manim_camera
import _manim_scene
import _manim_semantic_handles
import _manim_zoomed_scene as zoomed


class _View:
    def __init__(self):
        self.activated = False

    def zoomFactor(self):
        return 0.375


class ZoomedSceneFacadeTests(unittest.TestCase):
    def test_setup_forwards_typed_options_and_wraps_rust_handles(self) -> None:
        captured = {}
        view = _View()

        def bind_camera(scene, frame):
            _manim_semantic_handles._attach_shared_handle(frame, object())
            frame._scene = scene
            frame._object = SimpleNamespace(id=0)

        def bind_zoom(scene, frame, display, **options):
            captured.update(options)
            _manim_semantic_handles._attach_shared_handle(frame, object())
            _manim_semantic_handles._attach_shared_handle(display, object())
            frame._scene = display._scene = scene
            frame._object = SimpleNamespace(id=1)
            display._object = SimpleNamespace(id=2)
            return view

        def activate(scene, actual, *wrappers):
            self.assertIs(actual, view)
            self.assertEqual(wrappers, (scene.zoomed_camera.frame, scene.zoomed_display))
            actual.activated = True

        with (
            patch.object(_manim_scene, "_bind_camera_frame", bind_camera),
            patch.object(_manim_scene, "_bind_zoomed_view", bind_zoom),
            patch.object(_manim_scene, "_activate_zoomed_view", activate),
        ):
            scene = zoomed.ZoomedScene(
                zoomed_display_height=2.0,
                zoomed_display_width=4.0,
                zoomed_display_center=noon.Vec2(1.0, -2.0),
                zoomed_camera_frame_starting_position=noon.Vec2(-0.5, 0.25),
                zoom_factor=0.2,
                zoomed_camera_config={"default_frame_stroke_width": 4},
                image_frame_stroke_width=5,
            )
            scene.setup()
            self.assertEqual(captured["display_height"], 2.0)
            self.assertEqual(captured["display_width"], 4.0)
            self.assertEqual(captured["display_center"], noon.Vec2(1.0, -2.0))
            self.assertEqual(captured["camera_frame_start"], noon.Vec2(-0.5, 0.25))
            self.assertEqual(captured["zoom_factor"], 0.2)
            self.assertEqual(captured["camera_frame_stroke_width"], 4.0)
            self.assertEqual(captured["image_frame_stroke_width"], 5.0)
            self.assertIs(scene.zoomed_display.display_frame, scene.zoomed_display)
            self.assertEqual(scene.get_zoom_factor(), 0.375)
            scene.activate_zooming()
            self.assertTrue(view.activated)
            self.assertTrue(scene.zoom_activated)

    def test_animated_activation_rejects_before_publication(self) -> None:
        scene = object.__new__(zoomed.ZoomedScene)
        scene._zoomed_view_handle = _View()
        scene.zoom_activated = False
        with self.assertRaisesRegex(NotImplementedError, "animated"):
            scene.activate_zooming(animate=True)
        self.assertFalse(scene.zoom_activated)


if __name__ == "__main__":
    unittest.main()
