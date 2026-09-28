"""Public Scene methods bind to the shared host without a final installer."""
import os
from pathlib import Path
import subprocess
import sys
import textwrap
import unittest


class SceneFacadeTests(unittest.TestCase):
    def test_cursor_and_portable_barriers_use_stable_public_methods(self):
        source = textwrap.dedent("""
            import asyncio
            import sys
            from types import ModuleType

            bridge = ModuleType("js")
            bridge.noonResolveAnimationOptions = lambda *args: None
            sys.modules["js"] = bridge
            import noon
            original_play, original_wait = noon.Scene.play, noon.Scene.wait
            import _manim_compat as compat

            import _manim_scene as canonical
            from _manim_source_execution import has_portable_scene_methods
            assert noon.Scene.play is original_play
            assert noon.Scene.wait is original_wait
            assert not hasattr(canonical, "install")

            class Context:
                def __init__(self):
                    self.time = 0.0
                    self.waits = []
                def authoredDuration(self): return self.time
                def ordinaryWait(self, duration):
                    self.waits.append(duration)
                    self.time += duration
                def authoredWait(self, duration): self.time += duration

            scene = noon.Scene()
            context = Context()
            scene._canonical_authoring_context = context
            assert scene.time == 0.0
            assert scene.wait(0.5) is scene
            assert scene.declare_wait(0.25) is scene
            assert scene.time == 0.75
            assert has_portable_scene_methods(scene, play=original_play, wait=original_wait)
            setattr(scene, canonical._PORTABLE_CONSTRUCT_MODE, True)
            assert asyncio.run(canonical.await_source_barrier(scene.wait, 0.5)) is scene
            assert scene.time == 1.25
            assert context.waits == [0.5, 0.5]
            assert not getattr(scene, canonical._PORTABLE_BARRIER_CALL)

            class Override(noon.Scene):
                def wait(self, duration=1.0): return "custom"
            overridden = Override()
            assert not has_portable_scene_methods(overridden, play=original_play, wait=original_wait)
            setattr(overridden, canonical._PORTABLE_CONSTRUCT_MODE, True)
            try:
                asyncio.run(canonical.await_source_barrier(overridden.wait, 1.0))
            except RuntimeError as error:
                assert "canonical play/wait" in str(error)
            else:
                raise AssertionError("custom wait must retain its original execution path")
        """)
        python_dir = Path(__file__).resolve().parent
        result = subprocess.run(
            [sys.executable, "-c", source], cwd=python_dir,
            env={**os.environ, "PYTHONPATH": str(python_dir)},
            capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_click_indicate_declares_typed_action_without_callbacks(self):
        source = textwrap.dedent("""
            import sys
            from types import ModuleType, SimpleNamespace
            bridge = ModuleType("js")
            def resolve(lag, duration, rate, child_lag, arc, reverse, *play):
                import math
                return SimpleNamespace(runTime=duration if math.isfinite(duration) else 1.0,
                    rateFunc=rate, lagRatio=child_lag if math.isfinite(child_lag) else lag,
                    pathArc=arc if math.isfinite(arc) else 0.0, reverseRateFunction=reverse == 1)
            bridge.noonResolveAnimationOptions = resolve
            sys.modules["js"] = bridge
            from noon import Scene, Circle, Indicate, linear
            from _typed_geometry_test_support import identity_only_wrapper
            scene = Scene()
            calls = []
            scene._canonical_authoring_context = SimpleNamespace(
                liveBindClickIndicate=lambda *args: calls.append(args))
            circle = identity_only_wrapper(Circle)
            circle._scene = scene
            circle._semantic_handle = object()
            assert scene.on_click(circle, Indicate(circle, run_time=0.4)) is scene
            assert len(calls) == 1
            assert calls[0][0] is circle._semantic_handle
            assert calls[0][1] == 1.2
            assert calls[0][-3:] == (0.4, "there_and_back", 0.0)
            other = identity_only_wrapper(Circle)
            other._scene = scene
            other._semantic_handle = object()
            for target, animation, exception in [
                (circle, lambda: None, NotImplementedError),
                (circle, Indicate(other), ValueError),
                (circle, Indicate(circle, rate_func=linear), NotImplementedError),
                (circle, Indicate(circle, path_arc=0.3), NotImplementedError),
            ]:
                try:
                    scene.on_click(target, animation)
                except exception:
                    pass
                else:
                    raise AssertionError("unsupported binding must fail before calling Rust")
            circle._scene = Scene()
            try:
                scene.on_click(circle, Indicate(circle))
            except ValueError:
                pass
            else:
                raise AssertionError("foreign scene target must be rejected")
            assert len(calls) == 1
        """)
        python_dir = Path(__file__).resolve().parent
        result = subprocess.run(
            [sys.executable, "-c", source], cwd=python_dir,
            env={**os.environ, "PYTHONPATH": str(python_dir)},
            capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
