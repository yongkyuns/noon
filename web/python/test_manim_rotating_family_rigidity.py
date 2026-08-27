import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimRotatingFamilyRigidityTests(unittest.TestCase):
    def test_family_rotation_is_split_before_pointwise_transform_can_collapse(self) -> None:
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
            import _manim_geometry  # noqa: F401
            import _manim_animate  # noqa: F401
            import _manim_rotate
            _manim_rotate.install()
            import _manim_updaters
            _manim_updaters.install()

            from noon import Arrow, Circle, Line, ORIGIN, PI, RIGHT, Rotating, Scene, VGroup

            scene = Scene()
            arrow = Arrow(start=ORIGIN, end=RIGHT, buff=0)
            scene.add(arrow)
            pivot = arrow.get_start()
            scene.play(Rotating(arrow, PI, about_point=pivot, run_time=1.0))
            # RotatingDemo immediately schedules the same segmented animation again.
            # Segment boundaries must be computed absolutely rather than accumulated,
            # or floating-point drift can leave the final Transform microscopically
            # active at the exact start of this second play.
            scene.play(Rotating(arrow, PI, about_point=pivot, run_time=1.0))
            assert scene.time == 2.0

            document = scene.to_document()
            family_tracks = [
                track for track in document["tracks"]
                if track["property"] == "transform"
            ]
            assert len(family_tracks) > 2, "a 180-degree Arrow rotation must be segmented"

            # Every retained Arrow member must hand off exactly at the authored scene
            # boundary. This catches the old repeated-addition drift independently of
            # whether a following animation happens to query that member immediately.
            for object_id in {track["object"] for track in family_tracks}:
                object_tracks = [
                    track for track in family_tracks if track["object"] == object_id
                ]
                final_track = max(
                    object_tracks,
                    key=lambda track: track["timing"]["start_time"],
                )
                timing = final_track["timing"]
                assert timing["start_time"] + timing["duration"] == scene.time

            # The runtime's generic pointwise-rotation Transform interpolator is safe
            # for short arcs, but a single 180-degree interval collapses its scale at
            # the midpoint. Keep every authored interval small enough that the worst
            # midpoint scale loss stays visually negligible.
            for track in family_tracks:
                values = track["values"]["object"]
                start_rotation = float(values["from"]["transform"]["rotation"])
                end_rotation = float(values["to"]["transform"]["rotation"])
                delta = abs(end_rotation - start_rotation)
                assert delta <= math.pi / 36.0 + 1e-9, delta
                assert math.cos(delta / 2.0) > 0.999

            # The public RotatingDemo next rotates a family containing a circle, line,
            # and the Arrow. That path must use the same segmented lowering rather
            # than reintroducing a single collapse-prone transform per leaf.
            group_scene = Scene()
            circle = Circle(radius=1)
            line = Line(start=ORIGIN, end=RIGHT)
            grouped_arrow = Arrow(start=ORIGIN, end=RIGHT, buff=0)
            family = VGroup(circle, line, grouped_arrow)
            group_scene.add(family)
            group_scene.play(Rotating(family, PI, about_point=RIGHT, run_time=1.0))
            group_document = group_scene.to_document()
            group_tracks = [
                track for track in group_document["tracks"]
                if track["property"] == "transform"
            ]
            assert len(group_tracks) > 4
            for track in group_tracks:
                values = track["values"]["object"]
                start_rotation = float(values["from"]["transform"]["rotation"])
                end_rotation = float(values["to"]["transform"]["rotation"])
                assert abs(end_rotation - start_rotation) <= math.pi / 36.0 + 1e-9
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
