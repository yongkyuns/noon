import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSemanticHandleLayoutBoundsTests(unittest.TestCase):
    def test_detached_handles_use_exact_world_bounds_for_layout(self) -> None:
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

            import _manim_compat

            _manim_compat.install()
            import _manim_phase_b  # noqa: F401 - installs exact Manim layout bounds
            import _manim_semantic_handles as handles


            class FakeHandle:
                # Deliberately omit center/width/height/critical/nextTo/align shortcuts.
                # The adapter must obtain layout from the shared exact bounds contract.
                def __init__(self, snapshot_json):
                    self.snapshot = json.loads(snapshot_json)

                def snapshotJson(self):
                    return json.dumps(self.snapshot, separators=(\",\", \":\"))

                def replaceSnapshotJson(self, snapshot_json):
                    self.snapshot = json.loads(snapshot_json)

                def cloneHandle(self):
                    return FakeHandle(self.snapshotJson())

                def setFillOpacity(self, opacity):
                    fill = self.snapshot[\"style\"][\"fill\"]
                    if fill is not None:
                        fill[\"alpha\"] = float(opacity)

                def setStrokeOpacity(self, opacity):
                    stroke = self.snapshot[\"style\"][\"stroke\"]
                    if stroke is not None:
                        stroke[\"alpha\"] = float(opacity)

                def shift(self, x, y):
                    translation = self.snapshot[\"transform\"][\"translation\"]
                    translation[\"x\"] += float(x)
                    translation[\"y\"] += float(y)

                def scale(self, x, y):
                    scale = self.snapshot[\"transform\"][\"scale\"]
                    scale[\"x\"] *= float(x)
                    scale[\"y\"] *= float(y)

                def rotate(self, angle):
                    self.snapshot[\"transform\"][\"rotation\"] += float(angle)

                def rotateAboutPoint(self, angle, point_x, point_y):
                    translation = self.snapshot[\"transform\"][\"translation\"]
                    relative_x = translation[\"x\"] - float(point_x)
                    relative_y = translation[\"y\"] - float(point_y)
                    cosine = math.cos(float(angle))
                    sine = math.sin(float(angle))
                    translation[\"x\"] = (
                        float(point_x)
                        + relative_x * cosine
                        - relative_y * sine
                    )
                    translation[\"y\"] = (
                        float(point_y)
                        + relative_x * sine
                        + relative_y * cosine
                    )
                    self.snapshot[\"transform\"][\"rotation\"] += float(angle)


            handles._create_handle = FakeHandle
            handles.install()

            from noon import (
                Circle,
                DEFAULT_FRAME_WIDTH,
                PI,
                Path,
                RIGHT,
                Square,
                UP,
                VectorPath,
            )

            curve = Path(
                VectorPath()
                .move_to((-1.0, 0.0))
                .quadratic_to((0.0, 2.0), (1.0, 0.0))
            ).rotate(PI / 4.0)
            expected_curve_extent = 9.0 * math.sqrt(2.0) / 8.0
            assert abs(curve.width - expected_curve_extent) < 1e-12
            assert abs(curve.height - expected_curve_extent) < 1e-12

            ellipse = Circle(1.0).scale((2.0, 1.0)).rotate(PI / 4.0)
            expected_ellipse_extent = math.sqrt(10.0)
            assert abs(ellipse.width - expected_ellipse_extent) < 1e-12
            assert abs(ellipse.height - expected_ellipse_extent) < 1e-12

            square = Square(1.0).next_to(curve, RIGHT, buff=0.5)
            gap = (
                square.get_center().x
                - square.width * 0.5
                - curve.get_center().x
                - curve.width * 0.5
            )
            assert abs(gap - 0.5) < 1e-12

            moved = curve.copy().move_to((3.0, -2.0))
            assert abs(moved.get_center().x - 3.0) < 1e-12
            assert abs(moved.get_center().y + 2.0) < 1e-12

            aligned = Square(1.0).align_to(curve, UP)
            aligned_top = aligned.get_center().y + aligned.height * 0.5
            curve_top = curve.get_center().y + curve.height * 0.5
            assert abs(aligned_top - curve_top) < 1e-12

            edge = curve.copy().to_edge(RIGHT, buff=0.5)
            edge_right = edge.get_center().x + edge.width * 0.5
            assert abs(edge_right - (DEFAULT_FRAME_WIDTH * 0.5 - 0.5)) < 1e-12
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
