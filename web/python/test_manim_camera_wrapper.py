import os
from pathlib import Path
import subprocess
import sys
import textwrap
import unittest


class MovingCameraWrapperTests(unittest.TestCase):
    def test_fresh_camera_frame_initializes_opaque_wrapper_before_binding(self) -> None:
        source = textwrap.dedent(
            """
            import sys
            from types import ModuleType, SimpleNamespace

            bridge = ModuleType("js")
            bridge.noonResolveAnimationOptions = object()
            sys.modules["js"] = bridge
            for name in ("_manim_family_creation",):
                module = ModuleType(name)
                module.install = lambda: None
                sys.modules[name] = module

            import _manim_compat as compat

            import _manim_camera as camera

            handle = object()

            class Scene:
                def _bind_camera_frame(self, frame):
                    assert frame._raw is None
                    assert frame._scene is None
                    assert frame._object is None
                    assert frame._semantic_handle is None
                    assert frame._semantic_handle_fresh is False
                    camera._semantic_handles._attach_shared_handle(frame, handle)
                    frame._scene = self
                    frame._object = SimpleNamespace(id=7)

            scene = Scene()
            frame = camera._CameraFrame(scene)
            assert frame._scene is scene
            assert frame._object.id == 7
            assert frame._semantic_handle is handle
            assert frame._semantic_handle_fresh is True
            assert frame.width_value == camera._base.DEFAULT_FRAME_WIDTH
            assert frame.height_value == camera._base.DEFAULT_FRAME_HEIGHT
            """
        )
        python_dir = Path(__file__).resolve().parent
        result = subprocess.run(
            [sys.executable, "-c", source],
            cwd=python_dir,
            env={**os.environ, "PYTHONPATH": str(python_dir)},
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_camera_and_zoomed_display_saved_states_keep_live_owner_and_aliases(self) -> None:
        source = textwrap.dedent(
            r"""
            import copy
            import sys
            import types
            from types import SimpleNamespace

            bridge = types.ModuleType("js")
            bridge.noonResolveAnimationOptions = object()

            class Handle:
                def __init__(self, owner, state):
                    self.owner = owner
                    self.state = copy.deepcopy(state)

                def cloneHandle(self):
                    self.owner.semantic_revision += 1
                    return Handle(self.owner, self.state)

                def targetEditor(self):
                    return Handle(self.owner, self.state)

                def scale(self, x, y):
                    self.state["scale"] = [self.state["scale"][0] * x,
                                           self.state["scale"][1] * y]

            class Context:
                def __init__(self):
                    self.semantic_revision = 0
                    self.execution_revision = 0
                    self.ownership = "none"
                    self.calls = []

                def liveExecutionOwnership(self):
                    return self.ownership

                def liveTargetEditor(self, handle):
                    self._require_coherent()
                    self.semantic_revision += 1
                    self.execution_revision = self.semantic_revision
                    self.calls.append("liveTargetEditor")
                    return handle.targetEditor()

                def liveScale(self, handle, x, y):
                    self._require_coherent()
                    handle.scale(x, y)
                    self.semantic_revision += 1
                    self.execution_revision = self.semantic_revision
                    self.calls.append("liveScale")

                def liveMoveToPoint(self, handle, x, y, *args):
                    self._require_coherent()
                    handle.state["center"] = [x, y]
                    self.semantic_revision += 1
                    self.execution_revision = self.semantic_revision
                    self.calls.append("liveMoveToPoint")

                def liveBecomeMobject(self, handle, target, *flags):
                    self._require_coherent()
                    handle.state = copy.deepcopy(target.state)
                    self.semantic_revision += 1
                    self.execution_revision = self.semantic_revision
                    self.calls.append("liveBecomeMobject")

                def _require_coherent(self):
                    if self.semantic_revision != self.execution_revision:
                        raise RuntimeError(
                            f"semantic revision {self.semantic_revision} has not been "
                            f"published into execution revision context {self.execution_revision}"
                        )

            context = Context()
            import _manim_semantic_handles as handles
            import _typed_geometry_test_support as geometry_test
            geometry_test.install_js_bridge(bridge, lambda _: None)
            geometry_test.install_module_bridge(handles, lambda _: None)
            sys.modules["js"] = bridge

            import _manim_camera as camera
            import _manim_zoomed_scene as zoomed

            class Scene:
                _canonical_authoring_context = context

                def _bind_camera_frame(self, frame):
                    handles._attach_shared_handle(
                        frame, Handle(context, {"scale": [1.0, 1.0], "center": [0.0, 0.0]})
                    )
                    frame._scene = self
                    frame._object = SimpleNamespace(id=1)

            scene = Scene()
            frame = camera._CameraFrame(scene)
            display = zoomed._ZoomedDisplay(4.0, 3.0, zoomed._ZoomedCamera(frame))
            handles._attach_shared_handle(
                display, Handle(context, {"scale": [1.0, 1.0], "center": [0.0, 0.0]})
            )
            display._scene = scene
            display._object = SimpleNamespace(id=2)
            frame.save_state()
            display.save_state()
            # The initial save predates live execution. The worker lowers this
            # authored revision before returning the same player to the source.
            context.execution_revision = context.semantic_revision
            context.ownership = "returned"

            frame.animate.scale(0.62).move_to((1.0, 2.0))
            builder = display.animate
            builder.scale(0.5).move_to((2.0, 1.0))
            target = builder.target
            assert target.display_frame is target
            assert target.camera.frame is not frame
            assert target.camera.frame._canonical_live_target_context is context
            assert context.semantic_revision == context.execution_revision
            assert context.calls.count("liveTargetEditor") >= 5, context.calls
            assert context.calls[-2:] == ["liveScale", "liveMoveToPoint"], context.calls

            context.liveScale(frame._semantic_handle, 2.0, 2.0)
            frame.restore()
            assert frame._semantic_handle.state["scale"] == [1.0, 1.0]
            assert context.calls[-1] == "liveBecomeMobject"
            context.liveScale(display._semantic_handle, 2.0, 2.0)
            display.restore()
            assert display._semantic_handle.state["scale"] == [1.0, 1.0]
            assert context.calls[-1] == "liveBecomeMobject"
            assert context.semantic_revision == context.execution_revision
            """
        )
        python_dir = Path(__file__).resolve().parent
        result = subprocess.run(
            [sys.executable, "-c", source],
            cwd=python_dir,
            env={**os.environ, "PYTHONPATH": str(python_dir)},
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
