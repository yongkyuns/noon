import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimUpdaterLifecycleTests(unittest.TestCase):
    def test_add_remove_history_becomes_runtime_activation_windows(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        existing = env.get("PYTHONPATH")
        env["PYTHONPATH"] = str(python_dir) if not existing else os.pathsep.join((str(python_dir), existing))
        source = textwrap.dedent(
            """
            import json
            import math
            import sys
            import types

            fake_js = types.ModuleType("js")
            fake_js.noonResolveUniformCompositionSchedule = lambda *args: None
            fake_js.noonResolveAnimationOptions = lambda *args: None
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_phase_b  # noqa: F401
            import _manim_updaters as updaters
            updaters.install()

            from noon import LEFT, ORIGIN, Line, Scene

            scene = Scene()
            moving = Line(ORIGIN, LEFT)

            def forward(mobject, dt):
                mobject.rotate_about_origin(dt)

            def backward(mobject, dt):
                mobject.rotate_about_origin(-dt)

            moving.add_updater(forward)
            scene.add(moving)
            scene.wait(2)
            moving.remove_updater(forward)
            moving.add_updater(backward)
            scene.wait(2)
            moving.remove_updater(backward)
            scene.wait(0.5)

            assert not moving.has_updaters()
            config = updaters.register_scene(scene)
            assert config is not None
            assert len(config["slots"]) == 2, config
            first, second = config["slots"]
            assert first["active_after"] == 0.0
            assert first["active_through"] == 2.0
            assert second["active_after"] == 2.0
            assert second["active_through"] == 4.0

            session = config["session_id"]
            object_id = moving.id
            base = scene._objects[object_id]
            def frame(time, dt, callback):
                return {
                    "time": time,
                    "delta_time": dt,
                    "objects": [{
                        "object": object_id,
                        "transform": base["transform"],
                        "style": base["style"],
                        "presence": True,
                        "appearance": 1.0,
                        "reveal": 1.0,
                        "morph": 1.0,
                    }],
                    "signals": [],
                    "invocations": [{"callback": callback, "object_indices": [0]}],
                }

            forward_batch = json.loads(updaters.run_callback_phase(session, frame(1.0, 0.25, 0), 0))
            forward_rotation = forward_batch["patches"][0]["set_transform"]["transform"]["rotation"]
            assert abs(forward_rotation - 0.25) < 1e-6, forward_rotation

            backward_batch = json.loads(updaters.run_callback_phase(session, frame(3.0, 0.25, 1), 1))
            backward_rotation = backward_batch["patches"][0]["set_transform"]["transform"]["rotation"]
            assert abs(backward_rotation + 0.25) < 1e-6, backward_rotation

            detached_scene = Scene()
            detached = Line(ORIGIN, LEFT)

            def removed_before_bind(mobject):
                mobject.shift(LEFT * 99)

            def live_after_bind(mobject, dt):
                mobject.rotate_about_origin(dt)

            detached.add_updater(removed_before_bind)
            detached.remove_updater(removed_before_bind)
            detached.add_updater(live_after_bind)
            detached_scene.add(detached)

            detached_config = updaters.register_scene(detached_scene)
            assert detached_config is not None
            assert len(detached_config["slots"]) == 2, detached_config
            removed_slot, live_slot = detached_config["slots"]
            assert removed_slot["active_after"] == 0.0
            assert removed_slot["active_through"] == 0.0
            assert live_slot["active_after"] == 0.0
            assert "active_through" not in live_slot
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
            msg=f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
