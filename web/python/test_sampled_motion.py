"""Real Python routing tests; shared Rust tests own sampled-motion semantics."""

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
bridge.noonResolveAnimationOptions = Mock(
    side_effect=AssertionError("sampled records must not resolve options per target"))
sys.modules["js"] = bridge

import noon
from noon import SampledMotion, Mobject, Scene, AnimationGroup, Succession, Wait, linear
import _manim_scene as operations
import _noon_sampled_motion as sampled

sampled._bulk = Mock(side_effect=lambda values: values)
sampled._strings = Mock(side_effect=lambda values: list(values))
builders = []
def new_builder(*args):
    builder = Mock()
    builders.append(builder)
    return builder

scene = Scene()
context = SimpleNamespace(
    beginOrdinaryCompositionBuilder=Mock(side_effect=new_builder),
    ordinaryCanPlayComposition=Mock(return_value=True))
scene._canonical_authoring_context = context

def target(index, owner=scene):
    result = object.__new__(Mobject)
    result._scene = owner
    result._semantic_handle = object()
    result._object = SimpleNamespace(id=index) if owner is not None else None
    return result

def candidate(animations, options=None):
    return operations._build_canonical_composition_candidate(
        scene, "parallel", tuple(animations), None, options or {})

def motion(index):
    return SampledMotion(target(index), [0.13, 0.73, 1.0],
                         [(1, 2), (3, 4, 0), (-1, -2)],
                         [-3.0, -10.0, -6 * math.pi])
"""


class SampledMotionRoutingTests(unittest.TestCase):
    def run_python(self, source):
        result = subprocess.run(
            [sys.executable, "-c", textwrap.dedent(SETUP) + textwrap.dedent(source)],
            cwd=Path(__file__).resolve().parent,
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_public_scene_play_batches_many_records_in_one_host_admission(self):
        self.run_python("""
            assert "SampledMotion" in noon.__all__
            animations = [motion(index) for index in range(61)]
            bridge.noonResolveAnimationOptions.assert_not_called()
            with patch.object(operations, "_play_canonical_composition", return_value=scene) as play:
                assert scene.play(*animations, run_time=2.3, rate_func=linear) is scene
                play.assert_called_once()
            assert len(builders) == 1
            builders[0].appendSampledMotionBatch.assert_called_once()
            bridge.noonResolveAnimationOptions.assert_not_called()
            assert sampled._bulk.call_count == 6
            assert sampled._strings.call_count == 2
            arguments = builders[0].appendSampledMotionBatch.call_args.args
            assert arguments[0] is context
            assert arguments[1] == [str(index) for index in range(61)]
            assert list(arguments[2]) == [3 * (index + 1) for index in range(61)]
            assert len(arguments[3]) == 183
            assert arguments[-3:] == (2.3, "linear", None)
            context.ordinaryCanPlayComposition.assert_called_once_with(builders[0])
            assert not any("copy" in str(call).lower() for call in builders[0].mock_calls)
        """)

    def test_ragged_arrays_and_option_presence_cross_without_python_defaults(self):
        self.run_python("""
            first = SampledMotion(target(7), [0.37, 1], [(2, 1), (4, 3)],
                [-2, -6 * math.pi], run_time=0.7, rate_func=linear,
                reverse_rate_function=True, lag_ratio=0.0, path_arc=0.0)
            second = SampledMotion(target(9), [1], [(-3, -2)])
            candidate([first, second], {"duration": 2.3, "easing": "there_and_back"})
            args = builders[0].appendSampledMotionBatch.call_args.args
            assert args[1] == ["7", "9"]
            assert [buffer.typecode for buffer in args[2:8]] == ["I", "d", "d", "d", "B", "d"]
            assert list(args[2]) == [2, 3]
            assert list(args[3]) == [0.37, 1, 1]
            assert list(args[4]) == [2, 1, 0, 4, 3, 0, -3, -2, 0]
            assert list(args[5]) == [-2, -6 * math.pi, 0]
            assert list(args[6]) == [31, 0]
            assert list(args[7]) == [0.7, 0, 0, 0, 0, 0]
            assert args[8] == ["linear", ""]
            assert args[9:] == (2.3, "there_and_back", None)
            bridge.noonResolveAnimationOptions.assert_not_called()
        """)

    def test_composition_preserves_order_and_batches_each_consecutive_run(self):
        self.run_python("""
            nested = Succession(motion(20), motion(21), run_time=4.7)
            candidate([motion(1), motion(2), Wait(0.2), motion(3), nested, motion(4)])
            root, inner = builders
            names = [call[0] for call in root.mock_calls]
            assert names == ["setCompositionRateFunction", "appendSampledMotionBatch",
                "appendWait", "appendSampledMotionBatch", "appendComposition",
                "appendSampledMotionBatch"], names
            assert [call.args[1] for call in root.appendSampledMotionBatch.call_args_list] == [
                ["1", "2"], ["3"], ["4"]]
            root.appendComposition.assert_called_once_with(inner)
            inner.appendSampledMotionBatch.assert_called_once()
            assert inner.appendSampledMotionBatch.call_args.args[1] == ["20", "21"]
            assert context.beginOrdinaryCompositionBuilder.call_args_list[1].args == (
                "sequence", 4.7, 1.0, None)
            bridge.noonResolveAnimationOptions.assert_not_called()
        """)

    def test_predeclaration_is_detached_but_play_requires_current_membership(self):
        self.run_python("""
            detached = target(7, None)
            request = SampledMotion(detached, [1], [(3, 4)])
            for owner in [None, Scene()]:
                detached._scene = owner
                try:
                    candidate([request])
                except ValueError as error:
                    assert "belong to this Scene" in str(error)
                else:
                    raise AssertionError("unbound or foreign target crossed to the engine")
            assert all(builder.appendSampledMotionBatch.call_count == 0 for builder in builders)
            detached._scene = scene
            detached._object = SimpleNamespace(id=7)
            candidate([request])
            builders[-1].appendSampledMotionBatch.assert_called_once()
            assert request.key_times == (1.0,)
            assert request.positions == ((3.0, 4.0, 0.0),)
        """)

    def test_python_checks_shapes_and_forwards_semantic_failures_to_rust(self):
        self.run_python("""
            for keys, points, angles in [([1], [(1,)], None),
                                         ([0.5, 1], [(1, 2)], None),
                                         ([1], [(1, 2)], [0, 1])]:
                try:
                    SampledMotion(target(0), keys, points, angles)
                except ValueError:
                    pass
                else:
                    raise AssertionError("invalid Python shape was accepted")
            # No Python phase, finiteness, planar, timing or angle policy.
            request = SampledMotion(target(0), [0, float("nan"), 0.9],
                [(1, 2, 3), (float("inf"), 2, 0), (0, 0, 0)],
                [-100, float("nan"), 100], run_time=float("nan"),
                path_arc=0.4, lag_ratio=0.2, reverse_rate_function=False)
            candidate([request])
            args = builders[-1].appendSampledMotionBatch.call_args.args
            assert math.isnan(args[3][1]) and args[3][0] == 0
            assert args[4][2] == 3 and math.isinf(args[4][3])
            assert math.isnan(args[5][1])
            assert list(args[6]) == [15]
            assert math.isnan(args[7][0]) and list(args[7])[1:] == [0.2, 0.4]
            original = ValueError("shared sampled motion validation")
            builder = Mock()
            builder.appendSampledMotionBatch.side_effect = original
            try:
                sampled.append_batch(scene, context, builder, [request], {})
            except ValueError as error:
                assert error is original
            else:
                raise AssertionError("shared validation failure was swallowed")
        """)

    def test_unsupported_options_and_alias_conflicts_do_not_cross(self):
        self.run_python("""
            for request, kwargs in [
                (SampledMotion(target(0), [1], [(0, 0)], axis=(0, 0, 1)), {}),
                (motion(0), {"duration": 1, "run_time": 1}),
                (motion(0), {"easing": "linear", "rate_func": linear}),
            ]:
                before = sum(builder.appendSampledMotionBatch.call_count for builder in builders)
                try:
                    candidate([request], kwargs)
                except (ValueError, NotImplementedError):
                    pass
                else:
                    raise AssertionError("unsupported options silently ignored")
                assert sum(builder.appendSampledMotionBatch.call_count for builder in builders) == before
        """)


if __name__ == "__main__":
    unittest.main()
