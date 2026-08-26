import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimApplyMethodTests(unittest.TestCase):
    def test_apply_method_builds_target_from_bound_mutator(self) -> None:
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

            from noon import (
                RED,
                RIGHT,
                ApplyMethod,
                Scene,
                Square,
                VGroup,
                linear,
            )

            scene = Scene()
            square = Square(side_length=1.2, fill_opacity=0.6, stroke_opacity=0.4)

            shift_animation = ApplyMethod(
                square.shift,
                2 * RIGHT,
                run_time=1.5,
                rate_func=linear,
            )
            assert shift_animation.source is square
            assert shift_animation.method.__self__ is square
            assert shift_animation.method_args == (2 * RIGHT,)
            scene.play(shift_animation)

            tracks = [
                track
                for track in scene.to_document()["tracks"]
                if track["object"] == square.id and track["property"] == "transform"
            ]
            assert len(tracks) == 1
            assert tracks[0]["timing"] == {
                "start_time": 0.0,
                "duration": 1.5,
                "easing": "linear",
            }
            assert tracks[0]["values"]["object"]["to"]["transform"]["translation"] == {
                "x": 2.0,
                "y": 0.0,
            }

            # Manim treats a final positional dict as kwargs for the bound method,
            # not as Transform animation kwargs.
            fill_animation = ApplyMethod(
                square.set_fill,
                RED,
                {"opacity": 0.25},
                run_time=0.5,
                rate_func=linear,
            )
            scene.play(fill_animation)
            assert abs(scene.time - 2.0) < 1e-12
            tracks = [
                track
                for track in scene.to_document()["tracks"]
                if track["object"] == square.id and track["property"] == "transform"
            ]
            assert len(tracks) == 2
            second = tracks[1]
            assert second["timing"] == {
                "start_time": 1.5,
                "duration": 0.5,
                "easing": "linear",
            }
            target_fill = second["values"]["object"]["to"]["style"]["fill"]
            assert target_fill["red"] == RED.red
            assert target_fill["green"] == RED.green
            assert target_fill["blue"] == RED.blue
            assert abs(target_fill["alpha"] - 0.25) < 1e-12

            mistaken = Square()
            invoked = mistaken.shift(RIGHT)
            try:
                ApplyMethod(invoked)
            except ValueError as error:
                assert str(error) == (
                    "Whoops, looks like you accidentally invoked the method you want to animate"
                )
            else:
                raise AssertionError("ApplyMethod must reject an already-invoked method")

            try:
                ApplyMethod(VGroup(Square(), Square()).shift, RIGHT)
            except NotImplementedError:
                pass
            else:
                raise AssertionError("retained family ApplyMethod must stay explicit")
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
