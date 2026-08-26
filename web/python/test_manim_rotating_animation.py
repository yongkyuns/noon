import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimRotatingAnimationTests(unittest.TestCase):
    def test_centered_2d_rotating_matches_manim_defaults_and_options(self) -> None:
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

            from noon import IN, ORIGIN, PI, RIGHT, TAU, Rotating, Scene, Square, linear, smooth

            # ManimCE v0.21 Rotating defaults: one full turn, five seconds, linear.
            scene = Scene()
            square = Square()
            scene.add(square)
            animation = Rotating(square)
            assert animation.about_point is None
            assert abs(animation.angle - TAU) < 1e-12
            scene.play(animation)
            assert abs(scene.time - 5.0) < 1e-12
            document = scene.to_document()
            rotation = next(
                track
                for track in document["tracks"]
                if track["object"] == square.id and track["property"] == "rotation"
            )
            values = rotation["values"]["scalar"]
            assert abs(values["from"] - 0.0) < 1e-12
            assert abs(values["to"] - TAU) < 1e-12
            assert abs(rotation["timing"]["duration"] - 5.0) < 1e-12
            assert rotation["timing"]["easing"] == "linear"

            # Animation-local options are accepted, while Scene.play overrides retain
            # the same precedence shared by the rest of the Manim compatibility layer.
            override_scene = Scene()
            override_square = Square()
            override_scene.add(override_square)
            override_scene.play(
                Rotating(
                    override_square,
                    angle=PI / 2,
                    run_time=3.0,
                    rate_func=smooth,
                ),
                run_time=0.75,
                rate_func=linear,
            )
            override_track = next(
                track
                for track in override_scene.to_document()["tracks"]
                if track["object"] == override_square.id and track["property"] == "rotation"
            )
            assert abs(override_scene.time - 0.75) < 1e-12
            assert abs(override_track["values"]["scalar"]["to"] - PI / 2) < 1e-12
            assert abs(override_track["timing"]["duration"] - 0.75) < 1e-12
            assert override_track["timing"]["easing"] == "linear"

            # OUT/IN z-axis behavior maps directly to the scalar rotation channel.
            in_scene = Scene()
            in_square = Square()
            in_scene.add(in_square)
            in_scene.play(Rotating(in_square, angle=PI / 2, axis=IN, run_time=1.0))
            in_track = next(
                track
                for track in in_scene.to_document()["tracks"]
                if track["property"] == "rotation"
            )
            assert abs(in_track["values"]["scalar"]["to"] + PI / 2) < 1e-12

            # An explicit center pivot is representable and remains exact.
            centered_scene = Scene()
            centered_square = Square()
            centered_scene.add(centered_square)
            centered_scene.play(
                Rotating(centered_square, angle=PI, about_point=ORIGIN, run_time=1.0)
            )
            centered_track = next(
                track
                for track in centered_scene.to_document()["tracks"]
                if track["property"] == "rotation"
            )
            assert abs(centered_track["values"]["scalar"]["to"] - PI) < 1e-12

            # External pivots/edges and non-z axes need curved translation/3D state.
            # They must fail atomically rather than silently approximating Manim.
            edge_scene = Scene()
            edge_square = Square()
            try:
                edge_scene.play(Rotating(edge_square, about_edge=RIGHT))
            except NotImplementedError:
                pass
            else:
                raise AssertionError("external Rotating about_edge should be rejected")
            assert edge_scene.time == 0.0
            assert edge_scene.to_document()["objects"] == []
            assert edge_square._scene is None and edge_square._object is None

            axis_scene = Scene()
            axis_square = Square()
            try:
                axis_scene.play(Rotating(axis_square, axis=(1.0, 0.0, 0.0)))
            except NotImplementedError:
                pass
            else:
                raise AssertionError("non-z Rotating axis should be rejected")
            assert axis_scene.time == 0.0
            assert axis_scene.to_document()["objects"] == []
            assert axis_square._scene is None and axis_square._object is None
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
