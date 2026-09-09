import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimFadeEndpointTests(unittest.TestCase):
    def test_fade_options_preserve_inert_endpoint_requests(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        existing_pythonpath = env.get("PYTHONPATH")
        env["PYTHONPATH"] = (
            str(python_dir)
            if not existing_pythonpath
            else os.pathsep.join((str(python_dir), existing_pythonpath))
        )

        source = textwrap.dedent(
            """
            import math
            import sys
            import types

            fake_js = types.ModuleType("js")

            class Result:
                ok = True
                errorKind = ""
                message = ""

            def resolve_animation_options(
                default_lag_ratio,
                animation_run_time,
                animation_rate_func,
                animation_lag_ratio,
                path_arc,
                reverse_rate_function,
                play_run_time,
                play_rate_func,
                play_lag_ratio,
            ):
                result = Result()
                result.runTime = (
                    play_run_time
                    if math.isfinite(play_run_time)
                    else animation_run_time
                    if math.isfinite(animation_run_time)
                    else 1.0
                )
                result.rateFunc = play_rate_func or animation_rate_func or "smooth"
                result.lagRatio = (
                    play_lag_ratio
                    if math.isfinite(play_lag_ratio)
                    else animation_lag_ratio
                    if math.isfinite(animation_lag_ratio)
                    else default_lag_ratio
                )
                result.pathArc = path_arc if math.isfinite(path_arc) else 0.0
                result.reverseRateFunction = reverse_rate_function == 1
                return result

            fake_js.noonResolveAnimationOptions = resolve_animation_options
            sys.modules["js"] = fake_js

            import _manim_compat

            from _test_manim_membership import install_test_membership
            install_test_membership(_manim_compat)
            import _manim_rate_functions
            import _manim_animate  # noqa: F401

            from noon import ORIGIN, FadeIn, FadeOut, RIGHT, Scene, Square, UP

            from _typed_geometry_test_support import identity_only_wrapper as identity

            class CenterCountingSquare(Square):
                def __init__(self):
                    self._scene = None
                    self.point = ORIGIN
                    self.center_reads = 0

                def get_center(self):
                    self.center_reads += 1
                    return self.point

            # Canonical target_position records the absolute point without
            # evaluating the fade target's center in Python. Mobject coercion
            # still resolves the referenced object's center at construction.
            canonical_target = CenterCountingSquare()
            point_reference = CenterCountingSquare()
            point_reference.point = RIGHT * 3.0
            canonical_target._semantic_handle = object()
            canonical_target.center_reads = 0
            point_reference.center_reads = 0
            point_fade = FadeIn(canonical_target, target_position=point_reference)
            assert canonical_target.center_reads == 0
            assert point_reference.center_reads == 1
            assert point_fade._fade_point == RIGHT * 3.0

            target = identity(Square)
            fade_in = FadeIn(target, shift=UP, scale=0.5, run_time=2.0)
            assert fade_in.target is target and target._scene is None
            assert fade_in._fade_shift_vector == UP
            assert fade_in._fade_scale_factor == 0.5
            assert fade_in.anim_args == {"run_time": 2.0}
            fade_out = FadeOut(target, shift=RIGHT * 2.0, scale=1.5, run_time=0.8)
            assert fade_out._fade_shift_vector == RIGHT * 2.0
            assert fade_out._fade_scale_factor == 1.5
            point = FadeIn(target, target_position=RIGHT * 3.0, run_time=1.25)
            assert point._fade_point_target
            assert point._fade_point == RIGHT * 3.0
            assert point._fade_shift_vector == ORIGIN
            precedence = FadeIn(target, shift=UP, target_position=RIGHT * 8.0)
            assert not precedence._fade_point_target
            assert precedence._fade_shift_vector == UP
            # The paired Rust/Python affine-fade examples qualify actual endpoints
            # through shared execution; this unit test only protects call coercion.
            """
        )

        completed = subprocess.run(
            [sys.executable, "-c", source],
            check=False,
            cwd=python_dir,
            env=env,
            capture_output=True,
            text=True,
        )
        self.assertEqual(
            completed.returncode,
            0,
            f"compatibility subprocess failed:\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
