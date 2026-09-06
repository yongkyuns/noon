import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedLayoutQueryTests(unittest.TestCase):
    def test_detached_layout_queries_do_not_materialize_snapshots(self) -> None:
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
            import math
            import sys
            import types

            fake_js = types.ModuleType("js")
            fake_js.noonResolveUniformCompositionSchedule = object()
            fake_js.noonResolveAnimationOptions = object()
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_phase_b  # noqa: F401 - installs exact Manim world bounds
            import noon as _base
            import _manim_semantic_handles as handles

            class FakeHandle:
                def __init__(self, snapshot_json):
                    self.snapshot = json.loads(snapshot_json)
                    self.snapshot_requests = 0

                def _raw(self):
                    return _base._ir.Mobject(
                        geometry=self.snapshot["geometry"],
                        transform=self.snapshot["transform"],
                        style=self.snapshot["style"],
                    )

                def _bounds(self):
                    bounds = _base._bounds(self._raw())
                    assert bounds is not None
                    return bounds

                def snapshotJson(self):
                    self.snapshot_requests += 1
                    return json.dumps(self.snapshot, separators=(",", ":"))

                def replaceSnapshotJson(self, snapshot_json):
                    self.snapshot = json.loads(snapshot_json)

                def cloneHandle(self):
                    return FakeHandle(json.dumps(self.snapshot, separators=(",", ":")))

                @property
                def centerX(self):
                    minimum, maximum = self._bounds()
                    return (minimum.x + maximum.x) * 0.5

                @property
                def centerY(self):
                    minimum, maximum = self._bounds()
                    return (minimum.y + maximum.y) * 0.5

                @property
                def width(self):
                    minimum, maximum = self._bounds()
                    return maximum.x - minimum.x

                @property
                def height(self):
                    minimum, maximum = self._bounds()
                    return maximum.y - minimum.y

                def criticalX(self, direction_x, direction_y):
                    minimum, maximum = self._bounds()
                    center = (minimum.x + maximum.x) * 0.5
                    return minimum.x if direction_x < 0 else maximum.x if direction_x > 0 else center

                def criticalY(self, direction_x, direction_y):
                    minimum, maximum = self._bounds()
                    center = (minimum.y + maximum.y) * 0.5
                    return minimum.y if direction_y < 0 else maximum.y if direction_y > 0 else center

                def setFillOpacity(self, opacity):
                    fill = self.snapshot["style"]["fill"]
                    if fill is not None:
                        fill["alpha"] = float(opacity)

                def setStrokeOpacity(self, opacity):
                    stroke = self.snapshot["style"]["stroke"]
                    if stroke is not None:
                        stroke["alpha"] = float(opacity)

                def shift(self, x, y):
                    translation = self.snapshot["transform"]["translation"]
                    translation["x"] += float(x)
                    translation["y"] += float(y)

                def scale(self, x, y):
                    scale = self.snapshot["transform"]["scale"]
                    scale["x"] *= float(x)
                    scale["y"] *= float(y)

                def rotateAboutPoint(self, angle, point_x, point_y):
                    translation = self.snapshot["transform"]["translation"]
                    dx = translation["x"] - float(point_x)
                    dy = translation["y"] - float(point_y)
                    cosine = math.cos(float(angle))
                    sine = math.sin(float(angle))
                    translation["x"] = float(point_x) + dx * cosine - dy * sine
                    translation["y"] = float(point_y) + dx * sine + dy * cosine
                    self.snapshot["transform"]["rotation"] += float(angle)

                def alignOnFrame(self, direction_x, direction_y, buff):
                    point_x = self.criticalX(direction_x, direction_y)
                    point_y = self.criticalY(direction_x, direction_y)
                    shift_x = 0.0
                    shift_y = 0.0
                    if direction_x != 0.0:
                        target_x = math.copysign(_base.DEFAULT_FRAME_WIDTH * 0.5, direction_x)
                        shift_x = target_x - point_x - direction_x * float(buff)
                    if direction_y != 0.0:
                        target_y = math.copysign(_base.DEFAULT_FRAME_HEIGHT * 0.5, direction_y)
                        shift_y = target_y - point_y - direction_y * float(buff)
                    self.shift(shift_x, shift_y)

            fake_js.noonCreateAuthoringMobjectHandle = FakeHandle
            handles._create_handle = FakeHandle
            handles.install()

            from noon import Circle, DEFAULT_FRAME_WIDTH, PI, Path, RIGHT, VectorPath

            ellipse = Circle(1.0).scale((2.0, 1.0)).rotate(PI / 4.0)
            handle = ellipse._semantic_handle
            handle.snapshot_requests = 0
            expected = math.sqrt(10.0)
            assert abs(ellipse.width - expected) < 1e-12
            assert abs(ellipse.height - expected) < 1e-12
            assert abs(ellipse.get_center().x) < 1e-12
            assert abs(ellipse.get_center().y) < 1e-12
            # A typed leaf must ask its semantic handle for the critical point;
            # raw geometry is neither an input nor a fallback for this query.
            ellipse._current_raw = lambda: (_ for _ in ()).throw(
                AssertionError("critical point read raw Python geometry")
            )
            assert abs(ellipse.get_critical_point(RIGHT).x - expected * 0.5) < 1e-12
            assert handle.snapshot_requests == 0

            curve = Path(
                VectorPath()
                .move_to((-1.0, 0.0))
                .quadratic_to((0.0, 2.0), (1.0, 0.0))
            ).rotate(PI / 4.0)
            curve_handle = curve._semantic_handle
            curve_handle.snapshot_requests = 0
            expected_curve_extent = 9.0 * math.sqrt(2.0) / 8.0
            assert abs(curve.width - expected_curve_extent) < 1e-12
            assert abs(curve.height - expected_curve_extent) < 1e-12
            assert curve_handle.snapshot_requests == 0

            edge = ellipse.copy().to_edge(RIGHT, buff=0.5)
            edge_handle = edge._semantic_handle
            edge_handle.snapshot_requests = 0
            assert abs(
                edge.get_critical_point(RIGHT).x
                - (DEFAULT_FRAME_WIDTH * 0.5 - 0.5)
            ) < 1e-12
            assert edge_handle.snapshot_requests == 0
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
            "shared layout query subprocess failed:\n"
            f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
