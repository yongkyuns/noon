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

    def test_canonical_reverse_smooth_lambda_maps_to_shared_transform_option(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = str(python_dir)
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

            def transform(*args):
                result = Result()
                result.runTime = 1.0
                result.rateFunc = args[2] or "smooth"
                result.lagRatio = 0.0
                result.pathArc = 0.0
                result.reverseRateFunction = args[5] == 1
                assert result.rateFunc == "smooth"
                assert result.reverseRateFunction
                return result

            fake_js.noonResolveAnimationOptions = transform
            fake_js.noonResolveTransformAnimationOptions = transform
            sys.modules["js"] = fake_js
            import _manim_animation_options as options
            from _manim_rate_functions import smooth

            class Animation:
                anim_args = {"rate_func": lambda t: smooth(1 - t)}

            resolved = options.resolve_transform(
                builder_args=options.builder_args(Animation()),
                default_lag_ratio=0.0,
                play_run_time=None,
                play_easing=None,
                play_rate_func=None,
                play_lag_ratio=None,
                play_path_arc=None,
            )
            assert resolved.rate_func == "smooth"
            assert resolved.reverse_rate_function

            play_resolved = options.resolve_transform(
                builder_args={},
                default_lag_ratio=0.0,
                play_run_time=None,
                play_easing=None,
                play_rate_func=lambda t: smooth(1 - t),
                play_lag_ratio=None,
                play_path_arc=None,
            )
            assert play_resolved.rate_func == "smooth"
            assert play_resolved.reverse_rate_function

            class Unsupported:
                anim_args = {"rate_func": lambda t: smooth(1 - t) + 0.0}

            try:
                options.resolve_transform(
                    builder_args=options.builder_args(Unsupported()),
                    default_lag_ratio=0.0,
                    play_run_time=None,
                    play_easing=None,
                    play_rate_func=None,
                    play_lag_ratio=None,
                    play_path_arc=None,
                )
            except NotImplementedError:
                pass
            else:
                raise AssertionError("arbitrary Python easing was admitted")

            import _manim_rate_functions as rates
            namespace = {"smooth": lambda t: t * t}
            namespace["smooth"].__name__ = "smooth"
            impostor = eval("lambda t: smooth(1 - t)", namespace)
            assert not rates.is_reverse_smooth_rate_func(impostor)
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
