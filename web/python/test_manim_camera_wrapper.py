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
            for name in (
                "_manim_family_creation",
                "_manim_retained_family_fade_batch",
            ):
                module = ModuleType(name)
                module.install = lambda: None
                sys.modules[name] = module

            import _manim_compat as compat
            compat.install()
            import _manim_phase_b

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


if __name__ == "__main__":
    unittest.main()
