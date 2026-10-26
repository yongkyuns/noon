"""Boundary routing only; shared Rust tests own pulse sampling and lifecycle."""
from pathlib import Path
import subprocess
import sys
import textwrap
import unittest


class GlowPulseRoutingTests(unittest.TestCase):
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
