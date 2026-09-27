import unittest
import sys
from types import SimpleNamespace
from unittest.mock import patch

import noon
_previous_js = sys.modules.get("js")
sys.modules["js"] = SimpleNamespace(noonResolveAnimationOptions=lambda *args: None)
try:
    import _manim_camera
    import _manim_scene
    import _manim_semantic_handles
    import _manim_zoomed_scene as zoomed
finally:
    if _previous_js is None:
        sys.modules.pop("js", None)
    else:
        sys.modules["js"] = _previous_js

from _manim_source_execution import (
    bind_portable_construct,
    compile_authoring_source,
    has_portable_scene_methods,
)



class _View:
    def __init__(self):
        self.activated = False

    def zoomFactor(self):
        return 0.375


class ZoomedSceneFacadeTests(unittest.TestCase):
    def test_portable_activation_admits_only_the_canonical_zoomed_method(self) -> None:
        source = '''
class Example(ZoomedScene):
    def construct(self):
        self.activate_zooming()
        self.wait(1)
'''
        code, pairs = compile_authoring_source(source)
        namespace = {"ZoomedScene": zoomed.ZoomedScene}
        exec(code, namespace)
        # Admission inspects methods without setup or semantic allocation.
        scene = object.__new__(namespace["Example"])
        construct = bind_portable_construct(scene.construct, pairs)
        self.assertIsNotNone(construct)
        methods = _manim_scene._portable_scene_methods(scene)
        self.assertIsNotNone(methods)
        self.assertTrue(has_portable_scene_methods(scene, **methods))

        class ClassOverride(zoomed.ZoomedScene):
            def activate_zooming(self, animate=False):
                return None

        for candidate in (object.__new__(ClassOverride), scene):
            if candidate is scene:
                candidate.activate_zooming = lambda animate=False: None
            methods = _manim_scene._portable_scene_methods(candidate)
            self.assertIsNotNone(methods)
            self.assertFalse(has_portable_scene_methods(candidate, **methods))

        class OrdinaryScene(noon.Scene):
            def activate_zooming(self):
                return None

        self.assertIsNone(_manim_scene._portable_scene_methods(OrdinaryScene()))

    def test_portable_activation_admission_does_not_invoke_authored_lookup(self) -> None:
        effects = []

        class DescriptorScene(noon.Scene):
            @property
            def activate_zooming(self):
                effects.append("descriptor")
                return lambda: None

        class DynamicScene(noon.Scene):
            def __getattr__(self, name):
                effects.append(name)
                if name == "activate_zooming":
                    return lambda: None
                raise AttributeError(name)

        self.assertIsNone(_manim_scene._portable_scene_methods(DescriptorScene()))
        self.assertIsNone(_manim_scene._portable_scene_methods(DynamicScene()))
        self.assertEqual(effects, [])

    def test_nested_activation_override_is_not_portable(self) -> None:
        source = '''
class Override(ZoomedScene):
    def activate_zooming(self, animate=False):
        return None

    def construct(self):
        [self.activate_zooming() for _ in range(1)]
        self.wait(1)
'''
        code, pairs = compile_authoring_source(source)
        namespace = {"ZoomedScene": zoomed.ZoomedScene}
        exec(code, namespace)
        scene = object.__new__(namespace["Override"])
        construct = bind_portable_construct(scene.construct, pairs)
        self.assertIsNotNone(construct)
        methods = _manim_scene._portable_scene_methods(scene)
        self.assertIsNotNone(methods)
        self.assertFalse(
            has_portable_scene_methods(scene, **methods)
        )

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
            patch.object(_manim_scene, "_zoomed_view_factor", lambda scene, actual: actual.zoomFactor()),
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
