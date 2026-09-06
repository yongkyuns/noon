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

            def resolve_uniform_schedule(child_count, lag_ratio, run_time):
                result = Result()
                result.intervals = []
                return result

            fake_js.noonResolveAnimationOptions = resolve_animation_options
            fake_js.noonResolveUniformCompositionSchedule = resolve_uniform_schedule
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_rate_functions
            _manim_rate_functions.install()
            import _manim_phase_b  # noqa: F401
            import _manim_animate  # noqa: F401

            from noon import BLUE, GREEN, Indicate, RIGHT, Scene, Square, VGroup, linear

            # Manim set_color changes RGB without changing independent fill/stroke opacity.
            style_probe = Square(
                fill_color=BLUE,
                fill_opacity=0.35,
                stroke_color=BLUE,
                stroke_opacity=0.65,
            )
            style_probe.set_color(GREEN)
            assert abs(style_probe.style["fill"]["alpha"] - 0.35) < 1e-12
            assert abs(style_probe.style["stroke"]["alpha"] - 0.65) < 1e-12

            scene = Scene()
            square = Square(
                side_length=1.5,
                fill_color=BLUE,
                fill_opacity=1.0,
                stroke_opacity=0.0,
            )
            scene.add(square)
            animation = Indicate(square)
            assert abs(animation.scale_factor - 1.2) < 1e-12
            assert animation.anim_args["rate_func"].__name__ == "there_and_back"

            # Compatibility construction stays inert. Playback must be claimed by
            # the canonical shared semantic path rather than expanding snapshots
            # and two Python-authored intervals.
            try:
                import _manim_animate as animate
                animate._expanded_schedule(
                    scene,
                    animation,
                    start_time=0.0,
                    run_time=1.0,
                    easing="there_and_back",
                    lag_ratio=0.0,
                )
            except NotImplementedError:
                pass
            else:
                raise AssertionError("Indicate must not use Python schedule expansion")

            family = Indicate(VGroup(Square(), Square()))
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
