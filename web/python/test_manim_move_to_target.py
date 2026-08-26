import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimMoveToTargetTests(unittest.TestCase):
    def test_docs_example_lowers_to_generic_transform(self) -> None:
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

            from noon import Circle, GREEN, MoveToTarget, RIGHT, Scene, Transform, UP

            missing = Circle()
            try:
                MoveToTarget(missing)
                raise AssertionError("MoveToTarget must require generate_target()")
            except ValueError as error:
                assert str(error) == "MoveToTarget called on mobjectwithout attribute 'target'"

            circle = Circle()
            generated = circle.generate_target()
            assert generated is circle.target
            assert generated is not circle
            circle.target.set_fill(color=GREEN, opacity=0.5)
            circle.target.shift(2 * RIGHT + UP).scale(0.5)

            scene = Scene()
            scene.add(circle)
            animation = MoveToTarget(circle)
            assert isinstance(animation, Transform)
            scene.play(animation)

            tracks = [track for track in scene.to_ir()["tracks"] if track["property"] == "transform"]
            assert len(tracks) == 1
            track = tracks[0]
            assert abs(track["timing"]["start_time"] - 0.0) < 1e-12
            assert abs(track["timing"]["duration"] - 1.0) < 1e-12
            assert track["timing"]["easing"] == "smooth"

            target = track["values"]["object"]["to"]
            translation = target["transform"]["translation"]
            scale = target["transform"]["scale"]
            assert abs(translation["x"] - 2.0) < 1e-12
            assert abs(translation["y"] - 1.0) < 1e-12
            assert abs(scale["x"] - 0.5) < 1e-12
            assert abs(scale["y"] - 0.5) < 1e-12
            fill = target["style"]["fill"]
            assert abs(fill["red"] - GREEN.red) < 1e-12
            assert abs(fill["green"] - GREEN.green) < 1e-12
            assert abs(fill["blue"] - GREEN.blue) < 1e-12
            assert abs(fill["alpha"] - 0.5) < 1e-12

            final = scene._snapshot_for_object_at(circle._object, 1.0)
            assert final == target

            # Transform snapshots its target at play time, matching Manim begin().
            circle2 = Circle()
            circle2.generate_target()
            pending = MoveToTarget(circle2)
            circle2.target.shift(RIGHT)
            scene2 = Scene()
            scene2.add(circle2)
            scene2.play(pending)
            target2 = [
                track for track in scene2.to_ir()["tracks"] if track["property"] == "transform"
            ][0]["values"]["object"]["to"]
            assert abs(target2["transform"]["translation"]["x"] - 1.0) < 1e-12
            """
        )

        completed = subprocess.run(
            [sys.executable, "-c", source],
            cwd=python_dir,
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)


if __name__ == "__main__":
    unittest.main()
