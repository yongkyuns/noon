import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimWiggleAnimationTests(unittest.TestCase):
    def test_default_wiggle_lowers_to_retained_scale_and_rotation_tracks(self) -> None:
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
            import _manim_rotate
            _manim_rotate.install()

            from noon import RIGHT, Scene, Square, VGroup, Wiggle, linear

            scene = Scene()
            square = Square(side_length=1.5, fill_opacity=1.0, stroke_opacity=0.0)
            scene.add(square)
            animation = Wiggle(square)
            assert abs(animation.scale_value - 1.1) < 1e-12
            assert abs(animation.rotation_angle - 0.01 * math.tau) < 1e-12
            assert animation.n_wiggles == 6
            assert abs(animation.anim_args["run_time"] - 2.0) < 1e-12

            scene.play(animation)
            assert abs(scene.time - 2.0) < 1e-12
            tracks = [
                track
                for track in scene.to_document()["tracks"]
                if track["object"] == square.id
            ]
            transforms = [track for track in tracks if track["property"] == "transform"]
            rotations = [track for track in tracks if track["property"] == "rotation"]
            assert len(transforms) == 2
            assert len(rotations) == 1

            transforms.sort(key=lambda track: track["id"])
            outward, returning = transforms
            for track in transforms:
                assert abs(track["timing"]["start_time"] - 0.0) < 1e-12
                assert abs(track["timing"]["duration"] - 2.0) < 1e-12
                assert track["timing"]["easing"] == "smooth"
                assert len(track["time_map"]["steps"]) == 1
                assert track["time_map"]["steps"][0]["rate_func"] == "smooth"
            assert outward["time_map"]["steps"][0]["start"] == 0.0
            assert outward["time_map"]["steps"][0]["duration"] == 0.5
            assert returning["time_map"]["steps"][0]["start"] == 0.5
            assert returning["time_map"]["steps"][0]["duration"] == 0.5

            outward_target = outward["values"]["object"]["to"]
            assert abs(outward_target["transform"]["scale"]["x"] - 1.1) < 1e-12
            assert abs(outward_target["transform"]["scale"]["y"] - 1.1) < 1e-12
            restored = returning["values"]["object"]["to"]
            assert abs(restored["transform"]["scale"]["x"] - 1.0) < 1e-12
            assert abs(restored["transform"]["scale"]["y"] - 1.0) < 1e-12

            rotation = rotations[0]
            assert rotation["timing"]["easing"] == "wiggle_6"
            assert abs(rotation["timing"]["start_time"] - 0.0) < 1e-12
            assert abs(rotation["timing"]["duration"] - 2.0) < 1e-12
            assert rotation["time_map"] == {
                "steps": [{"start": 0.0, "duration": 1.0, "rate_func": "smooth"}]
            }
            values = rotation["values"]["scalar"]
            assert abs(values["from"] - 0.0) < 1e-12
            assert abs(values["to"] - 0.01 * math.tau) < 1e-12

            # The retained evaluator mirrors Manim's internal wiggle(alpha, 6).
            assert abs(_manim_rate_functions._wiggle_6(0.25) + 0.5) < 1e-12
            assert abs(_manim_rate_functions._wiggle_6(0.0)) < 1e-12
            assert abs(_manim_rate_functions._wiggle_6(1.0)) < 1e-12

            # A following animation starts from the restored source transform.
            scene.play(square.animate.shift(RIGHT))
            later = [
                track
                for track in scene.to_document()["tracks"]
                if track["property"] == "transform"
                and abs(track["timing"]["start_time"] - 2.0) < 1e-12
            ]
            assert len(later) == 1
            following_source = later[0]["values"]["object"]["from"]
            assert abs(following_source["transform"]["scale"]["x"] - 1.0) < 1e-12
            assert abs(following_source["transform"]["rotation"] - 0.0) < 1e-12

            try:
                Wiggle(VGroup(Square(), Square()))
            except NotImplementedError:
                pass
            else:
                raise AssertionError("Wiggle family support must not be approximated")

            try:
                Wiggle(Square(), n_wiggles=5)
            except NotImplementedError:
                pass
            else:
                raise AssertionError("nondefault wiggle count must not be approximated")

            pivot_scene = Scene()
            pivot_square = Square()
            pivot_scene.add(pivot_square)
            try:
                pivot_scene.play(Wiggle(pivot_square, rotate_about_point=RIGHT))
            except NotImplementedError:
                pass
            else:
                raise AssertionError("external Wiggle pivots must not be approximated")

            rate_scene = Scene()
            rate_square = Square()
            rate_scene.add(rate_square)
            try:
                rate_scene.play(Wiggle(rate_square), rate_func=linear)
            except NotImplementedError:
                pass
            else:
                raise AssertionError("unsupported custom Wiggle rate must not be approximated")
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
