import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimRotateSemanticsTests(unittest.TestCase):
    def test_rotate_uses_manim_pivots_for_python_and_semantic_handles(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        existing = env.get("PYTHONPATH")
        env["PYTHONPATH"] = str(python_dir) if not existing else os.pathsep.join((str(python_dir), existing))
        source = textwrap.dedent(
  """
  import json
  import math
  import _manim_compat
  _manim_compat.install()
  import _manim_phase_b  # noqa: F401
  from noon import IN, LEFT, Line, ORIGIN, PI, RIGHT

  def close(a, b, eps=1e-9):
      assert abs(a - b) <= eps, (a, b)

  line = Line(ORIGIN, RIGHT).shift(2 * RIGHT)
  before = line.get_center()
  line.rotate(PI / 2)
  after = line.get_center()
  close(after.x, before.x)
  close(after.y, before.y)

  around_origin = Line(ORIGIN, RIGHT).shift(2 * RIGHT)
  around_origin.rotate(PI / 2, about_point=ORIGIN)
  close(around_origin.get_center().x, 0.0)
  close(around_origin.get_center().y, 2.5)

  around_left = Line(ORIGIN, RIGHT).shift(2 * RIGHT)
  around_left.rotate(PI / 2, about_edge=LEFT)
  close(around_left.get_center().x, 2.0)
  close(around_left.get_center().y, 0.5)

  precedence = Line(ORIGIN, RIGHT).shift(2 * RIGHT)
  precedence.rotate(PI / 2, about_point=ORIGIN, about_edge=RIGHT)
  close(precedence.get_center().x, 0.0)
  close(precedence.get_center().y, 2.5)

  clockwise = Line(ORIGIN, RIGHT).shift(2 * RIGHT)
  clockwise.rotate(PI / 2, axis=IN, about_point=ORIGIN)
  close(clockwise.get_center().x, 0.0)
  close(clockwise.get_center().y, -2.5)

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
          if self.snapshot[\"style\"][\"fill\"] is not None:
              self.snapshot[\"style\"][\"fill\"][\"alpha\"] = float(opacity)
      def setStrokeOpacity(self, opacity):
          if self.snapshot[\"style\"][\"stroke\"] is not None:
              self.snapshot[\"style\"][\"stroke\"][\"alpha\"] = float(opacity)
      def shift(self, x, y):
          t = self.snapshot[\"transform\"][\"translation\"]
          t[\"x\"] += float(x); t[\"y\"] += float(y)
      def scale(self, x, y):
          s = self.snapshot[\"transform\"][\"scale\"]
          s[\"x\"] *= float(x); s[\"y\"] *= float(y)
      def rotateAboutPoint(self, angle, point_x, point_y):
          t = self.snapshot[\"transform\"][\"translation\"]
          dx = t[\"x\"] - point_x; dy = t[\"y\"] - point_y
          c = math.cos(angle); s = math.sin(angle)
          t[\"x\"] = point_x + dx * c - dy * s
          t[\"y\"] = point_y + dx * s + dy * c
          self.snapshot[\"transform\"][\"rotation\"] += float(angle)

  handles._create_handle = FakeHandle
  handles.install()
  semantic = Line(ORIGIN, RIGHT).shift(2 * RIGHT)
  before = semantic.get_center()
  semantic.rotate(PI / 2)
  after = semantic.get_center()
  close(after.x, before.x)
  close(after.y, before.y)
  semantic_origin = Line(ORIGIN, RIGHT).shift(2 * RIGHT)
  semantic_origin.rotate(PI / 2, about_point=ORIGIN)
  close(semantic_origin.get_center().x, 0.0)
  close(semantic_origin.get_center().y, 2.5)
  """
        )
        completed = subprocess.run(
  [sys.executable, "-c", source], cwd=python_dir, env=env,
  capture_output=True, text=True, check=False,
        )
        self.assertEqual(completed.returncode, 0, f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}")


if __name__ == "__main__":
    unittest.main()
