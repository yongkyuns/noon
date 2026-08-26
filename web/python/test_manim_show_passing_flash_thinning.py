import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimShowPassingFlashThinningTests(unittest.TestCase):
    def test_line_thinning_and_zero_width_endpoint_are_retained(self) -> None:
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
                    (offset + duration for offset, duration in zip(starts, child_run_times)),
                    default=0.0,
                )
                requested = float(run_time)
                scale = requested / intrinsic if math.isfinite(requested) else 1.0
                result = Result()
                result.intrinsicRunTime = intrinsic
                result.runTime = requested if math.isfinite(requested) else intrinsic
                result.intervals = [
                    Interval(offset * scale, duration * scale)
                    for offset, duration in zip(starts, child_run_times)
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

            def resolve_lifecycle(
                intent,
                binding,
                has_presence_timeline,
                present,
                has_future_event,
                at_time_zero,
            ):
                result = Result()
                result.bind = False
                result.showNow = False
                result.hideNow = False
                result.showAtStart = False
                result.hideAtEnd = False
                if binding == "other_scene":
                    result.ok = intent == "remove"
                    if not result.ok:
                        result.errorKind = "other_scene"
                    return result
                if has_future_event:
                    result.ok = False
                    result.errorKind = "future_event"
                    return result
                if intent == "add":
                    if binding == "detached":
                        result.bind = True
                        result.showNow = not at_time_zero
                    elif has_presence_timeline and not present:
                        result.showNow = True
                    return result
                if intent == "remove_after_animation":
                    if binding != "this_scene":
                        result.ok = False
                        result.errorKind = "requires_bound"
                    elif not present:
                        result.ok = False
                        result.errorKind = "requires_present"
                    else:
                        result.hideAtEnd = True
                    return result
                if intent == "introduce":
                    if binding == "detached":
                        result.bind = True
                    result.showAtStart = True
                return result

            def validate_presence(
                has_previous,
                previous_time,
                previous_to,
                time,
                from_,
            ):
                result = Result()
                result.bind = False
                result.showNow = False
                result.hideNow = False
                result.showAtStart = False
                result.hideAtEnd = False
                if has_previous and (time < previous_time or bool(previous_to) != bool(from_)):
                    result.ok = False
                    result.errorKind = "invalid"
                    result.message = "invalid presence transition"
                return result

            fake_js.noonResolveAnimationOptions = resolve_animation_options
            fake_js.noonResolveCompositionSchedule = resolve_composition_schedule
            fake_js.noonResolveUniformCompositionSchedule = resolve_uniform_schedule
            fake_js.noonResolveLifecyclePlan = resolve_lifecycle
            fake_js.noonValidatePresenceTransition = validate_presence
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_rate_functions
            _manim_rate_functions.install()
            import _manim_phase_b  # noqa: F401
            import _manim_animate  # noqa: F401
            import _manim_composition
            _manim_composition.install()
            import _manim_lifecycle  # noqa: F401
            import _manim_indication
            _manim_indication.install()
            import _manim_indication_thinning
            _manim_indication_thinning.install()

            from noon import (
                Line,
                Scene,
                ShowPassingFlash,
                ShowPassingFlashWithThinningStrokeWidth,
                Square,
            )

            line = Line((-2.0, 0.0), (2.0, 0.0), stroke_width=8)
            thinning = ShowPassingFlashWithThinningStrokeWidth(
                line,
                n_segments=5,
                time_width=0.4,
                remover=False,
                run_time=2.0,
            )
            assert thinning.n_segments == 5
            assert thinning.time_width == 0.4
            assert thinning.remover is False
            assert len(thinning.animations) == 5

            expected_time_widths = [0.4, 0.3, 0.2, 0.1, 0.0]
            expected_stroke_widths = [0.0, 2.0, 4.0, 6.0, 8.0]
            for animation, expected_tw, expected_stroke in zip(
                thinning.animations,
                expected_time_widths,
                expected_stroke_widths,
                strict=True,
            ):
                assert isinstance(animation, ShowPassingFlash)
                assert abs(animation.time_width - expected_tw) < 1e-12
                assert abs(animation.anim_args["run_time"] - 2.0) < 1e-12
                stored_width = animation.mobject._current_raw().style["stroke_width"]
                assert abs(
                    stored_width
                    - _manim_phase_b._manim_stroke_width(expected_stroke)
                ) < 1e-12

            scene = Scene()
            scene.play(thinning)
            assert abs(scene.time - 2.0) < 1e-12
            assert len(scene.to_document()["objects"]) == 5
            assert all(animation.mobject not in scene.mobjects for animation in thinning.animations)

            # Upstream's final thinning segment has time_width=0. It is an exact
            # zero-length sliver for the entire interval, then restores and removes.
            last = thinning.animations[-1].mobject
            last_tracks = [
                track
                for track in scene.to_document()["tracks"]
                if track["object"] == last.id
            ]
            last_reveals = [track for track in last_tracks if track["property"] == "reveal"]
            assert len(last_reveals) == 2
            assert last_reveals[0]["values"]["scalar"] == {"from": 0.0, "to": 0.0}
            assert last_reveals[0]["timing"] == {
                "start_time": 0.0,
                "duration": 2.0,
                "easing": "linear",
            }
            assert last_reveals[1]["values"]["scalar"] == {"from": 1.0, "to": 1.0}
            assert last_reveals[1]["timing"]["start_time"] == 2.0
            assert not any(track["property"] == "position" for track in last_tracks)
            last_presence = [track for track in last_tracks if track["property"] == "presence"]
            assert len(last_presence) == 1
            assert last_presence[0]["timing"]["start_time"] == 2.0

            # Zero-width ShowPassingFlash is useful independently and retains the
            # same cleanup/remover lifecycle as the generated endpoint child.
            zero_scene = Scene()
            zero_line = Line((0.0, 0.0), (1.0, 0.0))
            zero_scene.play(ShowPassingFlash(zero_line, time_width=0.0))
            assert abs(zero_scene.time - 1.0) < 1e-12
            assert zero_line not in zero_scene.mobjects

            one = ShowPassingFlashWithThinningStrokeWidth(
                line,
                n_segments=1,
                time_width=0.25,
            )
            assert len(one.animations) == 1
            assert one.animations[0].time_width == 0.25
            assert one.animations[0].mobject._current_raw().style["stroke_width"] == 0.0

            try:
                ShowPassingFlashWithThinningStrokeWidth(line, n_segments=0)
            except ValueError:
                pass
            else:
                raise AssertionError("n_segments=0 must remain outside the qualified subset")

            try:
                ShowPassingFlashWithThinningStrokeWidth(Square())
            except NotImplementedError:
                pass
            else:
                raise AssertionError("general VMobject thinning remains partial")
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
