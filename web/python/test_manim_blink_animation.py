import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimBlinkAnimationTests(unittest.TestCase):
    def test_blink_lowers_to_retained_style_phases(self) -> None:
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
            import json
            import math
            import sys
            import types

            fake_js = types.ModuleType("js")

            class Result:
                ok = True
                errorKind = ""
                message = ""

            class Interval:
                def __init__(self, start, duration):
                    self.startTime = start
                    self.duration = duration
                    self.endTime = start + duration

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

            def resolve_composition_schedule(child_run_times_json, lag_ratio, run_time):
                child_run_times = [float(value) for value in json.loads(child_run_times_json)]
                starts = []
                start = 0.0
                previous = 0.0
                for index, child_run_time in enumerate(child_run_times):
                    if index > 0:
                        start += float(lag_ratio) * previous
                    starts.append(start)
                    previous = child_run_time
                intrinsic = max(
                    (start + duration for start, duration in zip(starts, child_run_times)),
                    default=0.0,
                )
                requested = float(run_time)
                scale = requested / intrinsic if math.isfinite(requested) else 1.0
                result = Result()
                result.intrinsicRunTime = intrinsic
                result.runTime = requested if math.isfinite(requested) else intrinsic
                result.intervals = [
                    Interval(start * scale, duration * scale)
                    for start, duration in zip(starts, child_run_times)
                ]
                return result

            def resolve_uniform_schedule(child_count, lag_ratio, run_time):
                result = Result()
                result.runTime = run_time
                result.intrinsicRunTime = run_time
                if child_count <= 0:
                    result.intervals = []
                    return result
                child_duration = run_time / (1.0 + lag_ratio * (child_count - 1))
                result.intervals = [
                    Interval(index * lag_ratio * child_duration, child_duration)
                    for index in range(child_count)
                ]
                return result

            def resolve_lifecycle(*args):
                result = Result()
                result.bind = False
                result.showNow = False
                result.hideNow = False
                result.showAtStart = False
                result.hideAtEnd = False
                return result

            fake_js.noonResolveAnimationOptions = resolve_animation_options
            fake_js.noonResolveCompositionSchedule = resolve_composition_schedule
            fake_js.noonResolveUniformCompositionSchedule = resolve_uniform_schedule
            fake_js.noonResolveLifecyclePlan = resolve_lifecycle
            fake_js.noonValidatePresenceTransition = resolve_lifecycle
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_rate_functions
            _manim_rate_functions.install()
            import _manim_phase_b  # noqa: F401
            import _manim_animate  # noqa: F401
            import _manim_composition
            _manim_composition.install()
            import _manim_updaters
            _manim_updaters.install()

            from noon import BLUE, WHITE, Blink, Scene, Square, VGroup

            def phase_alphas(track):
                source = track["values"]["object"]["from"]
                target = track["values"]["object"]["to"]
                assert source == target
                return (
                    target["style"]["fill"]["alpha"],
                    target["style"]["stroke"]["alpha"],
                )

            scene = Scene()
            square = Square(
                side_length=1.5,
                fill_color=BLUE,
                fill_opacity=0.35,
                stroke_color=WHITE,
                stroke_opacity=0.65,
                stroke_width=4,
            )
            scene.add(square)
            blink = Blink(square)
            assert blink.time_on == 0.5
            assert blink.time_off == 0.5
            assert blink.blinks == 1
            assert blink.hide_at_end is False
            assert len(blink.animations) == 3

            scene.play(blink)
            assert abs(scene.time - 1.5) < 1e-12
            tracks = [
                track for track in scene.to_document()["tracks"]
                if track["object"] == square.id and track["property"] == "transform"
            ]
            assert len(tracks) == 3
            assert [track["timing"] for track in tracks] == [
                {"start_time": 0.0, "duration": 0.5, "easing": "linear"},
                {"start_time": 0.5, "duration": 0.5, "easing": "linear"},
                {"start_time": 1.0, "duration": 0.5, "easing": "linear"},
            ]
            assert [phase_alphas(track) for track in tracks] == [
                (1.0, 1.0),
                (0.0, 0.0),
                (1.0, 1.0),
            ]
            assert _manim_updaters.register_scene(scene) is None

            # A following retained animation starts from the exact final Blink state.
            scene.play(square.animate.shift((1.0, 0.0)))
            following = [
                track for track in scene.to_document()["tracks"]
                if track["object"] == square.id
                and track["property"] == "transform"
                and abs(track["timing"]["start_time"] - 1.5) < 1e-12
            ]
            assert len(following) == 1
            following_source = following[0]["values"]["object"]["from"]
            assert following_source["style"]["fill"]["alpha"] == 1.0
            assert following_source["style"]["stroke"]["alpha"] == 1.0

            hidden_scene = Scene()
            hidden = Square(
                fill_color=BLUE,
                fill_opacity=1.0,
                stroke_color=WHITE,
                stroke_opacity=1.0,
            )
            hidden_scene.add(hidden)
            hidden_scene.play(
                Blink(
                    hidden,
                    time_on=0.25,
                    time_off=0.75,
                    blinks=2,
                    hide_at_end=True,
                )
            )
            assert abs(hidden_scene.time - 2.0) < 1e-12
            hidden_tracks = [
                track for track in hidden_scene.to_document()["tracks"]
                if track["object"] == hidden.id and track["property"] == "transform"
            ]
            assert len(hidden_tracks) == 4
            assert [track["timing"]["start_time"] for track in hidden_tracks] == [
                0.0,
                0.25,
                1.0,
                1.25,
            ]
            assert [track["timing"]["duration"] for track in hidden_tracks] == [
                0.25,
                0.75,
                0.25,
                0.75,
            ]
            assert phase_alphas(hidden_tracks[-1]) == (0.0, 0.0)

            # Scene.play(run_time=...) scales the complete Succession exactly.
            scaled_scene = Scene()
            scaled = Square(fill_color=BLUE, fill_opacity=1.0, stroke_opacity=0.0)
            scaled_scene.add(scaled)
            scaled_scene.play(
                Blink(scaled, time_on=0.25, time_off=0.75),
                run_time=2.5,
            )
            assert abs(scaled_scene.time - 2.5) < 1e-12
            scaled_tracks = [
                track for track in scaled_scene.to_document()["tracks"]
                if track["object"] == scaled.id and track["property"] == "transform"
            ]
            # Intrinsic runtime is 1.25, so the play override scales every phase by 2.
            assert [track["timing"]["start_time"] for track in scaled_tracks] == [
                0.0,
                0.5,
                2.0,
            ]
            assert [track["timing"]["duration"] for track in scaled_tracks] == [
                0.5,
                1.5,
                0.5,
            ]

            try:
                Blink(VGroup(Square(), Square()))
            except NotImplementedError:
                pass
            else:
                raise AssertionError("Blink retained groups must remain explicitly partial")

            for bad_time in (0.0, -1.0, float("inf")):
                try:
                    Blink(Square(), time_on=bad_time)
                except ValueError:
                    pass
                else:
                    raise AssertionError("Blink must reject invalid phase durations")

            try:
                Blink(Square(), blinks=0)
            except NotImplementedError:
                pass
            else:
                raise AssertionError("zero-blink edge semantics remain outside the retained subset")

            try:
                Blink(Square(), lag_ratio=0.5)
            except NotImplementedError:
                pass
            else:
                raise AssertionError("overlapping Blink phases must not be approximated")
            """
        )

        completed = subprocess.run(
            [sys.executable, "-c", source],
            cwd=python_dir,
            env=env,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(
            completed.returncode,
            0,
            msg=f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
