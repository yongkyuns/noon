import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSemanticHandleAlignmentTests(unittest.TestCase):
    def test_semantic_handles_share_manim_placement_formulas(self) -> None:
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

            from noon import LEFT, Rectangle, RIGHT, Square, UP, UR, VGroup

            target = Square(2.0)
            diagonal = Square(2.0).next_to(target, UR, buff=0.25)
            assert abs(diagonal.get_center().x - 2.25) < 1e-12
            assert abs(diagonal.get_center().y - 2.25) < 1e-12

            reference = Rectangle(width=2.0, height=3.0)
            aligned = Square(1.0).next_to(
                reference, RIGHT, buff=0.4, aligned_edge=UP
            )
            assert abs(aligned.get_top().y - reference.get_top().y) < 1e-12
            assert abs(aligned.get_left().x - reference.get_right().x - 0.4) < 1e-12

            moved = Square(1.0).move_to(reference, aligned_edge=UP + LEFT)
            assert abs(moved.get_top().y - reference.get_top().y) < 1e-12
            assert abs(moved.get_left().x - reference.get_left().x) < 1e-12

            short = Rectangle(width=1.0, height=1.0)
            tall = Rectangle(width=1.0, height=3.0)
            VGroup(short, tall).arrange(
                RIGHT, buff=0.3, aligned_edge=UP, center=False
            )
            assert abs(short.get_top().y - tall.get_top().y) < 1e-12
            assert abs(tall.get_left().x - short.get_right().x - 0.3) < 1e-12
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
