import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class MovingDotsPrimitiveTests(unittest.TestCase):
    def test_runtime_tracker_match_points_and_geometry_patch_bridge(self) -> None:
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
            import sys
            import types

            fake_js = types.ModuleType("js")
            fake_js.noonResolveAnimationOptions = lambda *args: None
            sys.modules["js"] = fake_js

            import _manim_compat as manim
            import _manim_geometry  # noqa: F401 - installs match_points/layout semantics
            import _manim_reactive as reactive
            import _manim_updaters as updaters
            import noon as api

            updaters.install()

            # ValueTracker reads the evaluated runtime signal only while a coherent
            # callback phase is active, then falls back to its authoring value.
            scene = manim.Scene()
            tracker = reactive.value_tracker(scene, 0.0)
            reactive._enter_callback_signal_values(
                {"signals": [{"signal": tracker.signal_id, "value": {"scalar": 2.25}}]}
            )
            try:
                assert tracker.get_value() == 2.25
            finally:
                reactive._leave_callback_signal_values()
            assert tracker.get_value() == 0.0

            # Raw geometry replacement is no longer a callback compatibility path.
            # The canonical opaque-handle proof lives in test_canonical_line_match.
            source_line = manim.Line((-1.0, 0.0), (1.0, 0.0)).set_color(api.RED)
            target_line = manim.Line((2.0, 3.0), (4.0, 5.0))
            try:
                source_line.match_points(target_line)
            except NotImplementedError as error:
                assert "opaque shared semantic Line handles" in str(error)
            else:
                raise AssertionError("raw Line geometry matching must not remain available")
            """
        )

        completed = subprocess.run(
            [sys.executable, "-c", source],
            cwd=python_dir,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(
            completed.returncode,
            0,
            f"MovingDots primitive subprocess failed:\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
