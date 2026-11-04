"""Boundary routing only; shared Rust tests own pulse sampling and lifecycle."""
from pathlib import Path
import subprocess
import sys
import textwrap
import unittest


class GlowPulseRoutingTests(unittest.TestCase):
    def test_portable_play_build_cooperates_and_cancellation_does_not_activate(self):
        source = textwrap.dedent(
            """
            import asyncio
            import sys
            import types
            from types import SimpleNamespace
            from unittest.mock import patch

            bridge = types.ModuleType("js")
            resolved = SimpleNamespace(runTime=0.2, rateFunc="linear", lagRatio=0.0,
                pathArc=0.0, reverseRateFunction=False)
            bridge.noonResolveAnimationOptions = lambda *args: resolved
            bridge.noonResolveGlowPulseAnimationOptions = lambda *args: resolved
            sys.modules["js"] = bridge

            from noon import AnimationGroup, GlowPulse, Mobject, Scene, linear
            import _manim_scene as operations
            from _manim_scene import await_source_barrier
            from _manim_source_execution import cooperative_source_yield_scope

            clock = [0.0]
            class Builder:
                def __init__(self): self.rate = None; self.children = []
                def setCompositionRateFunction(self, rate): self.rate = rate
                def setPlayRateFunction(self, rate): self.rate = rate
                def appendComposition(self, child): self.children.append(child)
                def appendGlowPulseMobject(self, *args):
                    self.children.append(("pulse", args)); clock[0] += 0.002
            class Context:
                def __init__(self): self.builders = []; self.validated = False
                def beginOrdinaryCompositionBuilder(self, *args):
                    builder = Builder(); self.builders.append(builder); return builder
                def ordinaryCanPlayComposition(self, builder):
                    self.validated = True; clock[0] += 0.002
                    return True
            def target(scene, slot):
                value = object.__new__(Mobject)
                value._scene = scene
                value._semantic_handle = SimpleNamespace(slot=slot)
                return value
            def tree(builder):
                return tuple(tree(item) if isinstance(item, Builder) else item
                             for item in builder.children)

            def scene_with_context():
                scene = Scene()
                context = Context()
                scene._canonical_authoring_context = context
                return scene, context
            sync_scene, sync_context = scene_with_context()
            first = GlowPulse(target(sync_scene, 1), intensity=1)
            second = GlowPulse(target(sync_scene, 2), intensity=2)
            third = GlowPulse(target(sync_scene, 3), intensity=3)
            group = AnimationGroup(first, AnimationGroup(second, third, rate_func=linear), rate_func=linear)
            expected = operations._build_canonical_composition_candidate(
                sync_scene, "parallel", tuple(group.animations), group, {})

            async def main():
                scene, context = scene_with_context()
                first = GlowPulse(target(scene, 1), intensity=1)
                second = GlowPulse(target(scene, 2), intensity=2)
                third = GlowPulse(target(scene, 3), intensity=3)
                group = AnimationGroup(first, AnimationGroup(second, third, rate_func=linear), rate_func=linear)
                activated = []
                original = operations._play_canonical_composition
                operations._play_canonical_composition = lambda scene, *args: activated.append(args) or scene
                yields = []
                async def checkpoint():
                    assert not activated
                    yields.append(context.validated)
                    await asyncio.sleep(0)
                operations._begin_async_continuation_construct(scene)
                scene._noon_portable_construct_mode = True
                try:
                    with patch("_manim_source_execution.perf_counter", side_effect=lambda: clock[0]):
                        with cooperative_source_yield_scope(checkpoint):
                            clock[0] = 0.01
                            await await_source_barrier(scene.play, group)
                finally:
                    scene._noon_portable_construct_mode = False
                    operations._finish_async_continuation_construct(scene)
                assert len(yields) >= 3, yields
                assert yields[-1] is True
                assert len(activated) == 1
                assert tree(expected[0]) == tree(activated[0][0])

                scene, context = scene_with_context()
                first = GlowPulse(target(scene, 1), intensity=1)
                later = GlowPulse(target(scene, 2), intensity=2)
                group = AnimationGroup(first, later, rate_func=linear)
                activated.clear()
                entered = asyncio.Event()
                async def blocked_checkpoint():
                    assert not scene._noon_portable_barrier_call
                    entered.set()
                    await asyncio.Event().wait()
                operations._begin_async_continuation_construct(scene)
                scene._noon_portable_construct_mode = True
                async def cancelled_play():
                    try:
                        await await_source_barrier(scene.play, group)
                    finally:
                        scene._noon_portable_construct_mode = False
                        operations._finish_async_continuation_construct(scene)
                try:
                    with patch("_manim_source_execution.perf_counter", side_effect=lambda: clock[0]):
                        with cooperative_source_yield_scope(blocked_checkpoint):
                            clock[0] = 1.0
                            task = asyncio.create_task(cancelled_play())
                            try:
                                await asyncio.wait_for(entered.wait(), 1.0)
                            except BaseException:
                                task.cancel()
                                try: await task
                                except BaseException: pass
                                raise
                            task.cancel()
                            try: await task
                            except asyncio.CancelledError: pass
                            else: raise AssertionError("cooperative build was not cancellable")
                finally:
                    operations._play_canonical_composition = original
                assert len(context.builders) == 1
                assert len(context.builders[0].children) == 1
                assert not activated
                assert not scene._noon_async_continuation_pending
                assert not scene._noon_async_continuation_mode
                assert not scene._noon_portable_construct_mode
                assert not scene._noon_portable_barrier_call

            asyncio.run(main())
            """
        )
        result = subprocess.run(
            [sys.executable, "-c", source], cwd=Path(__file__).resolve().parent,
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_pulse_uses_ordinary_builder_and_shared_option_resolver(self):
        source = textwrap.dedent(
            """
            import math
            import sys
            from types import ModuleType, SimpleNamespace
            from unittest.mock import Mock

            bridge = ModuleType("js")
            resolved = SimpleNamespace(runTime=0.6, rateFunc="linear", lagRatio=0.0,
                pathArc=0.0, reverseRateFunction=False)
            resolver = Mock(return_value=resolved)
            bridge.noonResolveAnimationOptions = resolver
            bridge.noonResolveGlowPulseAnimationOptions = resolver
            sys.modules["js"] = bridge

            from noon import GlowPulse, Mobject, Scene, Group, linear, there_and_back
            import _manim_scene as operations

            scene = Scene()
            target = object.__new__(Mobject)
            target._scene = scene
            target._semantic_handle = object()
            builder = Mock()
            scene._canonical_authoring_context = SimpleNamespace(
                beginOrdinaryCompositionBuilder=Mock(return_value=builder),
                ordinaryCanPlayComposition=Mock(return_value=True))
            pulse = GlowPulse(target, intensity=2.0)
            resolver.assert_not_called()
            candidate = operations._build_canonical_composition_candidate(
                scene, "parallel", (pulse,), None,
                {"run_time": 0.6, "rate_func": linear},
            )
            assert candidate[0] is builder
            builder.appendGlowPulseMobject.assert_called_once_with(
                target._semantic_handle, 2.0, 0.6, "linear", 0.0, False)
            args = resolver.call_args.args
            assert args[1] == 1.0 and args[2] == "there_and_back"
            assert args[6] == 0.6 and args[7] == "linear"

            class StopDuration:
                def __float__(self): raise StopIteration("duration coercion")
            try:
                operations._play(scene, run_time=StopDuration())
            except StopIteration as error:
                assert error.args == ("duration coercion",)
            else:
                raise AssertionError("the synchronous play API changed StopIteration")

            resolved.reverseRateFunction = True
            reversed_parameters = operations._glow_pulse_parameters(
                GlowPulse(target, intensity=2, reverse_rate_function=True), {})
            assert resolver.call_args.args[5] == 1
            assert reversed_parameters[-1] is True
            resolved.reverseRateFunction = False

            resolver.reset_mock()
            operations._glow_pulse_parameters(
                GlowPulse(target, intensity=2.0, run_time=float("nan")), {})
            assert math.isnan(resolver.call_args.args[1])
            assert resolver.call_args.args[6] is None
            operations._glow_pulse_parameters(pulse, {"run_time": float("nan")})
            assert math.isnan(resolver.call_args.args[6])

            for options in ({"color": "red"}, {"radius": 2}, {"source": "silhouette"}):
                try:
                    operations._glow_pulse_parameters(GlowPulse(target, intensity=2, **options), {})
                except NotImplementedError:
                    pass
                else:
                    raise AssertionError("pulse must not silently claim other glow parameters")

            target._scene = Scene()
            try:
                operations._canonical_glow_pulse_animation(scene, pulse)
            except ValueError:
                pass
            else:
                raise AssertionError("foreign-scene pulse target was accepted")
            group = object.__new__(Group)
            group._semantic_family_handle = object()
            try:
                operations._canonical_glow_pulse_animation(scene, GlowPulse(group, intensity=2))
            except NotImplementedError:
                pass
            else:
                raise AssertionError("pulse must not implicitly broadcast through a family")
            """
        )
        result = subprocess.run(
            [sys.executable, "-c", source], cwd=Path(__file__).resolve().parent,
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
