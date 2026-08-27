import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimRotatingFamilyMotionTests(unittest.TestCase):
    def test_arrow_tip_stays_on_short_rigid_rotation_intervals(self) -> None:
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
            import _manim_geometry
            _manim_geometry.install()
            import _manim_animate  # noqa: F401
            import _manim_rotate
            _manim_rotate.install()
            import _manim_updaters
            _manim_updaters.install()

            from noon import Arrow, ORIGIN, PI, RIGHT, Rotating, Scene

            scene = Scene()
            arrow = Arrow(start=ORIGIN, end=RIGHT, buff=0)
            scene.add(arrow)
            scene.play(
                Rotating(
                    arrow,
                    angle=PI,
                    about_point=arrow.get_start(),
                    run_time=1.0,
                )
            )

            assert abs(scene.time - 1.0) < 1e-12
            document = scene.to_document()
            shaft_id = arrow._shaft.id
            tip_id = arrow._tip.id
            shaft_tracks = [
                track
                for track in document["tracks"]
                if track["object"] == shaft_id and track["property"] == "transform"
            ]
            tip_tracks = [
                track
                for track in document["tracks"]
                if track["object"] == tip_id and track["property"] == "transform"
            ]

            # A half turn is split at a maximum five-degree interval. The old lowering
            # emitted one one-second Transform per leaf, which sent the tip through the
            # diameter while the shaft rotated and visibly detached it.
            assert len(shaft_tracks) == len(tip_tracks) == 36
            assert all(track["timing"]["easing"] == "linear" for track in shaft_tracks)
            assert all(track["timing"]["easing"] == "linear" for track in tip_tracks)
            assert all(
                abs(track["timing"]["duration"] - 1.0 / 36.0) < 1e-12
                for track in shaft_tracks + tip_tracks
            )

            for tracks in (shaft_tracks, tip_tracks):
                for previous, current in zip(tracks, tracks[1:]):
                    assert previous["values"]["object"]["to"] == current["values"]["object"]["from"]

            # The interval endpoints are rigid rotations of the same family. With a
            # five-degree maximum chord, the remaining in-interval radial error is
            # below 1e-3 scene units at unit radius instead of the old O(1) separation.
            import _manim_animate

            for shaft_track, tip_track in zip(shaft_tracks, tip_tracks):
                shaft = _manim_animate._snapshot_mobject(
                    shaft_track["values"]["object"]["to"]
                )
                tip = _manim_animate._snapshot_mobject(
                    tip_track["values"]["object"]["to"]
                )
                shaft_end = shaft.get_end()
                tip_center = tip.get_center()
                gap = math.hypot(
                    shaft_end.x - tip_center.x,
                    shaft_end.y - tip_center.y,
                )
                assert gap < 1e-9, gap
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
