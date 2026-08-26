import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimDrawBorderThenFillTests(unittest.TestCase):
    def test_default_animation_lowers_to_exact_two_phase_timeline(self) -> None:
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
                result.runTime = run_time
                result.intrinsicRunTime = run_time
                result.intervals = []
                return result

            def resolve_composition_schedule(child_run_times_json, lag_ratio, run_time):
                result = Result()
                result.runTime = run_time
                result.intrinsicRunTime = run_time
                result.intervals = []
                return result

            def resolve_lifecycle(intent, binding, has_timeline, present, has_future, at_zero):
                result = Result()
                result.bind = binding == "detached"
                result.showNow = False
                result.hideNow = False
                result.showAtStart = intent == "introduce"
                result.hideAtEnd = False
                if intent == "require_present" and binding == "detached":
                    result.ok = False
                    result.errorKind = "requires_present"
                    result.message = "target must be present"
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
                BLUE,
                ORANGE,
                YELLOW,
                DrawBorderThenFill,
                Scene,
                Square,
                linear,
            )

            scene = Scene()
            square = Square(
                fill_color=ORANGE,
                fill_opacity=1.0,
                stroke_color=BLUE,
                stroke_width=6.0,
            )
            scene.play(DrawBorderThenFill(square))
            assert abs(scene.time - 2.0) < 1e-12
            document = scene.to_document()
            tracks = [track for track in document["tracks"] if track["object"] == square.id]
            transforms = [track for track in tracks if track["property"] == "transform"]
            reveals = [track for track in tracks if track["property"] == "reveal"]
            presence = [track for track in tracks if track["property"] == "presence"]
            assert len(transforms) == 2
            assert len(reveals) == 1
            assert len(presence) == 1

            first, second = transforms
            assert first["timing"] == {
                "start_time": 0.0,
                "duration": 1.0,
                "easing": "smooth",
            }
            assert second["timing"] == {
                "start_time": 1.0,
                "duration": 1.0,
                "easing": "smooth",
            }
            assert reveals[0]["timing"] == {
                "start_time": 0.0,
                "duration": 1.0,
                "easing": "smooth",
            }
            assert reveals[0]["values"]["scalar"] == {"from": 0.0, "to": 1.0}

            outline = first["values"]["object"]["from"]
            assert outline == first["values"]["object"]["to"]
            assert abs(outline["style"]["fill"]["alpha"]) < 1e-12
            assert abs(outline["style"]["stroke_width"] - 0.02) < 1e-12
            assert abs(outline["style"]["stroke"]["red"] - BLUE.red) < 1e-12
            assert abs(outline["style"]["stroke"]["green"] - BLUE.green) < 1e-12
            assert abs(outline["style"]["stroke"]["blue"] - BLUE.blue) < 1e-12

            final = second["values"]["object"]["to"]
            assert abs(final["style"]["fill"]["alpha"] - 1.0) < 1e-12
            assert abs(final["style"]["stroke_width"] - 0.06) < 1e-12

            override_scene = Scene()
            override_square = Square(fill_color=ORANGE, fill_opacity=1.0)
            override_scene.play(
                DrawBorderThenFill(
                    override_square,
                    stroke_width=3.0,
                    stroke_color=YELLOW,
                ),
                run_time=4.0,
            )
            assert abs(override_scene.time - 4.0) < 1e-12
            override_tracks = [
                track
                for track in override_scene.to_document()["tracks"]
                if track["object"] == override_square.id and track["property"] == "transform"
            ]
            assert [track["timing"]["duration"] for track in override_tracks] == [2.0, 2.0]
            override_outline = override_tracks[0]["values"]["object"]["from"]
            assert abs(override_outline["style"]["stroke_width"] - 0.03) < 1e-12
            assert abs(override_outline["style"]["stroke"]["red"] - YELLOW.red) < 1e-12

            try:
                DrawBorderThenFill(Square(), rate_func=linear)
            except NotImplementedError:
                pass
            else:
                raise AssertionError("custom rate_func must remain explicitly partial")

            # A nonlinear parent composition path must preserve the two internal
            # half-phases instead of stretching both emitted tracks across the root.
            nested_scene = Scene()
            nested_square = Square(fill_color=ORANGE, fill_opacity=1.0)
            pending = []
            parent_steps = [{"start": 0.2, "duration": 0.6, "rate_func": "smooth"}]
            _manim_draw_border_then_fill._composition_play_leaf(
                nested_scene,
                DrawBorderThenFill(nested_square),
                start_time=0.0,
                run_time=2.0,
                time_map_steps=parent_steps,
                pending_time_maps=pending,
            )
            assert len(pending) == 3
            phase_steps = [entry[2][-1] for entry in pending]
            assert phase_steps.count({"start": 0.0, "duration": 0.5, "rate_func": "linear"}) == 2
            assert phase_steps.count({"start": 0.5, "duration": 0.5, "rate_func": "linear"}) == 1
            for _, _, steps in pending:
                assert steps[0] == parent_steps[0]
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
