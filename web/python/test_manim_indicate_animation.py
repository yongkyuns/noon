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

            from noon import BLUE, Indicate, RIGHT, Scene, Square, VGroup, linear

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

            scene.play(animation)
            assert abs(scene.time - 1.0) < 1e-12
            tracks = [
                track
                for track in scene.to_document()["tracks"]
                if track["object"] == square.id and track["property"] == "transform"
            ]
            assert len(tracks) == 2
            tracks.sort(key=lambda track: track["timing"]["start_time"])
            outward, returning = tracks
            assert abs(outward["timing"]["start_time"] - 0.0) < 1e-12
            assert abs(outward["timing"]["duration"] - 0.5) < 1e-12
            assert outward["timing"]["easing"] == "smooth"
            assert abs(returning["timing"]["start_time"] - 0.5) < 1e-12
            assert abs(returning["timing"]["duration"] - 0.5) < 1e-12
            assert returning["timing"]["easing"] == "smooth"

            outward_target = outward["values"]["object"]["to"]
            assert abs(outward_target["transform"]["scale"]["x"] - 1.2) < 1e-12
            assert abs(outward_target["transform"]["scale"]["y"] - 1.2) < 1e-12
            indicated_fill = outward_target["style"]["fill"]
            assert abs(indicated_fill["red"] - 1.0) < 1e-12
            assert abs(indicated_fill["green"] - 1.0) < 1e-12
            assert abs(indicated_fill["blue"] - 0.0) < 1e-12

            source_again = returning["values"]["object"]["to"]
            assert abs(source_again["transform"]["scale"]["x"] - 1.0) < 1e-12
            assert abs(source_again["transform"]["scale"]["y"] - 1.0) < 1e-12

            # A following animation must start from the restored source state, not
            # the temporary enlarged/yellow target used at the midpoint.
            scene.play(square.animate.shift(RIGHT))
            later_tracks = [
                track
                for track in scene.to_document()["tracks"]
                if track["object"] == square.id
                and track["property"] == "transform"
                and abs(track["timing"]["start_time"] - 1.0) < 1e-12
            ]
            assert len(later_tracks) == 1
            following_source = later_tracks[0]["values"]["object"]["from"]
            assert abs(following_source["transform"]["scale"]["x"] - 1.0) < 1e-12
            source_fill = following_source["style"]["fill"]
            assert abs(source_fill["red"] - BLUE.red) < 1e-12
            assert abs(source_fill["green"] - BLUE.green) < 1e-12
            assert abs(source_fill["blue"] - BLUE.blue) < 1e-12

            # Overriding there_and_back with an ordinary rate function has normal
            # Transform endpoint semantics and remains one deterministic interval.
            linear_scene = Scene()
            linear_square = Square(fill_color=BLUE, fill_opacity=1.0, stroke_opacity=0.0)
            linear_scene.add(linear_square)
            linear_scene.play(Indicate(linear_square, rate_func=linear), run_time=2.0)
            linear_tracks = [
                track
                for track in linear_scene.to_document()["tracks"]
                if track["property"] == "transform"
            ]
            assert len(linear_tracks) == 1
            assert abs(linear_tracks[0]["timing"]["duration"] - 2.0) < 1e-12
            assert linear_tracks[0]["timing"]["easing"] == "linear"

            try:
                Indicate(VGroup(Square(), Square()))
            except NotImplementedError:
                pass
            else:
                raise AssertionError("Indicate group/family support must not be approximated")
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
