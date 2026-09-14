import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimApplyMatrixTests(unittest.TestCase):
    def test_apply_matrix_is_inert_until_shared_play_and_defaults_to_three_seconds(self) -> None:
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
                    play_run_time if math.isfinite(play_run_time) else
                    animation_run_time if math.isfinite(animation_run_time) else 1.0
                )
                result.rateFunc = play_rate_func or animation_rate_func or "smooth"
                result.lagRatio = default_lag_ratio
                result.pathArc = path_arc if math.isfinite(path_arc) else 0.0
                result.reverseRateFunction = reverse_rate_function == 1
                return result

            fake_js.noonResolveAnimationOptions = resolve_animation_options
            sys.modules["js"] = fake_js

            from _typed_geometry_test_support import identity_only_wrapper as identity
            import _manim_animate  # noqa: F401
            from noon import ApplyMatrix, ORIGIN, Rectangle

            rect = identity(Rectangle)
            matrix = [[1.0, 0.5], [0.0, 1.0]]
            animation = ApplyMatrix(matrix, rect)
            assert type(animation) is ApplyMatrix
            assert animation.source is rect
            assert animation.matrix is matrix
            assert animation.about_point == ORIGIN
            assert animation.anim_args == {"run_time": 3.0}
            assert not hasattr(animation, "target")

            custom = ApplyMatrix(matrix, rect, about_point=(1.0, -2.0), run_time=1.25)
            assert custom.about_point == (1.0, -2.0)
            assert custom.anim_args == {"run_time": 1.25}
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source], cwd=python_dir, env=env,
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(
            completed.returncode, 0,
            msg=f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
