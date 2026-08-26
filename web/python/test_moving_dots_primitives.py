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

            # ManimCE v0.21 MovingDots relies on match_points replacing geometry and
            # placement without copying the temporary Line's default style.
            source_line = manim.Line((-1.0, 0.0), (1.0, 0.0)).set_color(api.RED)
            target_line = manim.Line((2.0, 3.0), (4.0, 5.0))
            source_line.match_points(target_line)
            assert source_line.geometry == target_line.geometry
            assert source_line.transform == target_line.transform
            assert source_line.style["stroke"] == api.RED.to_ir()

            # The arbitrary-updater bridge returns one atomic geometry patch batch.
            callback_scene = manim.Scene()
            line = manim.Line((-1.0, 0.0), (1.0, 0.0)).set_color(api.RED)
            callback_scene.add(line)
            line.add_updater(
                lambda mob: mob.match_points(manim.Line((0.0, 0.0), (2.0, 1.0)))
            )
            registration = updaters.register_scene(callback_scene)
            assert registration is not None
            frame = {
                "time": 0.5,
                "delta_time": 0.5,
                "signals": [],
                "objects": [
                    {
                        "object": line.id,
                        "transform": line.transform,
                        "style": line.style,
                        "presence": True,
                        "appearance": 1.0,
                        "reveal": 1.0,
                        "morph": 0.0,
                    }
                ],
                "invocations": [{"callback": 0, "object_indices": [0]}],
            }
            batch = json.loads(
                updaters.run_callback_phase(registration["session_id"], frame, 0)
            )
            assert "set_geometry" in batch["patches"][0]
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
