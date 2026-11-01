"""Python request routing; shared Rust tests own phase and path semantics."""
from pathlib import Path
import subprocess
import sys
import textwrap
import unittest


SETUP = """
import math
import sys
from types import ModuleType, SimpleNamespace
from unittest.mock import Mock, patch

bridge = ModuleType("js")
bridge.noonResolveAnimationOptions = Mock(side_effect=AssertionError("resolve in Rust admission"))
sys.modules["js"] = bridge
import noon
from noon import PhaseReveal, Mobject, Scene, Succession, Wait, linear
import _manim_scene as operations
import _noon_phase_reveal as reveal
reveal._bulk = Mock(side_effect=lambda values: list(values))
scene = Scene()
builders = []
def new_builder(*args):
    result = Mock()
    builders.append(result)
    return result
context = SimpleNamespace(beginOrdinaryCompositionBuilder=Mock(side_effect=new_builder),
                          ordinaryCanPlayComposition=Mock(return_value=True))
scene._canonical_authoring_context = context
def target(owner=scene):
    result = object.__new__(Mobject)
    result._scene = owner
    result._semantic_handle = object()
    result._object = SimpleNamespace(id=7) if owner is not None else None
    return result
def candidate(animations, options=None):
    return operations._build_canonical_composition_candidate(
        scene, "parallel", tuple(animations), None, options or {})
"""


class PhaseRevealRoutingTests(unittest.TestCase):
    def run_python(self, source):
        result = subprocess.run(
            [sys.executable, "-c", textwrap.dedent(SETUP) + textwrap.dedent(source)],
            cwd=Path(__file__).resolve().parent, capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_public_play_forwards_full_phase_table_and_option_presence(self):
        self.run_python("""
            assert "PhaseReveal" in noon.__all__
            request = PhaseReveal(target(), [0, .13, .73, 1], run_time=.7,
                rate_func=linear, reverse_rate_function=True, remover=True)
            with patch.object(operations, "_play_canonical_composition", return_value=scene) as play:
                assert scene.play(request, duration=2.3, easing="there_and_back") is scene
                play.assert_called_once()
            builders[0].appendPhaseReveal.assert_called_once()
            args = builders[0].appendPhaseReveal.call_args.args
            assert args[:3] == ("", request.mobject._semantic_handle, [0, .13, .73, 1])
            assert args[3:] == (.7, "linear", None, None, True, None, True,
                               2.3, "there_and_back", None), args
            bridge.noonResolveAnimationOptions.assert_not_called()
            context.ordinaryCanPlayComposition.assert_called_once_with(builders[0])
        """)

    def test_nested_composition_preserves_order_and_ordinary_root_timing(self):
        self.run_python("""
            request = PhaseReveal(target(), [0, .37, 1])
            candidate([Wait(.2), Succession(request, Wait(.1), run_time=4.7)])
            root, inner = builders
            assert [call[0] for call in root.mock_calls] == [
                "setCompositionRateFunction", "appendWait", "appendComposition"]
            inner.appendPhaseReveal.assert_called_once()
            assert inner.appendPhaseReveal.call_args.args[3:] == (None,) * 10
            assert context.beginOrdinaryCompositionBuilder.call_args_list[1].args == (
                "sequence", 4.7, 1.0, None)
        """)

    def test_python_preserves_invalid_semantic_values_for_shared_validation(self):
        self.run_python("""
            request = PhaseReveal(target(), [0, float("nan"), .9],
                run_time=float("inf"), path_arc=.2, lag_ratio=.3,
                introducer=False, reverse_rate_function=False)
            candidate([request])
            args = builders[0].appendPhaseReveal.call_args.args
            assert math.isnan(args[2][1]) and args[2][-1] == .9
            assert math.isinf(args[3]) and args[5:9] == (.3, .2, False, False)
            original = ValueError("shared phase/path validation")
            builder = Mock()
            builder.appendPhaseReveal.side_effect = original
            try:
                reveal.append(builder, request, "", {})
            except ValueError as error:
                assert error is original
            else:
                raise AssertionError("shared semantic error was swallowed")
        """)

    def test_wrapper_ownership_and_unknown_options_never_publish(self):
        self.run_python("""
            for request in [PhaseReveal(target(Scene()), [0, 1]),
                            PhaseReveal(target(), [0, 1], dissipating_time=1)]:
                try:
                    candidate([request])
                except (ValueError, NotImplementedError):
                    pass
                else:
                    raise AssertionError("unsupported request reached publication")
            assert all(builder.appendPhaseReveal.call_count == 0 for builder in builders)
            context.ordinaryCanPlayComposition.assert_not_called()
        """)

    def test_request_construction_is_inert_and_validates_only_python_shapes(self):
        self.run_python("""
            detached = target(None)
            request = PhaseReveal(detached, (value for value in [0, .2, 1]))
            assert request.key_times == (0, .2, 1)
            assert detached._scene is None and not builders
            try:
                PhaseReveal(object(), [0, 1])
            except TypeError:
                pass
            else:
                raise AssertionError("non-Mobject target accepted")
        """)


if __name__ == "__main__":
    unittest.main()
