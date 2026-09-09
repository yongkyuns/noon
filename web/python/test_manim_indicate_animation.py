import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimIndicateAnimationTests(unittest.TestCase):
    def test_indicate_matches_default_there_and_back_semantics(self) -> None:
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
            _manim_compat.install()
            from _test_manim_membership import install_test_membership
            install_test_membership(_manim_compat)
            import _manim_rate_functions
            _manim_rate_functions.install()
            import _manim_phase_b  # noqa: F401
            import _manim_animate  # noqa: F401

            from noon import BLUE, GREEN, Indicate, RIGHT, Scene, Square, VGroup, linear

            from _typed_geometry_test_support import identity_only_wrapper as identity
            square = identity(Square)
            animation = Indicate(square)
            assert abs(animation.scale_factor - 1.2) < 1e-12
            assert animation.anim_args["rate_func"].__name__ == "there_and_back"

            # Compatibility construction stays inert. Playback must be claimed by
            # the canonical shared semantic path rather than expanding snapshots
            # and two Python-authored intervals.
            import _manim_animate as animate
            assert not hasattr(animate, "_expanded_schedule")

            family = Indicate(identity(VGroup, submobjects=[]))
            assert abs(family.scale_factor - 1.2) < 1e-12
            assert family.anim_args["rate_func"].__name__ == "there_and_back"
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
