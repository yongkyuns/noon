import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimRotateAnimationTests(unittest.TestCase):
    def test_centered_2d_rotate_uses_procedural_rotation_track(self) -> None:
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

            from noon import IN, LEFT, ORIGIN, PI, RIGHT, Line, Rotate, Scene, Square, linear, smooth

            # Canonical quickstart subset: a shifted Square is still centered on its
            # transform origin, so explicit Rotate is a true angular path rather than
            # the target-state interpolation used by square.animate.rotate(...).
            scene = Scene()
            square = Square().shift(2 * RIGHT)
            scene.play(Rotate(square, angle=PI, rate_func=linear), run_time=2.0)
            assert abs(scene.time - 2.0) < 1e-12
            document = scene.to_document()
            rotation = next(
                track
                for track in document["tracks"]
                if track["object"] == square.id and track["property"] == "rotation"
            )
            values = rotation["values"]["scalar"]
            assert abs(values["from"] - 0.0) < 1e-12
            assert abs(values["to"] - PI) < 1e-12
            assert abs(rotation["timing"]["duration"] - 2.0) < 1e-12
            assert rotation["timing"]["easing"] == "linear"
            authored = next(obj for obj in document["objects"] if obj["id"] == square.id)
            assert abs(authored["transform"]["translation"]["x"] - 2.0) < 1e-12
            assert abs(authored["transform"]["translation"]["y"]) < 1e-12

            # The upstream DifferentRotations shape is one concurrent Scene.play.
            # Keep the two semantics structurally different while sharing the same
            # play start/end interval.
            mixed_scene = Scene()
            left_square = Square().shift(2 * LEFT)
            right_square = Square().shift(2 * RIGHT)
            mixed_scene.play(
                left_square.animate.rotate(PI),
                Rotate(right_square, angle=PI),
                run_time=2.0,
                rate_func=linear,
            )
            assert abs(mixed_scene.time - 2.0) < 1e-12
            mixed_document = mixed_scene.to_document()
            left_tracks = [
                track
                for track in mixed_document["tracks"]
                if track["object"] == left_square.id
            ]
            right_tracks = [
                track
                for track in mixed_document["tracks"]
                if track["object"] == right_square.id
            ]
            left_transform = next(track for track in left_tracks if track["property"] == "transform")
            right_rotation = next(track for track in right_tracks if track["property"] == "rotation")
            assert not any(track["property"] == "rotation" for track in left_tracks)
            assert not any(track["property"] == "transform" for track in right_tracks)
            for track in (left_transform, right_rotation):
                assert abs(track["timing"]["start_time"] - 0.0) < 1e-12
                assert abs(track["timing"]["duration"] - 2.0) < 1e-12
                assert track["timing"]["easing"] == "linear"

            # Constructor defaults and Scene.play override precedence are shared with
            # every other Manim animation through the common Rust option resolver.
            default_scene = Scene()
            default_square = Square()
            default_scene.play(Rotate(default_square))
            default_track = next(
                track
                for track in default_scene.to_document()["tracks"]
                if track["property"] == "rotation"
            )
            assert abs(default_scene.time - 1.0) < 1e-12
            assert default_track["timing"]["easing"] == "smooth"

            override_scene = Scene()
            override_square = Square()
            override_scene.play(
                Rotate(override_square, run_time=3.0, rate_func=linear),
                run_time=0.75,
                rate_func=smooth,
            )
            override_track = next(
                track
                for track in override_scene.to_document()["tracks"]
                if track["property"] == "rotation"
            )
            assert abs(override_scene.time - 0.75) < 1e-12
            assert abs(override_track["timing"]["duration"] - 0.75) < 1e-12
            assert override_track["timing"]["easing"] == "smooth"

            # IN reverses the 2D angular direction exactly; non-z axes are outside the
            # supported 2D subset and must fail instead of being projected silently.
            in_scene = Scene()
            in_square = Square()
            in_scene.play(Rotate(in_square, angle=PI / 2, axis=IN), rate_func=linear)
            in_track = next(
                track
                for track in in_scene.to_document()["tracks"]
                if track["property"] == "rotation"
            )
            assert abs(in_track["values"]["scalar"]["to"] + PI / 2) < 1e-12

            axis_scene = Scene()
            try:
                axis_scene.play(Rotate(Square(), axis=(1.0, 0.0, 0.0)))
            except NotImplementedError:
                pass
            else:
                raise AssertionError("non-z Rotate axis should be rejected")
            assert axis_scene.time == 0.0
            assert axis_scene.to_document()["objects"] == []

            # External pivots and offset-local geometry require a circular translation
            # path that the scalar rotation channel cannot encode yet. Both cases are
            # rejected atomically rather than approximated with linear translation.
            pivot_scene = Scene()
            pivot_square = Square().shift(2 * RIGHT)
            try:
                pivot_scene.play(Rotate(pivot_square, about_point=ORIGIN))
            except NotImplementedError:
                pass
            else:
                raise AssertionError("external Rotate pivot should be rejected")
            assert pivot_scene.time == 0.0
            assert pivot_scene.to_document()["objects"] == []
            assert pivot_square._scene is None and pivot_square._object is None

            line_scene = Scene()
            offset_line = Line(ORIGIN, RIGHT)
            try:
                line_scene.play(Rotate(offset_line))
            except NotImplementedError:
                pass
            else:
                raise AssertionError("offset-local Rotate should be rejected")
            assert line_scene.time == 0.0
            assert line_scene.to_document()["objects"] == []

            try:
                Rotate(Square(), path_arc=0.0)
            except NotImplementedError:
                pass
            else:
                raise AssertionError("explicit Rotate path overrides should be rejected")
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
