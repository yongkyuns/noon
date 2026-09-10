import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimScaleInPlaceTests(unittest.TestCase):
    def test_scale_in_place_is_inert_until_shared_play(self) -> None:
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

            from _typed_geometry_test_support import identity_only_wrapper as identity
            import _manim_compat

            import _manim_rate_functions
            import _manim_animate  # noqa: F401
            import _manim_animation_options  # shared option resolver

            from noon import (
                BLUE,
                Rectangle,
                ScaleInPlace,
                Scene,
                Square,
                VGroup,
                linear,
            )

            rect = identity(Rectangle)
            animation = ScaleInPlace(rect, 1.75, run_time=2.0, rate_func=linear)
            assert type(animation) is ScaleInPlace
            assert animation.source is rect
            assert animation.scale_factor == 1.75
            assert animation.anim_args == {"run_time": 2.0, "rate_func": linear}
            # Construction is metadata only; shared play owns target creation.
            assert not hasattr(animation, "target")
            assert rect._scene is None
            family = identity(VGroup, submobjects=[])
            assert ScaleInPlace(family, 2.0).source is family

            try:
                ScaleInPlace(identity(Square), float("nan"))
            except ValueError:
                pass
            else:
                raise AssertionError("non-finite ScaleInPlace factor must be rejected")
            """
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
            msg=f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
