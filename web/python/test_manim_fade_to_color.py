import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimFadeToColorTests(unittest.TestCase):
    def test_fade_to_color_uses_retained_target_state_style_transform(self) -> None:
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
                BLUE,
                GREEN,
                RED,
                FadeToColor,
                Scene,
                Square,
                VGroup,
                linear,
            )

            scene = Scene()
            square = Square(
                side_length=1.4,
                fill_color=BLUE,
                fill_opacity=0.35,
                stroke_color=GREEN,
                stroke_opacity=0.65,
                stroke_width=6,
            ).shift((1.25, -0.5)).rotate(0.2)

            animation = FadeToColor(square, RED, run_time=2.0, rate_func=linear)
            assert animation.source is square
            assert animation.anim_args["run_time"] == 2.0
            assert animation.anim_args["rate_func"] is linear

            # FadeToColor is ApplyMethod(mobject.set_color, ...), whose target is copied
            # in Transform.begin(). Later mutations must therefore supply the source
            # alpha/transform state while only color channels change in the target.
            square.set_fill(BLUE, opacity=0.55)
            square.set_stroke(GREEN, opacity=0.25, width=7)
            square.shift((0.5, 0.25))

            scene.play(animation)
            assert abs(scene.time - 2.0) < 1e-12
            assert square._scene is scene

            tracks = [
                track
                for track in scene.to_document()["tracks"]
                if track["object"] == square.id and track["property"] == "transform"
            ]
            assert len(tracks) == 1
            track = tracks[0]
            assert track["timing"] == {
                "start_time": 0.0,
                "duration": 2.0,
                "easing": "linear",
            }

            source_state = track["values"]["object"]["from"]
            target_state = track["values"]["object"]["to"]
            assert source_state["transform"]["translation"] == {"x": 1.75, "y": -0.25}
            assert target_state["transform"] == source_state["transform"]
            assert target_state["style"]["fill"]["red"] == RED.red
            assert target_state["style"]["fill"]["green"] == RED.green
            assert target_state["style"]["fill"]["blue"] == RED.blue
            assert target_state["style"]["stroke"]["red"] == RED.red
            assert target_state["style"]["stroke"]["green"] == RED.green
            assert target_state["style"]["stroke"]["blue"] == RED.blue
            assert abs(source_state["style"]["fill"]["alpha"] - 0.55) < 1e-12
            assert abs(source_state["style"]["stroke"]["alpha"] - 0.25) < 1e-12
            assert abs(target_state["style"]["fill"]["alpha"] - 0.55) < 1e-12
            assert abs(target_state["style"]["stroke"]["alpha"] - 0.25) < 1e-12
            assert abs(source_state["style"]["stroke_width"] - 7.0) < 1e-12
            assert abs(target_state["style"]["stroke_width"] - 7.0) < 1e-12

            try:
                FadeToColor(VGroup(Square(), Square()), RED)
            except NotImplementedError:
                pass
            else:
                raise AssertionError("retained family FadeToColor must stay explicit")
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
