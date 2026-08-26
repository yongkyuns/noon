import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimScaleInPlaceTests(unittest.TestCase):
    def test_scale_in_place_and_shrink_use_retained_target_state_transform(self) -> None:
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
                Rectangle,
                ScaleInPlace,
                Scene,
                ShrinkToCenter,
                Square,
                VGroup,
                linear,
            )

            scene = Scene()
            rect = Rectangle(
                width=2.0,
                height=1.0,
                fill_color=BLUE,
                fill_opacity=1.0,
                stroke_opacity=0.0,
            ).shift((1.25, -0.75)).rotate(0.2)
            animation = ScaleInPlace(rect, 1.75, run_time=2.0, rate_func=linear)
            assert animation.source is rect
            assert animation.anim_args["run_time"] == 2.0
            assert animation.anim_args["rate_func"] is linear

            scene.play(animation)
            assert abs(scene.time - 2.0) < 1e-12
            assert rect._scene is scene
            tracks = [
                track
                for track in scene.to_document()["tracks"]
                if track["object"] == rect.id and track["property"] == "transform"
            ]
            assert len(tracks) == 1
            track = tracks[0]
            assert abs(track["timing"]["duration"] - 2.0) < 1e-12
            assert track["timing"]["easing"] == "linear"
            start = track["values"]["object"]["from"]["transform"]
            target = track["values"]["object"]["to"]["transform"]
            assert start["translation"] == {"x": 1.25, "y": -0.75}
            assert target["translation"] == start["translation"]
            assert abs(start["rotation"] - 0.2) < 1e-12
            assert abs(target["rotation"] - 0.2) < 1e-12
            assert abs(target["scale"]["x"] - 1.75 * start["scale"]["x"]) < 1e-12
            assert abs(target["scale"]["y"] - 1.75 * start["scale"]["y"]) < 1e-12

            shrink_scene = Scene()
            square = Square(
                side_length=1.5,
                fill_color=BLUE,
                fill_opacity=1.0,
                stroke_opacity=0.0,
            ).shift((-1.5, 0.5))
            shrink_scene.play(ShrinkToCenter(square, run_time=0.5))
            assert abs(shrink_scene.time - 0.5) < 1e-12
            shrink_tracks = [
                track
                for track in shrink_scene.to_document()["tracks"]
                if track["object"] == square.id and track["property"] == "transform"
            ]
            assert len(shrink_tracks) == 1
            shrink_target = shrink_tracks[0]["values"]["object"]["to"]["transform"]
            assert shrink_target["translation"] == {"x": -1.5, "y": 0.5}
            assert abs(shrink_target["scale"]["x"]) < 1e-12
            assert abs(shrink_target["scale"]["y"]) < 1e-12

            try:
                ScaleInPlace(VGroup(Square(), Square()), 2.0)
            except NotImplementedError:
                pass
            else:
                raise AssertionError("retained family ScaleInPlace must stay explicit")

            try:
                ScaleInPlace(Square(), float("nan"))
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
