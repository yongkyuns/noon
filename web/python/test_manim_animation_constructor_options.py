import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimAnimationConstructorOptionsTests(unittest.TestCase):
    def test_constructor_options_remain_inert_shared_requests(self) -> None:
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

            fake_js.noonResolveAnimationOptions = resolve_animation_options
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_rate_functions
            _manim_rate_functions.install()
            import _manim_phase_b  # noqa: F401
            from noon import Scene
            play_before = Scene.play
            import _manim_animate
            import _manim_lifecycle
            assert Scene.play is play_before
            assert not hasattr(_manim_animate, "_aligned_scene_play")
            assert not hasattr(_manim_animate, "_expanded_schedule")
            assert not hasattr(_manim_lifecycle, "_compile_with_plan")

            from noon import Circle, Create, FadeIn, Scene, Square, Transform, linear, smooth

            # Constructor options are part of the public animation object, not a
            # Noon-only Scene.play workaround.
            transform = Transform(Square(), Circle(), run_time=1.25, path_arc=0.3)
            assert transform.anim_args == {"run_time": 1.25, "path_arc": 0.3}

            square, circle = Square(), Circle()
            create = Create(square, run_time=2.0, rate_func=linear)
            fade = FadeIn(circle, run_time=0.5)
            assert create.anim_args == {"run_time": 2.0, "rate_func": linear}
            assert fade.anim_args == {"run_time": 0.5}
            assert create.target is square and fade.target is circle
            assert square._scene is None and circle._scene is None
            # Execution is covered by paired timed-composition examples and shared
            # Rust authoring/lowering tests for option precedence.
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
