import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSubsetDisplayTests(unittest.TestCase):
    def test_subset_display_uses_exact_retained_step_thresholds(self) -> None:
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
                if result.reverseRateFunction:
                    result.ok = False
                    result.errorKind = "unsupported"
                    result.message = "reverse unsupported"
                return result

            def resolve_uniform_schedule(child_count, lag_ratio, run_time):
                result = Result()
                result.runTime = run_time
                result.intrinsicRunTime = run_time
                result.intervals = []
                return result

            def resolve_composition_schedule(child_run_times_json, lag_ratio, run_time):
                child_run_times = json.loads(child_run_times_json)
                result = Result()
                intrinsic = max(child_run_times) if child_run_times else 0.0
                actual = run_time if math.isfinite(run_time) else intrinsic
                result.runTime = actual
                result.intrinsicRunTime = intrinsic
                scale = 1.0 if intrinsic == 0 else actual / intrinsic
                result.intervals = [
                    types.SimpleNamespace(
                        startTime=0.0,
                        duration=float(child_run_times[0]) * scale,
                        endTime=float(child_run_times[0]) * scale,
                    )
                ] if child_run_times else []
                return result

            def resolve_lifecycle(intent, binding, has_timeline, present, has_future, at_zero):
                result = Result()
                result.bind = binding == "detached"
                result.showNow = False
                result.hideNow = False
                result.showAtStart = intent == "introduce"
                result.hideAtEnd = False
                if binding == "other_scene":
                    result.ok = False
                    result.message = "wrong scene"
                return result

            def validate_presence(*args):
                result = Result()
                result.bind = False
                result.showNow = False
                result.hideNow = False
                result.showAtStart = False
                result.hideAtEnd = False
                return result

            fake_js.noonResolveAnimationOptions = resolve_animation_options
            fake_js.noonResolveUniformCompositionSchedule = resolve_uniform_schedule
            fake_js.noonResolveCompositionSchedule = resolve_composition_schedule
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
            import _manim_growing
            _manim_growing.install()
            import _manim_draw_border_then_fill
            _manim_draw_border_then_fill.install()

            from noon import (
                AnimationGroup,
                BLUE,
                GREEN,
                RED,
                WHITE,
                Scene,
                ShowIncreasingSubsets,
                ShowSubmobjectsOneByOne,
                Square,
                VGroup,
                linear,
                there_and_back,
            )

            def square(color):
                return Square(
                    side_length=0.8,
                    fill_color=color,
                    fill_opacity=1.0,
                    stroke_color=WHITE,
                    stroke_opacity=1.0,
                    stroke_width=2.0,
                )

            # floor(smooth(alpha) * 2): first member switches exactly at alpha=0.5,
            # second member switches exactly at alpha=1.0. Both discontinuities are
            # ordinary retained opacity tracks, not Python callbacks or presence hacks.
            increasing_scene = Scene()
            left = square(RED)
            right = square(BLUE)
            increasing = ShowIncreasingSubsets(VGroup(left, right))
            assert left.style["opacity"] == 0.0
            assert right.style["opacity"] == 0.0
            increasing_scene.play(increasing)
            assert abs(increasing_scene.time - 1.0) < 1e-12
            tracks = [
                track
                for track in increasing_scene.to_document()["tracks"]
                if track["property"] == "opacity"
            ]
            assert len(tracks) == 2
            by_object = {track["object"]: track for track in tracks}
            assert by_object[left.id]["timing"] == {
                "start_time": 0.0,
                "duration": 0.5,
                "easing": "step_end",
            }
            assert by_object[right.id]["timing"] == {
                "start_time": 0.0,
                "duration": 1.0,
                "easing": "step_end",
            }
            assert increasing_scene._snapshot_for_object_at(left._object, 0.499)["style"]["opacity"] == 0.0
            assert increasing_scene._snapshot_for_object_at(left._object, 0.5)["style"]["opacity"] == 1.0
            assert increasing_scene._snapshot_for_object_at(right._object, 0.999)["style"]["opacity"] == 0.0
            assert increasing_scene._snapshot_for_object_at(right._object, 1.0)["style"]["opacity"] == 1.0

            # ceil(linear(alpha) * 2): at the exact half-way boundary the first
            # member remains visible and the second remains hidden; only immediately
            # after the boundary do they swap. step_start preserves that left-open edge.
            one_scene = Scene()
            first = square(GREEN)
            second = square(BLUE)
            one_scene.play(
                ShowSubmobjectsOneByOne(VGroup(first, second), rate_func=linear)
            )
            one_tracks = [
                track
                for track in one_scene.to_document()["tracks"]
                if track["property"] == "opacity"
            ]
            assert len(one_tracks) == 3
            first_tracks = [track for track in one_tracks if track["object"] == first.id]
            second_tracks = [track for track in one_tracks if track["object"] == second.id]
            assert [track["timing"]["easing"] for track in first_tracks] == [
                "step_start",
                "step_start",
            ]
            assert len(second_tracks) == 1
            assert second_tracks[0]["timing"]["easing"] == "step_start"
            assert one_scene._snapshot_for_object_at(first._object, 0.0)["style"]["opacity"] == 0.0
            assert one_scene._snapshot_for_object_at(second._object, 0.0)["style"]["opacity"] == 0.0
            assert one_scene._snapshot_for_object_at(first._object, 0.25)["style"]["opacity"] == 1.0
            assert one_scene._snapshot_for_object_at(second._object, 0.25)["style"]["opacity"] == 0.0
            assert one_scene._snapshot_for_object_at(first._object, 0.5)["style"]["opacity"] == 1.0
            assert one_scene._snapshot_for_object_at(second._object, 0.5)["style"]["opacity"] == 0.0
            assert one_scene._snapshot_for_object_at(first._object, 0.500001)["style"]["opacity"] == 0.0
            assert one_scene._snapshot_for_object_at(second._object, 0.500001)["style"]["opacity"] == 1.0
            assert one_scene._snapshot_for_object_at(first._object, 1.0)["style"]["opacity"] == 0.0
            assert one_scene._snapshot_for_object_at(second._object, 1.0)["style"]["opacity"] == 1.0

            # Nested composition uses the same retained tracks and normal composition
            # time-map infrastructure rather than falling back to a frame callback.
            nested_scene = Scene()
            nested_left = square(RED)
            nested_right = square(GREEN)
            nested_scene.play(
                AnimationGroup(
                    ShowIncreasingSubsets(
                        VGroup(nested_left, nested_right), rate_func=linear
                    ),
                    run_time=2.0,
                    rate_func=linear,
                )
            )
            assert abs(nested_scene.time - 2.0) < 1e-12
            nested_tracks = [
                track
                for track in nested_scene.to_document()["tracks"]
                if track["property"] == "opacity"
            ]
            assert len(nested_tracks) == 2
            assert max(
                track["timing"]["start_time"] + track["timing"]["duration"]
                for track in nested_tracks
            ) == 2.0

            # Nonmonotonic timing cannot be represented by one ordered threshold pass
            # and must fail atomically rather than approximate Manim.
            rejected_scene = Scene()
            rejected_a = square(RED)
            rejected_b = square(BLUE)
            rejected = ShowIncreasingSubsets(
                VGroup(rejected_a, rejected_b), rate_func=there_and_back
            )
            try:
                rejected_scene.play(rejected)
            except NotImplementedError:
                pass
            else:
                raise AssertionError("there_and_back subset display should be rejected")
            assert rejected_scene.time == 0.0
            assert rejected_scene.to_document()["objects"] == []
            assert rejected_a._scene is None and rejected_a._object is None
            assert rejected_b._scene is None and rejected_b._object is None

            # Mixed-opacity members would make global opacity differ from Manim's
            # destructive set_opacity(0/1), so reject that breadth explicitly.
            try:
                ShowIncreasingSubsets(
                    VGroup(
                        Square(fill_color=RED, fill_opacity=0.5),
                        square(BLUE),
                    )
                )
            except NotImplementedError:
                pass
            else:
                raise AssertionError("mixed-opacity subset members should be rejected")
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
