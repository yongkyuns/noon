import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimMoveToTargetTests(unittest.TestCase):
    def test_docs_example_lowers_to_retained_transform(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            part for part in (str(python_dir), env.get("PYTHONPATH", "")) if part
        )
        source = textwrap.dedent(
            """
            import math, sys, types
            fake_js = types.ModuleType("js")
            class Result:
                ok=True; errorKind=""; message=""
            def resolve_animation_options(default_lag_ratio, animation_run_time, animation_rate_func, animation_lag_ratio, path_arc, reverse_rate_function, play_run_time, play_rate_func, play_lag_ratio):
                r=Result(); r.runTime=play_run_time if math.isfinite(play_run_time) else animation_run_time if math.isfinite(animation_run_time) else 1.0; r.rateFunc=play_rate_func or animation_rate_func or "smooth"; r.lagRatio=play_lag_ratio if math.isfinite(play_lag_ratio) else animation_lag_ratio if math.isfinite(animation_lag_ratio) else default_lag_ratio; r.pathArc=path_arc if math.isfinite(path_arc) else 0.0; r.reverseRateFunction=reverse_rate_function == 1; return r
            def resolve_uniform_schedule(child_count, lag_ratio, run_time):
                r=Result(); r.intervals=[]; return r
            fake_js.noonResolveAnimationOptions=resolve_animation_options
            fake_js.noonResolveUniformCompositionSchedule=resolve_uniform_schedule
            sys.modules["js"]=fake_js
            import _manim_compat; _manim_compat.install()
            import _manim_rate_functions; _manim_rate_functions.install()
            import _manim_phase_b, _manim_animate
            from noon import Circle, GREEN, MoveToTarget, RIGHT, Scene, Transform, UP
            missing=Circle()
            try:
                MoveToTarget(missing)
                raise AssertionError("missing target must fail")
            except ValueError as error:
                assert str(error) == "MoveToTarget called on mobjectwithout attribute 'target'"
            c=Circle(); c.generate_target(); c.target.set_fill(color=GREEN, opacity=0.5); c.target.shift(2*RIGHT + UP).scale(0.5)
            scene=Scene(); scene.add(c); animation=MoveToTarget(c); assert isinstance(animation, Transform); scene.play(animation)
            tracks=[t for t in scene._tracks if t["property"] == "transform"]
            assert len(tracks) == 1 and abs(tracks[0]["timing"]["duration"] - 1.0) < 1e-12
            target=tracks[0]["values"]["object"]["to"]
            assert abs(target["transform"]["translation"]["x"] - 2.0) < 1e-12
            assert abs(target["transform"]["translation"]["y"] - 1.0) < 1e-12
            assert abs(target["transform"]["scale"]["x"] - 0.5) < 1e-12
            assert abs(target["style"]["fill"]["alpha"] - 0.5) < 1e-12
            c2=Circle(); c2.generate_target(); pending=MoveToTarget(c2); c2.target.shift(RIGHT); scene2=Scene(); scene2.add(c2); scene2.play(pending)
            to=[t for t in scene2._tracks if t["property"] == "transform"][0]["values"]["object"]["to"]
            assert abs(to["transform"]["translation"]["x"] - 1.0) < 1e-12
            """
        )
        completed = subprocess.run([sys.executable, "-c", source], cwd=python_dir, env=env, text=True, capture_output=True, check=False)
        self.assertEqual(completed.returncode, 0, completed.stderr)


if __name__ == "__main__":
    unittest.main()
