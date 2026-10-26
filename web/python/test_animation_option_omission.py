"""Only argument conversion is mocked; Rust owns timing validation."""
import os
from pathlib import Path
import subprocess
import sys
import textwrap
import unittest


class AnimationOptionOmissionTests(unittest.TestCase):
    def test_optional_numbers_preserve_explicit_nonfinite_values(self):
        python_dir = Path(__file__).resolve().parent
        source = textwrap.dedent(
            """
            import math
            import sys
            from types import ModuleType, SimpleNamespace

            calls = []
            def resolver(*args):
                calls.append(args)
                return SimpleNamespace(runTime=1.0, rateFunc="smooth", lagRatio=0.0,
                    pathArc=0.0, reverseRateFunction=False)

            bridge = ModuleType("js")
            bridge.noonResolveAnimationOptions = resolver
            bridge.noonResolveCreateAnimationOptions = resolver
            bridge.noonResolveGlowPulseAnimationOptions = resolver
            bridge.noonResolveTransformAnimationOptions = resolver
            sys.modules["js"] = bridge
            import _manim_animation_options as options

            common = dict(default_lag_ratio=0.0, play_run_time=None,
                play_easing=None, play_rate_func=None, play_lag_ratio=None)
            options.resolve(builder_args={}, **common)
            assert all(calls[-1][index] is None for index in (1, 3, 4, 6, 8))

            for value in (float("nan"), float("inf"), -float("inf"), 0.0, -0.5):
                options.resolve(builder_args={"run_time": value}, **common)
                assert calls[-1][1] == value or math.isnan(calls[-1][1]) and math.isnan(value)
                assert calls[-1][6] is None
                options.resolve(builder_args={}, **{**common, "play_run_time": value})
                assert calls[-1][6] == value or math.isnan(calls[-1][6]) and math.isnan(value)
                assert calls[-1][1] is None

            options.resolve_transform(builder_args={"path_arc": float("nan")},
                play_path_arc=None, **common)
            assert math.isnan(calls[-1][4]) and calls[-1][9] is None
            options.resolve_transform(builder_args={}, play_path_arc=float("nan"), **common)
            assert calls[-1][4] is None and math.isnan(calls[-1][9])

            options.resolve(builder_args={"reverse_rate_function": True,
                "run_time": float("nan")}, create=True, **common)
            assert math.isnan(calls[-1][1]) and calls[-1][6] is None
            options.resolve(builder_args={"reverse_rate_function": True,
                "run_time": float("nan")}, glow_pulse=True, **common)
            assert math.isnan(calls[-1][1]) and calls[-1][5] == 1 and calls[-1][6] is None
            """
        )
        env = dict(os.environ)
        env["PYTHONPATH"] = os.pathsep.join(filter(None, (str(python_dir), env.get("PYTHONPATH"))))
        result = subprocess.run(
            [sys.executable, "-c", source], cwd=python_dir, env=env,
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
