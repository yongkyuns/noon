import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimFadeEndpointTests(unittest.TestCase):
    def test_shift_scale_and_target_position_lower_to_transform_tracks(self) -> None:
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

            from noon import FadeIn, FadeOut, RIGHT, Scene, Square, UP

            class CenterCountingSquare(Square):
                def __init__(self):
                    super().__init__()
                    self.center_reads = 0

                def get_center(self):
                    self.center_reads += 1
                    return super().get_center()

            # Canonical target_position records the absolute point without
            # evaluating the fade target's center in Python. Mobject coercion
            # still resolves the referenced object's center at construction.
            canonical_target = CenterCountingSquare()
            point_reference = CenterCountingSquare().shift(RIGHT * 3.0)
            canonical_target._semantic_handle = object()
            canonical_target.center_reads = 0
            point_reference.center_reads = 0
            point_fade = FadeIn(canonical_target, target_position=point_reference)
            assert canonical_target.center_reads == 0
            assert point_reference.center_reads == 1
            assert point_fade._fade_point == RIGHT * 3.0

            def tracks(scene, object_id, property_name):
                return [
                    track
                    for track in scene.to_document()["tracks"]
                    if track["object"] == object_id and track["property"] == property_name
                ]

            # Manim FadeIn(..., shift=v) creates the faded copy at -v and scales
            # that copy around its center before interpolating to the authored state.
            fade_in_scene = Scene()
            entering = Square().shift(RIGHT * 2.0)
            fade_in_scene.play(
                FadeIn(entering, shift=UP, scale=0.5, run_time=2.0)
            )
            transform = tracks(fade_in_scene, entering.id, "transform")[0]
            appearance = tracks(fade_in_scene, entering.id, "appearance")[0]
            start = transform["values"]["object"]["from"]["transform"]
            end = transform["values"]["object"]["to"]["transform"]
            assert start["translation"] == {"x": 2.0, "y": -1.0}
            assert start["scale"] == {"x": 0.5, "y": 0.5}
            assert end["translation"] == {"x": 2.0, "y": 0.0}
            assert end["scale"] == {"x": 1.0, "y": 1.0}
            assert transform["timing"] == appearance["timing"]
            assert abs(fade_in_scene.time - 2.0) < 1e-12

            # FadeOut uses the opposite endpoint direction: +shift and the requested
            # scale are the disappearing target state.
            fade_out_scene = Scene()
            leaving = Square()
            fade_out_scene.add(leaving)
            fade_out_scene.play(
                FadeOut(leaving, shift=RIGHT * 2.0, scale=1.5, run_time=0.8)
            )
            transform = tracks(fade_out_scene, leaving.id, "transform")[0]
            appearance = tracks(fade_out_scene, leaving.id, "appearance")[0]
            start = transform["values"]["object"]["from"]["transform"]
            end = transform["values"]["object"]["to"]["transform"]
            assert start["translation"] == {"x": 0.0, "y": 0.0}
            assert start["scale"] == {"x": 1.0, "y": 1.0}
            assert end["translation"] == {"x": 2.0, "y": 0.0}
            assert end["scale"] == {"x": 1.5, "y": 1.5}
            assert transform["timing"] == appearance["timing"]

            # target_position is resolved at animation construction and is not
            # direction-reversed for FadeIn: the faded copy begins at that point.
            point_scene = Scene()
            point_entering = Square()
            point_scene.play(
                FadeIn(point_entering, target_position=RIGHT * 3.0, run_time=1.25)
            )
            transform = tracks(point_scene, point_entering.id, "transform")[0]
            start = transform["values"]["object"]["from"]["transform"]
            end = transform["values"]["object"]["to"]["transform"]
            assert start["translation"] == {"x": 3.0, "y": 0.0}
            assert end["translation"] == {"x": 0.0, "y": 0.0}

            # As in Manim's _Fade.__init__, explicit shift wins when both are given.
            precedence_scene = Scene()
            precedence = Square()
            precedence_scene.play(
                FadeIn(
                    precedence,
                    shift=UP,
                    target_position=RIGHT * 8.0,
                    run_time=0.5,
                )
            )
            transform = tracks(precedence_scene, precedence.id, "transform")[0]
            start = transform["values"]["object"]["from"]["transform"]
            assert start["translation"] == {"x": 0.0, "y": -1.0}
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
