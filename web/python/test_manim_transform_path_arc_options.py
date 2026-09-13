import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimTransformPathArcOptionsTests(unittest.TestCase):
    def test_transform_resolver_keeps_path_arc_precedence_in_shared_bridge(self) -> None:
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

            def generic(*args):
                raise AssertionError("Transform path arcs must not use the generic resolver")

            def transform(
                default_lag_ratio,
                animation_run_time,
                animation_rate_func,
                animation_lag_ratio,
                animation_path_arc,
                reverse_rate_function,
                play_run_time,
                play_rate_func,
                play_lag_ratio,
                play_path_arc,
            ):
                result = Result()
                result.runTime = play_run_time if math.isfinite(play_run_time) else 1.0
                result.rateFunc = play_rate_func or animation_rate_func or "smooth"
                result.lagRatio = (
                    play_lag_ratio if math.isfinite(play_lag_ratio) else default_lag_ratio
                )
                result.pathArc = (
                    play_path_arc
                    if math.isfinite(play_path_arc)
                    else animation_path_arc
                    if math.isfinite(animation_path_arc)
                    else 0.0
                )
                result.reverseRateFunction = reverse_rate_function == 1
                return result

            fake_js.noonResolveAnimationOptions = generic
            fake_js.noonResolveTransformAnimationOptions = transform
            sys.modules["js"] = fake_js

            import _manim_animation_options as options

            class Animation:
                anim_args = {"path_arc": 0.5}

            resolved = options.resolve_transform(
                builder_args=options.builder_args(Animation()),
                default_lag_ratio=0.0,
                play_run_time=None,
                play_easing=None,
                play_rate_func=None,
                play_lag_ratio=None,
                play_path_arc=-0.75,
            )
            assert resolved.path_arc == -0.75
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
