import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedCoordinatePlacementTests(unittest.TestCase):
    def test_directional_set_and_match_coord_use_shared_placement(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        existing = env.get("PYTHONPATH")
        env["PYTHONPATH"] = (
            str(python_dir)
            if not existing
            else os.pathsep.join((str(python_dir), existing))
        )
        source = textwrap.dedent(
            r"""
            import json
            import sys
            import types

            fake_js = types.ModuleType("js")
            point_calls = []
            handle_calls = []

            class FakeHandle:
                def __init__(self, snapshot):
                    self.snapshot = snapshot
                def snapshotJson(self):
                    return json.dumps(self.snapshot)
                def setFillOpacity(self, value):
                    if self.snapshot["style"]["fill"] is not None:
                        self.snapshot["style"]["fill"]["alpha"] = float(value)
                def setStrokeOpacity(self, value):
                    if self.snapshot["style"]["stroke"] is not None:
                        self.snapshot["style"]["stroke"]["alpha"] = float(value)
                def shift(self, x, y):
                    translation = self.snapshot["transform"]["translation"]
                    translation["x"] += float(x)
                    translation["y"] += float(y)
                def _critical(self, edge_x, edge_y):
                    translation = self.snapshot["transform"]["translation"]
                    geometry = self.snapshot["geometry"]
                    radius = float(geometry.get("circle", {}).get("radius", 0.0))
                    return (
                        float(translation["x"]) + (radius if edge_x > 0 else -radius if edge_x < 0 else 0.0),
                        float(translation["y"]) + (radius if edge_y > 0 else -radius if edge_y < 0 else 0.0),
                    )
                def manimMoveToPoint(self, x, y, edge_x, edge_y, mask_x, mask_y):
                    point_calls.append((
                        float(x), float(y), float(edge_x), float(edge_y),
                        float(mask_x), float(mask_y),
                    ))
                    current_x, current_y = self._critical(edge_x, edge_y)
                    translation = self.snapshot["transform"]["translation"]
                    if mask_x:
                        translation["x"] += float(x) - current_x
                    if mask_y:
                        translation["y"] += float(y) - current_y
                def manimMoveToHandle(self, other, edge_x, edge_y, mask_x, mask_y):
                    handle_calls.append((
                        float(edge_x), float(edge_y), float(mask_x), float(mask_y),
                    ))
                    current_x, current_y = self._critical(edge_x, edge_y)
                    target_x, target_y = other._critical(edge_x, edge_y)
                    translation = self.snapshot["transform"]["translation"]
                    if mask_x:
                        translation["x"] += target_x - current_x
                    if mask_y:
                        translation["y"] += target_y - current_y

            def generic_snapshot(value):
                return FakeHandle(json.loads(value))

            fake_js.noonCreateAuthoringMobjectHandle = generic_snapshot
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_phase_b
            import _manim_geometry
            import _manim_semantic_handles as handles
            handles.install()
            import _manim_shared_geometry
            _manim_shared_geometry.install()

            from noon import Circle, RIGHT, UP

            source = Circle(1.0).shift((-2.0, -1.0))
            target = Circle(1.0).shift((4.0, 3.0))
            fail = lambda *args, **kwargs: (_ for _ in ()).throw(
                AssertionError("Python coordinate/critical-point placement was used")
            )
            source.get_critical_point = fail
            target.get_critical_point = fail
            source.get_coord = fail
            target.get_coord = fail

            assert source.set_x(5.0, RIGHT) is source
            assert source.set_y(-4.0, UP) is source
            assert source.match_coord(target, 0, RIGHT) is source
            assert source.match_coord(target, 1, UP) is source

            assert point_calls == [
                (5.0, 0.0, 1.0, 0.0, 1.0, 0.0),
                (0.0, -4.0, 0.0, 1.0, 0.0, 1.0),
            ]
            assert handle_calls == [
                (1.0, 0.0, 1.0, 0.0),
                (0.0, 1.0, 0.0, 1.0),
            ]
            assert source.transform["translation"] == {"x": 4.0, "y": 3.0}

            try:
                source.set_coord(1.0, 2)
            except NotImplementedError as error:
                assert "only x/y" in str(error)
            else:
                raise AssertionError("2D set_coord unexpectedly accepted z")
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
            f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
