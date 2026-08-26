import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimRestoreAnimationTests(unittest.TestCase):
    def test_restore_resolves_latest_saved_state_at_play_time(self) -> None:
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

            from noon import BLUE, GREEN, RIGHT, UP, Restore, Scene, Square, VGroup, linear

            # Manim constructs Restore without touching saved_state; the missing-state
            # error belongs to Transform.begin()/Scene.play. Rollback must undo binding.
            missing = Square()
            pending_missing = Restore(missing)
            missing_scene = Scene()
            before = missing_scene.to_document()
            try:
                missing_scene.play(pending_missing)
                raise AssertionError("Restore without save_state() must fail at play time")
            except Exception as error:
                assert str(error) == "Trying to restore without having saved"
            assert missing_scene.to_document() == before
            assert abs(missing_scene.time) < 1e-12
            assert missing._scene is None
            assert missing._object is None

            # The target must be resolved at play time. The second save_state() below
            # happens after Restore(...) construction and therefore supersedes the first.
            square = Square(fill_color=BLUE, fill_opacity=0.4)
            square.save_state()
            square.shift(RIGHT).set_fill(color=GREEN, opacity=0.6)
            pending = Restore(square, run_time=2.0, rate_func=linear)
            square.save_state()
            square.shift(UP).scale(1.5)

            scene = Scene()
            scene.add(square)
            scene.play(pending)
            assert abs(scene.time - 2.0) < 1e-12

            tracks = [
                track
                for track in scene.to_document()["tracks"]
                if track["property"] == "transform"
            ]
            assert len(tracks) == 1
            track = tracks[0]
            assert track["timing"] == {
                "start_time": 0.0,
                "duration": 2.0,
                "easing": "linear",
            }

            target = track["values"]["object"]["to"]
            translation = target["transform"]["translation"]
            scale = target["transform"]["scale"]
            fill = target["style"]["fill"]
            assert abs(translation["x"] - 1.0) < 1e-12
            assert abs(translation["y"] - 0.0) < 1e-12
            assert abs(scale["x"] - 1.0) < 1e-12
            assert abs(scale["y"] - 1.0) < 1e-12
            assert abs(fill["red"] - GREEN.red) < 1e-12
            assert abs(fill["green"] - GREEN.green) < 1e-12
            assert abs(fill["blue"] - GREEN.blue) < 1e-12
            assert abs(fill["alpha"] - 0.6) < 1e-12
            assert scene._snapshot_for_object_at(square._object, 2.0) == target

            # Scene.play implicitly adds the animated mobject in Manim. Restore should
            # use the same binding path rather than requiring an explicit Scene.add().
            detached = Square()
            detached.save_state()
            detached.shift(RIGHT)
            detached_scene = Scene()
            detached_scene.play(Restore(detached))
            assert detached._scene is detached_scene
            assert detached._object is not None
            detached_tracks = [
                track
                for track in detached_scene.to_document()["tracks"]
                if track["property"] == "transform"
            ]
            assert len(detached_tracks) == 1
            detached_target = detached_tracks[0]["values"]["object"]["to"]
            assert abs(detached_target["transform"]["translation"]["x"]) < 1e-12
            assert abs(detached_target["transform"]["translation"]["y"]) < 1e-12

            try:
                Restore(VGroup(Square()))
                raise AssertionError("retained-family Restore must stay explicit partial")
            except NotImplementedError:
                pass
            """
        )

        completed = subprocess.run(
            [sys.executable, "-c", source],
            cwd=python_dir,
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)


if __name__ == "__main__":
    unittest.main()
