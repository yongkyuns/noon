import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimAnimationConstructorOptionsTests(unittest.TestCase):
    def test_constructor_runtime_and_rate_options_flow_into_scene_play(self) -> None:
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

            from noon import Circle, Create, FadeIn, Scene, Square, Transform, linear, smooth

            # Constructor options are part of the public animation object, not a
            # Noon-only Scene.play workaround.
            transform = Transform(Square(), Circle(), run_time=1.25, path_arc=0.3)
            assert transform.anim_args == {"run_time": 1.25, "path_arc": 0.3}

            # Unequal intrinsic runtimes remain concurrent. Scene duration is the
            # longest child, exactly as Manim's play compilation expects.
            scene = Scene()
            square = Square()
            circle = Circle()
            scene.play(
                Create(square, run_time=2.0, rate_func=linear),
                FadeIn(circle, run_time=0.5),
            )
            assert abs(scene.time - 2.0) < 1e-12

            document = scene.to_document()
            reveal = next(track for track in document["tracks"] if track["property"] == "reveal")
            appearance = next(
                track
                for track in document["tracks"]
                if track["object"] == circle.id and track["property"] == "appearance"
            )
            assert abs(reveal["timing"]["duration"] - 2.0) < 1e-12
            assert reveal["timing"]["easing"] == "linear"
            assert abs(appearance["timing"]["duration"] - 0.5) < 1e-12
            assert appearance["timing"]["easing"] == "smooth"

            # Scene.play options override constructor options through the same shared
            # resolver rather than mutating the animation object or double-applying.
            override_scene = Scene()
            override_square = Square()
            override_scene.play(
                Create(override_square, run_time=3.0, rate_func=linear),
                run_time=0.75,
                rate_func=smooth,
            )
            override_track = next(
                track
                for track in override_scene.to_document()["tracks"]
                if track["property"] == "reveal"
            )
            assert abs(override_scene.time - 0.75) < 1e-12
            assert abs(override_track["timing"]["duration"] - 0.75) < 1e-12
            assert override_track["timing"]["easing"] == "smooth"
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
