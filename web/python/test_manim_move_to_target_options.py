import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimMoveToTargetOptionTests(unittest.TestCase):
    def test_wrapper_forwards_transform_options_unchanged(self) -> None:
        python_dir = Path(__file__).resolve().parent
        repo_root = python_dir.parent.parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            part for part in (str(python_dir), env.get("PYTHONPATH", "")) if part
        )
        source = textwrap.dedent(
            """
            import sys
            import types

            js = types.ModuleType("js")
            js.noonResolveAnimationOptions = lambda *args: None
            sys.modules["js"] = js

            import _manim_compat
            _manim_compat.install()
            import _manim_animation_options as options
            from noon import Circle, RIGHT

            calls = []
            animate = types.ModuleType("_manim_animate")
            def transform(source, target, **kwargs):
                calls.append((source, target, kwargs))
                return "delegated-transform"
            animate.Transform = transform
            sys.modules["_manim_animate"] = animate

            circle = Circle()
            circle.generate_target()
            circle.target.shift(RIGHT)
            rate_func = object()
            result = options.MoveToTarget(
                circle,
                run_time=2.5,
                rate_func=rate_func,
                path_arc=0.75,
                key="move-target",
            )

            assert result == "delegated-transform"
            assert len(calls) == 1
            source, target, kwargs = calls[0]
            assert source is circle
            assert target is circle.target
            assert kwargs == {
                "run_time": 2.5,
                "rate_func": rate_func,
                "path_arc": 0.75,
                "key": "move-target",
            }
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source],
            cwd=repo_root,
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)


if __name__ == "__main__":
    unittest.main()
