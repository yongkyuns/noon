import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimLayoutClosureTests(unittest.TestCase):
    def test_supported_layout_contract_matches_manim_for_all_owners(self) -> None:
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

            import _manim_compat

            _manim_compat.install()
            import _manim_phase_b  # noqa: F401 - installs exact Manim layout bounds
            import _manim_semantic_handles as handles

            from noon import (
                DEFAULT_FRAME_HEIGHT,
                DEFAULT_FRAME_WIDTH,
                DEFAULT_MOBJECT_TO_EDGE_BUFFER,
                LEFT,
                Rectangle,
                RIGHT,
                Square,
                UP,
                UR,
                VGroup,
            )


            def assert_close(actual, expected, tolerance=1e-12):
                assert abs(actual - expected) < tolerance, (actual, expected)


            def check_layout_contract():
                aligned = Square(1.0).shift((-2.0, -3.0)).align_to((2.5, 1.75), UR)
                assert_close(aligned.get_right().x, 2.5)
                assert_close(aligned.get_top().y, 1.75)

                x_only = Square(1.0).shift((0.0, -1.25))
                original_y = x_only.get_center().y
                x_only.align_to((4.0, 99.0), RIGHT)
                assert_close(x_only.get_right().x, 4.0)
                assert_close(x_only.get_center().y, original_y)

                border = Rectangle(width=2.0, height=1.0).shift((1.0, -1.0))
                border_y = border.get_center().y
                border.to_edge(RIGHT)
                assert_close(
                    border.get_right().x,
                    DEFAULT_FRAME_WIDTH * 0.5 - DEFAULT_MOBJECT_TO_EDGE_BUFFER,
                )
                assert_close(border.get_center().y, border_y)

                corner = Square(1.0).to_corner(UR)
                assert_close(
                    corner.get_right().x,
                    DEFAULT_FRAME_WIDTH * 0.5 - DEFAULT_MOBJECT_TO_EDGE_BUFFER,
                )
                assert_close(
                    corner.get_top().y,
                    DEFAULT_FRAME_HEIGHT * 0.5 - DEFAULT_MOBJECT_TO_EDGE_BUFFER,
                )

                non_unit = Square(1.0).to_corner((2.0, 1.0), buff=0.25)
                assert_close(non_unit.get_right().x, DEFAULT_FRAME_WIDTH * 0.5 - 0.5)
                assert_close(non_unit.get_top().y, DEFAULT_FRAME_HEIGHT * 0.5 - 0.25)

                left = Rectangle(width=1.0, height=2.0).shift(LEFT * 2.0)
                right = Rectangle(width=2.0, height=1.0).shift(RIGHT * 3.0)
                group = VGroup(left, right)
                assert_close(group.width, 6.5)
                assert_close(group.height, 2.0)
                assert_close(group.get_center().x, 0.75)

                width_before = group.width
                height_before = group.height
                left.set_stroke(width=100.0)
                right.set_stroke(width=50.0)
                assert_close(group.width, width_before)
                assert_close(group.height, height_before)

                group.align_to((0.0, 2.25), UP)
                assert_close(group.get_top().y, 2.25)


            # CPython/fallback ownership path.
            check_layout_contract()


            class FakeHandle:
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

                def setStrokeWidth(self, width):
                    self.snapshot[\"style\"][\"stroke_width\"] = float(width)

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


            handles._create_handle = FakeHandle
            handles.install()

            # Detached browser semantic-handle ownership path.
            check_layout_contract()
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
