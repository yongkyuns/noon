import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class OrdinaryTrackerScopeTests(unittest.TestCase):
    def test_constructor_uses_task_local_canonical_scene_scope(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        existing = env.get("PYTHONPATH")
        env["PYTHONPATH"] = (
            str(python_dir)
            if not existing
            else os.pathsep.join((str(python_dir), existing))
        )
        source = textwrap.dedent(
            r'''
            import asyncio
            import sys
            import types
            from types import SimpleNamespace

            # Canonical scene imports resolve these browser-owned planners at module
            # import time. Keep the test bridge local to this subprocess, as the
            # production modules deliberately provide no native fallback for them.
            fake_js = types.ModuleType("js")
            fake_js.noonResolveAnimationOptions = object()
            fake_js.noonResolveUniformCompositionSchedule = object()
            fake_js.noonResolveCompositionSchedule = object()
            fake_js.noonResolveLifecyclePlan = object()
            fake_js.noonValidatePresenceTransition = object()
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_rate_functions
            _manim_rate_functions.install()
            import _manim_phase_b  # noqa: F401
            import _manim_geometry  # noqa: F401
            import _manim_semantic_handles
            _manim_semantic_handles.install()
            import _manim_shared_geometry
            _manim_shared_geometry.install()
            import _manim_dashed_line
            _manim_dashed_line.install()
            import _manim_animate  # noqa: F401
            import _manim_rotate
            _manim_rotate.install()
            import _manim_composition
            _manim_composition.install()
            import _manim_lifecycle  # noqa: F401
            import _manim_typst
            _manim_typst.install()
            import _manim_retained_animate
            _manim_retained_animate.install()
            import _manim_retained_state
            _manim_retained_state.install()
            import _manim_reactive as reactive
            import _manim_canonical_scene as canonical
            from _manim_reactive import ValueTracker


            class Context:
                def createValueTracker(self, value):
                    return SimpleNamespace(owner=self, value=float(value))

                def valueTrackerValue(self, handle):
                    return handle.value


            async def verify_isolated_suspended_sources():
                ready = asyncio.Event()
                entered = []
                trackers = []

                class Scene:
                    def __init__(self):
                        self._canonical_authoring_context = Context()

                    def setup(self):
                        self.initial = ValueTracker(1)

                    async def construct(self):
                        entered.append(self)
                        if len(entered) == 2:
                            ready.set()
                        await ready.wait()
                        trackers.append((self, ValueTracker(3)))

                    def tear_down(self):
                        pass

                scenes = [Scene(), Scene()]
                await asyncio.gather(
                    *(canonical.execute_construct(scene) for scene in scenes)
                )

                assert len(trackers) == 2
                for scene, tracker in trackers:
                    assert tracker._scene is scene
                    assert tracker._canonical_context is scene._canonical_authoring_context
                    assert tracker._canonical_handle.owner is scene._canonical_authoring_context
                    assert scene.initial._canonical_handle.owner is scene._canonical_authoring_context
                    assert not hasattr(tracker, "_value")
                    assert not hasattr(tracker, "_signal_id")
                    assert tracker.get_value() == 3.0
                assert reactive._current_authoring_scene() is None


            async def verify_failed_setup_resets_scope():
                teardown_called = False

                class Scene:
                    _canonical_authoring_context = Context()

                    def setup(self):
                        assert reactive._current_authoring_scene() is self
                        raise RuntimeError("setup failed")

                    def construct(self):
                        raise AssertionError("construct called after failed setup")

                    def tear_down(self):
                        nonlocal teardown_called
                        teardown_called = True

                try:
                    await canonical.execute_construct(Scene())
                except RuntimeError as error:
                    assert str(error) == "setup failed"
                else:
                    raise AssertionError("failed setup unexpectedly completed")
                assert not teardown_called
                assert reactive._current_authoring_scene() is None

                detached = ValueTracker(9)
                assert detached._value == 9.0
                assert detached._scene is None
                assert detached._signal_id is None
                assert not hasattr(detached, "_canonical_handle")


            async def main():
                await verify_isolated_suspended_sources()
                await verify_failed_setup_resets_scope()


            asyncio.run(main())
            '''
        )
        completed = subprocess.run(
            [sys.executable, "-c", source],
            cwd=python_dir,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(
            completed.returncode,
            0,
            "ordinary tracker scope subprocess failed:\n"
            f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
