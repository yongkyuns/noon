import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimFocusOnAnimationTests(unittest.TestCase):
    def test_focus_on_lowers_to_temporary_transform_and_removal(self) -> None:
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

            import noon

            # Production installs this validated primitive from _manim_lifecycle after
            # _manim_rotate. The focused unit test keeps the fake-JS surface small and
            # exercises the same canonical presence track shape directly.
            def add_presence_track(self, obj, from_, to, time, *, key=None):
                self._add_track(
                    obj,
                    "presence",
                    {"bool": {"from": bool(from_), "to": bool(to)}},
                    float(time),
                    0.0,
                    "linear",
                    key,
                )

            noon._ir.Scene._add_presence_track = add_presence_track

            from noon import FocusOn, RIGHT, Scene, Square

            scene = Scene()
            scene.play(FocusOn(RIGHT))
            assert abs(scene.time - 2.0) < 1e-12
            document = scene.to_document()
            assert len(document["objects"]) == 1
            spotlight = document["objects"][0]
            assert "circle" in spotlight["geometry"]
            assert abs(spotlight["geometry"]["circle"]["radius"] - 100.0 / 9.0) < 1e-12
            assert abs(spotlight["style"]["fill"]["alpha"]) < 1e-12
            assert abs(spotlight["style"]["stroke_width"]) < 1e-12

            transforms = [track for track in document["tracks"] if track["property"] == "transform"]
            presence = [track for track in document["tracks"] if track["property"] == "presence"]
            assert len(transforms) == 1
            assert len(presence) == 1
            transform = transforms[0]
            assert abs(transform["timing"]["start_time"]) < 1e-12
            assert abs(transform["timing"]["duration"] - 2.0) < 1e-12
            assert transform["timing"]["easing"] == "smooth"
            target = transform["values"]["object"]["to"]
            assert abs(target["transform"]["translation"]["x"] - 1.0) < 1e-12
            assert abs(target["transform"]["translation"]["y"]) < 1e-12
            assert abs(target["transform"]["scale"]["x"]) < 1e-12
            assert abs(target["transform"]["scale"]["y"]) < 1e-12
            assert abs(target["style"]["fill"]["alpha"] - 0.2) < 1e-12

            removal = presence[0]
            assert abs(removal["timing"]["start_time"] - 2.0) < 1e-12
            assert removal["timing"]["duration"] == 0.0
            assert removal["values"]["bool"] == {"from": True, "to": False}

            # Scene.play overrides the constructor default without changing lifecycle.
            override = Scene()
            override.play(FocusOn(2 * RIGHT, run_time=3.0), run_time=0.75)
            override_doc = override.to_document()
            override_transform = next(
                track for track in override_doc["tracks"] if track["property"] == "transform"
            )
            override_removal = next(
                track for track in override_doc["tracks"] if track["property"] == "presence"
            )
            assert abs(override.time - 0.75) < 1e-12
            assert abs(override_transform["timing"]["duration"] - 0.75) < 1e-12
            assert abs(override_removal["timing"]["start_time"] - 0.75) < 1e-12

            # A static leaf mobject is accepted by construction-time center, while
            # ambiguous mixed top-level composition stays explicitly unsupported.
            static_target = Square().shift(2 * RIGHT)
            static_scene = Scene()
            static_scene.play(FocusOn(static_target))
            static_transform = next(
                track
                for track in static_scene.to_document()["tracks"]
                if track["property"] == "transform"
            )
            static_endpoint = static_transform["values"]["object"]["to"]
            assert abs(static_endpoint["transform"]["translation"]["x"] - 2.0) < 1e-12

            rejected = Scene()
            focus = FocusOn(RIGHT)
            try:
                rejected.play(focus, Square().animate.shift(RIGHT))
            except NotImplementedError:
                pass
            else:
                raise AssertionError("mixed FocusOn composition should be rejected")
            assert rejected.time == 0.0
            assert rejected.to_document()["objects"] == []
            assert focus.mobject._scene is None and focus.mobject._object is None
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
