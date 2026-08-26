import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSpinInFromNothingTests(unittest.TestCase):
    def test_spiral_path_lowers_to_centered_scale_and_rotation_transform(self) -> None:
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

            from noon import (
                BLUE,
                PI,
                RIGHT,
                WHITE,
                YELLOW,
                Scene,
                SpinInFromNothing,
                Square,
                VGroup,
            )

            # The exact Manim spiral path for a collapsed centered source is
            # equivalent to scale 0->1 while rotation advances by ``angle``.
            scene = Scene()
            square = Square(
                side_length=1.4,
                fill_color=BLUE,
                fill_opacity=0.8,
                stroke_color=WHITE,
                stroke_opacity=0.6,
                stroke_width=5,
            ).rotate(PI / 6).shift(1.75 * RIGHT)
            scene.play(SpinInFromNothing(square))
            assert abs(scene.time - 1.0) < 1e-12

            tracks = [
                track for track in scene.to_document()["tracks"]
                if track["object"] == square.id
            ]
            transforms = [track for track in tracks if track["property"] == "transform"]
            presence = [track for track in tracks if track["property"] == "presence"]
            assert len(transforms) == 1
            assert len(presence) == 1
            transform = transforms[0]
            assert transform["timing"] == {
                "start_time": 0.0,
                "duration": 1.0,
                "easing": "smooth",
            }
            start = transform["values"]["object"]["from"]
            final = transform["values"]["object"]["to"]
            assert start["transform"]["scale"] == {"x": 0.0, "y": 0.0}
            assert start["transform"]["translation"] == final["transform"]["translation"]
            assert abs(final["transform"]["rotation"] - PI / 6) < 1e-12
            assert abs(start["transform"]["rotation"] - (PI / 6 - PI / 2)) < 1e-12

            # Custom angle, point color, and runtime use the same retained path.
            override_scene = Scene()
            override_square = Square(
                fill_color=BLUE,
                fill_opacity=0.8,
                stroke_color=WHITE,
                stroke_opacity=0.6,
                stroke_width=5,
            ).rotate(-PI / 5)
            override_scene.play(
                SpinInFromNothing(
                    override_square,
                    angle=PI,
                    point_color=YELLOW,
                    run_time=2.5,
                )
            )
            assert abs(override_scene.time - 2.5) < 1e-12
            override_track = next(
                track
                for track in override_scene.to_document()["tracks"]
                if track["object"] == override_square.id and track["property"] == "transform"
            )
            override_start = override_track["values"]["object"]["from"]
            override_final = override_track["values"]["object"]["to"]
            assert abs(
                override_start["transform"]["rotation"]
                - (override_final["transform"]["rotation"] - PI)
            ) < 1e-12
            assert abs(override_start["style"]["fill"]["red"] - YELLOW.red) < 1e-12
            assert abs(override_start["style"]["fill"]["green"] - YELLOW.green) < 1e-12
            assert abs(override_start["style"]["fill"]["blue"] - YELLOW.blue) < 1e-12
            assert abs(override_start["style"]["fill"]["alpha"] - 0.8) < 1e-12
            assert abs(override_start["style"]["stroke"]["alpha"] - 0.6) < 1e-12

            # Nested composition reuses the shared time-map hook rather than a
            # Python-owned interpolation loop.
            nested_scene = Scene()
            nested_square = Square().rotate(PI / 7)
            pending = []
            parent_steps = [{"start": 0.25, "duration": 0.5, "rate_func": "smooth"}]
            _manim_growing._composition_play_leaf(
                nested_scene,
                SpinInFromNothing(nested_square, angle=PI / 3),
                start_time=0.5,
                run_time=2.0,
                time_map_steps=parent_steps,
                pending_time_maps=pending,
            )
            assert len(pending) == 1
            assert pending[0][2] == parent_steps
            nested_track = next(
                track
                for track in nested_scene.to_document()["tracks"]
                if track["object"] == nested_square.id and track["property"] == "transform"
            )
            nested_start = nested_track["values"]["object"]["from"]
            nested_final = nested_track["values"]["object"]["to"]
            assert abs(
                nested_start["transform"]["rotation"]
                - (nested_final["transform"]["rotation"] - PI / 3)
            ) < 1e-12

            try:
                SpinInFromNothing(Square(), angle=float("inf"))
            except ValueError:
                pass
            else:
                raise AssertionError("non-finite spin angles must be rejected")

            try:
                SpinInFromNothing(VGroup(Square()))
            except NotImplementedError:
                pass
            else:
                raise AssertionError("retained groups must remain explicitly partial")
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
