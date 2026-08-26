import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimShowPassingFlashTests(unittest.TestCase):
    def test_line_window_is_retained_and_cleanup_restores_source(self) -> None:
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
                if intent in {"require_present", "require_available_target"}:
                    return result
                if intent == "introduce":
                    if binding == "detached":
                        result.bind = True
                    result.showAtStart = True
                    return result
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
            import _manim_lifecycle  # installs itself
            import _manim_indication
            _manim_indication.install()
            import _manim_updaters
            _manim_updaters.install()

            from noon import AnimationGroup, Line, Scene, ShowPassingFlash, Square

            def mapped_step(track):
                steps = track["time_map"]["steps"]
                assert len(steps) >= 1
                return steps[-1]

            scene = Scene()
            line = Line((-2.0, 0.0), (2.0, 0.0)).shift((1.0, 2.0))
            scene.add(line)
            flash = ShowPassingFlash(line)
            assert flash.time_width == 0.1
            assert flash.remover is True
            assert flash.introducer is True

            scene.play(flash)
            assert abs(scene.time - 1.0) < 1e-12
            document = scene.to_document()
            tracks = [track for track in document["tracks"] if track["object"] == line.id]
            reveals = [track for track in tracks if track["property"] == "reveal"]
            positions = [track for track in tracks if track["property"] == "position"]
            presence = [track for track in tracks if track["property"] == "presence"]

            # Two mapped width segments plus an exact-end full-reveal cleanup hold.
            assert len(reveals) == 3
            width_in, width_out, reveal_cleanup = reveals
            assert width_in["values"]["scalar"] == {"from": 0.0, "to": 0.1}
            assert width_out["values"]["scalar"] == {"from": 0.1, "to": 0.0}
            first = mapped_step(width_in)
            last = mapped_step(width_out)
            assert first["rate_func"] == "smooth"
            assert last["rate_func"] == "smooth"
            assert abs(first["start"] - 0.0) < 1e-12
            assert abs(first["duration"] - (0.1 / 1.1)) < 1e-12
            assert abs(last["start"] - (1.0 / 1.1)) < 1e-12
            assert abs(last["duration"] - (0.1 / 1.1)) < 1e-12
            assert reveal_cleanup["timing"]["start_time"] == 1.0
            assert reveal_cleanup["values"]["scalar"] == {"from": 1.0, "to": 1.0}

            # lower(alpha) is represented by translating exactly one transformed
            # source-line vector over the mapped lower-bound interval.
            assert len(positions) == 2
            lower, position_cleanup = positions
            lower_step = mapped_step(lower)
            assert abs(lower_step["start"] - (0.1 / 1.1)) < 1e-12
            assert abs(lower_step["duration"] - (1.0 / 1.1)) < 1e-12
            assert lower_step["rate_func"] == "smooth"
            assert lower["values"]["vec2"]["from"] == {"x": 1.0, "y": 2.0}
            assert lower["values"]["vec2"]["to"] == {"x": 5.0, "y": 2.0}
            assert position_cleanup["timing"]["start_time"] == 1.0
            assert position_cleanup["values"]["vec2"]["from"] == {"x": 1.0, "y": 2.0}
            assert position_cleanup["values"]["vec2"]["to"] == {"x": 1.0, "y": 2.0}

            assert len(presence) == 1
            assert presence[0]["timing"]["start_time"] == 1.0
            assert presence[0]["values"]["bool"] == {"from": True, "to": False}
            assert line not in scene.mobjects
            assert _manim_updaters.register_scene(scene) is None

            # Reusing the removed Line at the exact animation boundary must start
            # from Manim's restored full source, not the translated zero-width tail.
            scene.add(line)
            scene.play(line.animate.shift((1.0, 0.0)))
            assert abs(scene.time - 2.0) < 1e-12
            transforms = [
                track for track in scene.to_document()["tracks"]
                if track["object"] == line.id
                and track["property"] == "transform"
                and abs(track["timing"]["start_time"] - 1.0) < 1e-12
            ]
            assert len(transforms) == 1
            source_snapshot = transforms[0]["values"]["object"]["from"]
            assert source_snapshot["transform"]["translation"] == {"x": 1.0, "y": 2.0}

            # Play-level run_time/rate_func overrides apply before the bound formula.
            scaled_scene = Scene()
            scaled_line = Line((0.0, 0.0), (0.0, 3.0))
            scaled_scene.play(
                ShowPassingFlash(scaled_line, time_width=2.0),
                run_time=4.0,
                rate_func=_manim_rate_functions.linear,
            )
            assert abs(scaled_scene.time - 4.0) < 1e-12
            scaled_reveals = [
                track for track in scaled_scene.to_document()["tracks"]
                if track["object"] == scaled_line.id
                and track["property"] == "reveal"
                and "time_map" in track
            ]
            assert len(scaled_reveals) == 2
            assert scaled_reveals[0]["values"]["scalar"] == {"from": 0.0, "to": 1.0}
            assert abs(mapped_step(scaled_reveals[0])["duration"] - (1.0 / 3.0)) < 1e-12
            assert abs(mapped_step(scaled_reveals[1])["start"] - (2.0 / 3.0)) < 1e-12
            assert mapped_step(scaled_reveals[0])["rate_func"] == "linear"

            # Detached Lines introduced after time zero get an exact start show event.
            delayed_scene = Scene()
            delayed = Line((-1.0, 0.0), (1.0, 0.0))
            delayed_scene.play(ShowPassingFlash(delayed), start_time=0.5)
            delayed_presence = [
                track for track in delayed_scene.to_document()["tracks"]
                if track["object"] == delayed.id and track["property"] == "presence"
            ]
            assert [track["timing"]["start_time"] for track in delayed_presence] == [0.5, 1.5]
            assert [track["values"]["bool"] for track in delayed_presence] == [
                {"from": False, "to": True},
                {"from": True, "to": False},
            ]

            # Linear AnimationGroup composition is supported and carries the parent
            # interval ahead of the local passing-window map.
            group_scene = Scene()
            group_line = Line((-1.0, 0.0), (1.0, 0.0))
            group_scene.play(AnimationGroup(ShowPassingFlash(group_line), run_time=2.0))
            grouped = [
                track for track in group_scene.to_document()["tracks"]
                if track["object"] == group_line.id
                and track["property"] == "reveal"
                and "time_map" in track
            ]
            assert len(grouped) == 2
            assert len(grouped[0]["time_map"]["steps"]) == 2
            assert grouped[0]["time_map"]["steps"][0]["rate_func"] == "linear"
            assert grouped[0]["time_map"]["steps"][1]["rate_func"] == "smooth"

            try:
                ShowPassingFlash(Square())
            except NotImplementedError:
                pass
            else:
                raise AssertionError("general VMobject windows must remain explicitly partial")

            try:
                ShowPassingFlash(Line((0.0, 0.0), (1.0, 0.0)), time_width=0.0)
            except NotImplementedError:
                pass
            else:
                raise AssertionError("zero-width edge semantics remain outside this tranche")

            try:
                ShowPassingFlash(
                    Line((0.0, 0.0), (1.0, 0.0)),
                    reverse_rate_function=True,
                )
            except NotImplementedError:
                pass
            else:
                raise AssertionError("reversed rate semantics must not be approximated")
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
