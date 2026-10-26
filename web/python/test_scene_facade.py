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
                return SimpleNamespace(runTime=duration if duration is not None else 1.0,
                    rateFunc=rate, lagRatio=child_lag if child_lag is not None else lag,
                    pathArc=arc if arc is not None else 0.0, reverseRateFunction=reverse == 1)
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

    def test_click_glow_pulse_and_clear_use_canonical_rust_methods(self):
        source = textwrap.dedent("""
            import math
            import sys
            from types import ModuleType, SimpleNamespace
            bridge = ModuleType("js")
            resolutions = []
            def resolve(lag, duration, rate, child_lag, arc, reverse, *play):
                resolutions.append((duration, rate, reverse))
                # This double observes wrapper arguments; Rust tests own validation.
                if duration is not None and (not math.isfinite(duration) or duration <= 0.0):
                    raise ValueError("shared duration rejection")
                return SimpleNamespace(runTime=duration if duration is not None else 1.0,
                    rateFunc=rate, lagRatio=child_lag if child_lag is not None else lag,
                    pathArc=arc if arc is not None else 0.0, reverseRateFunction=reverse == 1)
            bridge.noonResolveAnimationOptions = resolve
            bridge.noonResolveGlowPulseAnimationOptions = resolve
            sys.modules["js"] = bridge
            from noon import Scene, Circle, GlowPulse, linear
            from _typed_geometry_test_support import identity_only_wrapper
            scene = Scene()
            calls = []
            scene._canonical_authoring_context = SimpleNamespace(
                liveBindClickGlowPulse=lambda *args: calls.append(("pulse", args)),
                liveClearClickAction=lambda *args: calls.append(("clear", args)))
            circle = identity_only_wrapper(Circle)
            circle._scene = scene
            circle._semantic_handle = object()
            pulse = GlowPulse(circle, intensity=2.0)
            assert not calls and not resolutions
            assert scene.on_click(circle, pulse) is scene
            assert calls == [("pulse", (circle._semantic_handle, 2.0, 1.0,
                "there_and_back", 0.0, False))]
            assert scene.on_click(circle, GlowPulse(circle, intensity=3.0, run_time=0.4,
                rate_func=linear, reverse_rate_function=True)) is scene
            assert calls[-1] == ("pulse", (circle._semantic_handle, 3.0, 0.4,
                "linear", 0.0, True))
            assert scene.on_click(circle, None) is scene
            assert calls[-1] == ("clear", (circle._semantic_handle,))

            other = identity_only_wrapper(Circle)
            other._scene = scene
            other._semantic_handle = object()
            for target, animation, exception in [
                (circle, GlowPulse(other, intensity=2.0), ValueError),
                (circle, GlowPulse(circle, intensity=2.0, path_arc=0.3), NotImplementedError),
                (circle, GlowPulse(circle, intensity=2.0, remover=True), NotImplementedError),
                (circle, GlowPulse(circle, intensity=2.0, introducer=True), NotImplementedError),
                (object(), None, TypeError),
            ]:
                try:
                    scene.on_click(target, animation)
                except exception:
                    pass
                else:
                    raise AssertionError("unsupported wrapper shape reached the binding")
            for duration in [float("nan"), float("inf"), 0.0, -1.0]:
                try:
                    scene.on_click(circle, GlowPulse(circle, intensity=2.0, run_time=duration))
                except ValueError as error:
                    assert str(error) == "shared duration rejection"
                else:
                    raise AssertionError("resolver rejection must propagate")
                observed = resolutions[-1][0]
                assert math.isnan(observed) if math.isnan(duration) else observed == duration
            circle._scene = Scene()
            for animation in [None, pulse]:
                try:
                    scene.on_click(circle, animation)
                except ValueError:
                    pass
                else:
                    raise AssertionError("foreign target must fail before replacement or clear")
            assert len(calls) == 3
        """)
        python_dir = Path(__file__).resolve().parent
        result = subprocess.run(
            [sys.executable, "-c", source], cwd=python_dir,
            env={**os.environ, "PYTHONPATH": str(python_dir)},
            capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_drag_targets_declare_bound_handles_without_python_pointer_policy(self):
        source = textwrap.dedent("""
            import sys
            from types import ModuleType, SimpleNamespace
            bridge = ModuleType("js")
            bridge.noonResolveAnimationOptions = lambda *args: None
            sys.modules["js"] = bridge
            from noon import Scene, Circle
            from _typed_geometry_test_support import identity_only_wrapper

            scene = Scene()
            calls = []
            class Batch:
                def __init__(self): self.members = []
                def appendMobject(self, wrapper_id, handle):
                    assert wrapper_id == ""
                    self.members.append(handle)
            def begin(kind):
                assert kind == "add"
                return Batch()
            scene._canonical_authoring_context = SimpleNamespace(
                beginMembershipBatch=begin,
                setTranslationDragTargets=lambda batch: calls.append(list(batch.members)))
            circle = identity_only_wrapper(Circle)
            circle._scene = scene
            circle._semantic_handle = object()
            circle._semantic_handle_fresh = True

            assert scene.set_drag_targets(circle) is scene
            assert calls == [[circle._semantic_handle]]
            for values, error in [
                ((circle, circle), ValueError),
                ((object(),), TypeError),
            ]:
                try:
                    scene.set_drag_targets(*values)
                except error:
                    pass
                else:
                    raise AssertionError("invalid drag declaration reached the Rust context")
            circle._semantic_handle_fresh = False
            try:
                scene.set_drag_targets(circle)
            except RuntimeError as error:
                assert "current semantic handle" in str(error)
            else:
                raise AssertionError("stale drag target was accepted")
            assert len(calls) == 1
        """)
        python_dir = Path(__file__).resolve().parent
        result = subprocess.run(
            [sys.executable, "-c", source], cwd=python_dir,
            env={**os.environ, "PYTHONPATH": str(python_dir)},
            capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
