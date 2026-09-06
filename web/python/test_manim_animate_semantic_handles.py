import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimAnimateSemanticHandleTests(unittest.TestCase):
    def test_moving_around_animate_builder_uses_detached_shared_handle(self) -> None:
        """The MovingAround chain must mutate a Rust-owned target copy.

        The browser worker supplies the real handle through ``js``.  This test uses a
        recording fake with the same narrow surface so it can assert ownership and
        dispatch without requiring a browser or changing the production adapter.
        """

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

            class FakeHandle:
                created = []

                def __init__(self, snapshot_json):
                    self.snapshot = json.loads(snapshot_json)
                    self.calls = []
                    self.snapshot_requests = 0
                    type(self).created.append(self)

                def __deepcopy__(self, memo):
                    raise TypeError("opaque Rust handles cannot be copied by Python")

                def snapshotJson(self):
                    self.snapshot_requests += 1
                    return json.dumps(self.snapshot, separators=(",", ":"))

                def replaceSnapshotJson(self, snapshot_json):
                    self.calls.append("replaceSnapshotJson")
                    self.snapshot = json.loads(snapshot_json)

                def cloneHandle(self):
                    self.calls.append("cloneHandle")
                    clone = FakeHandle(json.dumps(self.snapshot))
                    clone.calls.append("cloned")
                    return clone

                def targetEditor(self):
                    self.calls.append("targetEditor")
                    target = FakeHandle(json.dumps(self.snapshot))
                    target.calls.append("targetEditor")
                    return target

                @property
                def centerX(self):
                    self.calls.append("centerX")
                    return float(self.snapshot["transform"]["translation"]["x"])

                @property
                def centerY(self):
                    self.calls.append("centerY")
                    return float(self.snapshot["transform"]["translation"]["y"])

                def criticalX(self, direction_x, direction_y):
                    self.calls.append("criticalX")
                    return self.centerX

                def criticalY(self, direction_x, direction_y):
                    self.calls.append("criticalY")
                    return self.centerY

                def setFillOpacity(self, opacity):
                    self.calls.append("setFillOpacity")
                    self.snapshot["style"]["fill"]["alpha"] = float(opacity)

                def setStrokeOpacity(self, opacity):
                    self.calls.append("setStrokeOpacity")
                    self.snapshot["style"]["stroke"]["alpha"] = float(opacity)

                def setFillColor(self, red, green, blue, alpha):
                    self.calls.append("setFillColor")
                    fill = self.snapshot["style"]["fill"]
                    fill.update(red=float(red), green=float(green), blue=float(blue))

                def setStrokeColor(self, red, green, blue, alpha):
                    self.calls.append("setStrokeColor")

                def shift(self, x, y):
                    self.calls.append(("shift", float(x), float(y)))
                    translation = self.snapshot["transform"]["translation"]
                    translation["x"] += float(x)
                    translation["y"] += float(y)

                def scale(self, x, y):
                    self.calls.append(("scale", float(x), float(y)))
                    scale = self.snapshot["transform"]["scale"]
                    scale["x"] *= float(x)
                    scale["y"] *= float(y)

                def rotateAboutPoint(self, angle, point_x, point_y):
                    self.calls.append(("rotateAboutPoint", float(angle)))
                    translation = self.snapshot["transform"]["translation"]
                    dx = translation["x"] - float(point_x)
                    dy = translation["y"] - float(point_y)
                    cosine = math.cos(float(angle))
                    sine = math.sin(float(angle))
                    translation["x"] = float(point_x) + dx * cosine - dy * sine
                    translation["y"] = float(point_y) + dx * sine + dy * cosine
                    self.snapshot["transform"]["rotation"] += float(angle)

            fake_js.noonCreateAuthoringMobjectHandle = FakeHandle
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_rate_functions
            _manim_rate_functions.install()
            import _manim_phase_b  # noqa: F401
            import _manim_semantic_handles as handles
            handles._create_handle = FakeHandle
            handles.install()
            import _manim_animate as animate

            from noon import BLUE, LEFT, ORANGE, Scene, Square, VGroup

            # Detached objects must create animation targets through the explicit
            # Rust target-editor boundary, with no Python snapshot round-trip.
            detached = Square(color=BLUE, fill_opacity=1)
            detached_source_handle = detached._semantic_handle
            detached._retained_handle = detached_source_handle
            detached_before = detached._current_raw().to_ir()
            detached_source_handle.snapshot_requests = 0
            detached_builder = animate._AlignedAnimationBuilder(detached)
            detached_target = detached_builder.target
            detached_target_handle = detached_target._semantic_handle
            assert detached._raw is None
            assert detached_target._retained_handle is detached_target_handle
            assert detached_target._raw is None
            assert detached_target_handle is not detached_source_handle
            assert detached_target_handle.calls == ["targetEditor"]

            detached_builder.shift(LEFT)
            detached_builder.set_fill(ORANGE)
            detached_builder.scale(0.3)
            detached_builder.rotate(0.4)
            assert detached_target_handle.calls == [
                "targetEditor",
                ("shift", -1.0, 0.0),
                "setFillColor",
                ("scale", 0.3, 0.3),
                "centerX",
                "centerY",
                ("rotateAboutPoint", 0.4),
            ], detached_target_handle.calls
            assert detached_source_handle.snapshot_requests == 0
            assert detached_target_handle.snapshot_requests == 0
            assert detached._current_raw().to_ir() == detached_before

            square = Square(color=BLUE, fill_opacity=1)
            source_handle = square._semantic_handle
            scene = Scene()
            scene.add(square)
            source_before = square._current_raw().to_ir()
            source_handle.snapshot_requests = 0
            source_handle.calls.clear()
            builder = animate._AlignedAnimationBuilder(square)
            target = builder.target
            target_handle = target._semantic_handle

            assert square._scene is scene
            assert square._object is not None
            assert target._raw is None
            assert target_handle is not source_handle
            assert target._scene is None
            assert target._object is None

            builder.shift(LEFT)
            builder.set_fill(ORANGE)
            builder.scale(0.3)
            builder.rotate(0.4)

            assert target_handle.calls == [
                "targetEditor",
                ("shift", -1.0, 0.0),
                "setFillColor",
                ("scale", 0.3, 0.3),
                "centerX",
                "centerY",
                ("rotateAboutPoint", 0.4),
            ], target_handle.calls
            assert source_handle.calls == ["targetEditor"], source_handle.calls
            assert source_handle.snapshot_requests == 0
            assert target_handle.snapshot_requests == 0
            assert square._current_raw().to_ir() == source_before
            target_ir = target._current_raw().to_ir()
            assert target_ir["transform"]["translation"] == {"x": -1.0, "y": 0.0}
            assert target_ir["transform"]["scale"] == {"x": 0.3, "y": 0.3}
            assert abs(target_ir["transform"]["rotation"] - 0.4) < 1e-12
            assert square.get_center().x == 0.0
            assert square.get_center().y == 0.0

            # Group wrapper identity remains Python metadata while the installed
            # shared-family adapter owns membership and target topology in browsers.
            group = VGroup(Square(), Square().shift(LEFT))
            group_builder = group.animate
            assert isinstance(group_builder.target, VGroup)
            assert group_builder.target is not group
            assert len(group_builder.target.submobjects) == 2
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
            "semantic animate subprocess failed:\n"
            f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
