import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedShapeMatcherTests(unittest.TestCase):
    def test_leaf_and_family_targets_stay_on_shared_matcher_path(self) -> None:
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

            import _manim_compat
            _manim_compat.install()
            import _manim_phase_b
            import _manim_geometry
            import _manim_semantic_handles as handles

            calls = []

            def snapshot(cx, cy, width, height, *, background=False, fill_opacity=0.75):
                return {
                    "geometry": {"rectangle": {"size": {"x": float(width), "y": float(height)}}},
                    "transform": {
                        "translation": {"x": float(cx), "y": float(cy)},
                        "rotation": 0.0,
                        "scale": {"x": 1.0, "y": 1.0},
                    },
                    "style": {
                        "fill": {
                            "red": 0.0 if background else 1.0,
                            "green": 0.0 if background else 1.0,
                            "blue": 0.0,
                            "alpha": float(fill_opacity) if background else 0.0,
                        },
                        "stroke": {
                            "red": 0.0 if background else 1.0,
                            "green": 0.0 if background else 1.0,
                            "blue": 0.0,
                            "alpha": 0.0 if background else 1.0,
                        },
                        "stroke_width": 0.0 if background else 0.04,
                        "stroke_width_mode": "screen_space",
                        "stroke_join": "miter",
                        "stroke_cap": "butt",
                        "opacity": 1.0,
                    },
                }

            class FakeHandle:
                def __init__(self, store, snapshot_json):
                    self.store = store
                    self.identity = store.allocate()
                    self.snapshot = json.loads(snapshot_json)
                    self._sync()

                def _sync(self):
                    transform = self.snapshot["transform"]
                    self.wireTranslationX = float(transform["translation"]["x"])
                    self.wireTranslationY = float(transform["translation"]["y"])
                    self.wireScaleX = float(transform["scale"]["x"])
                    self.wireScaleY = float(transform["scale"]["y"])
                    self.wireRotation = float(transform["rotation"])
                    fill = self.snapshot["style"]["fill"]
                    stroke = self.snapshot["style"]["stroke"]
                    self.wireHasFill = fill is not None
                    self.wireFillRed = 0.0 if fill is None else float(fill["red"])
                    self.wireFillGreen = 0.0 if fill is None else float(fill["green"])
                    self.wireFillBlue = 0.0 if fill is None else float(fill["blue"])
                    self.wireFillAlpha = 0.0 if fill is None else float(fill["alpha"])
                    self.wireHasStroke = stroke is not None
                    self.wireStrokeRed = 0.0 if stroke is None else float(stroke["red"])
                    self.wireStrokeGreen = 0.0 if stroke is None else float(stroke["green"])
                    self.wireStrokeBlue = 0.0 if stroke is None else float(stroke["blue"])
                    self.wireStrokeAlpha = 0.0 if stroke is None else float(stroke["alpha"])
                    self.wireStrokeWidth = float(self.snapshot["style"]["stroke_width"])
                    self.wireObjectOpacity = float(self.snapshot["style"]["opacity"])

                def snapshotJson(self):
                    return json.dumps(self.snapshot, separators=(",", ":"))

                @property
                def centerX(self):
                    return self.wireTranslationX

                @property
                def centerY(self):
                    return self.wireTranslationY

                @property
                def width(self):
                    return float(self.snapshot["geometry"]["rectangle"]["size"]["x"])

                @property
                def height(self):
                    return float(self.snapshot["geometry"]["rectangle"]["size"]["y"])

                @property
                def fillOpacity(self):
                    return self.wireFillAlpha

                @property
                def strokeOpacity(self):
                    return self.wireStrokeAlpha

                def criticalX(self, direction_x, direction_y):
                    del direction_y
                    edge = self.width * 0.5
                    return self.centerX + (edge if direction_x > 0 else (-edge if direction_x < 0 else 0.0))

                def criticalY(self, direction_x, direction_y):
                    del direction_x
                    edge = self.height * 0.5
                    return self.centerY + (edge if direction_y > 0 else (-edge if direction_y < 0 else 0.0))

                def shift(self, x, y):
                    translation = self.snapshot["transform"]["translation"]
                    translation["x"] += float(x)
                    translation["y"] += float(y)
                    self._sync()

                def surroundingRectangleSnapshotJson(self, buff_x, buff_y, corner_radius):
                    calls.append(("leaf-surround", float(buff_x), float(buff_y), float(corner_radius)))
                    return json.dumps(snapshot(
                        self.centerX,
                        self.centerY,
                        self.width + 2.0 * float(buff_x),
                        self.height + 2.0 * float(buff_y),
                    ))

                def backgroundRectangleSnapshotJson(self, buff_x, buff_y, corner_radius, fill_opacity):
                    calls.append(("leaf-background", float(buff_x), float(buff_y), float(corner_radius)))
                    return json.dumps(snapshot(
                        self.centerX,
                        self.centerY,
                        self.width + 2.0 * float(buff_x),
                        self.height + 2.0 * float(buff_y),
                        background=True,
                        fill_opacity=float(fill_opacity),
                    ))

                def setStrokeWidth(self, value):
                    self.snapshot["style"]["stroke_width"] = float(value)
                    self._sync()

                def setFillOpacity(self, value):
                    self.snapshot["style"]["fill"]["alpha"] = float(value)
                    self._sync()

                def setStrokeOpacity(self, value):
                    self.snapshot["style"]["stroke"]["alpha"] = float(value)
                    self._sync()

                def setFillColor(self, red, green, blue, alpha):
                    del alpha
                    current = self.snapshot["style"]["fill"]["alpha"]
                    self.snapshot["style"]["fill"] = {
                        "red": float(red), "green": float(green), "blue": float(blue), "alpha": current
                    }
                    self._sync()

                def setStrokeColor(self, red, green, blue, alpha):
                    del alpha
                    current = self.snapshot["style"]["stroke"]["alpha"]
                    self.snapshot["style"]["stroke"] = {
                        "red": float(red), "green": float(green), "blue": float(blue), "alpha": current
                    }
                    self._sync()

            class FakeLayoutSession:
                def __init__(self):
                    self.members = []

                def includeMobject(self, member):
                    self.members.append(member)

                def _bounds(self):
                    min_x = min(member.criticalX(-1.0, 0.0) for member in self.members)
                    max_x = max(member.criticalX(1.0, 0.0) for member in self.members)
                    min_y = min(member.criticalY(0.0, -1.0) for member in self.members)
                    max_y = max(member.criticalY(0.0, 1.0) for member in self.members)
                    return min_x, min_y, max_x, max_y

                @property
                def centerX(self):
                    min_x, _, max_x, _ = self._bounds()
                    return (min_x + max_x) * 0.5

                @property
                def centerY(self):
                    _, min_y, _, max_y = self._bounds()
                    return (min_y + max_y) * 0.5

                @property
                def width(self):
                    min_x, _, max_x, _ = self._bounds()
                    return max_x - min_x

                @property
                def height(self):
                    _, min_y, _, max_y = self._bounds()
                    return max_y - min_y

                def surroundingRectangleSnapshotJson(self, buff_x, buff_y, corner_radius):
                    calls.append(("family-surround", len(self.members), float(corner_radius)))
                    return json.dumps(snapshot(
                        self.centerX, self.centerY,
                        self.width + 2.0 * float(buff_x), self.height + 2.0 * float(buff_y)
                    ))

                def backgroundRectangleSnapshotJson(self, buff_x, buff_y, corner_radius, fill_opacity):
                    calls.append(("family-background", len(self.members), float(corner_radius)))
                    return json.dumps(snapshot(
                        self.centerX, self.centerY,
                        self.width + 2.0 * float(buff_x), self.height + 2.0 * float(buff_y),
                        background=True, fill_opacity=float(fill_opacity)
                    ))

            class FakeFamilyHandle:
                def __init__(self, store):
                    self.store = store
                    self.identity = store.allocate()
                    self.members = []

                def layoutSession(self):
                    return FakeLayoutSession()

                @property
                def memberCount(self):
                    return len(self.members)

                def addMobject(self, member):
                    if member.identity in self.members:
                        return False
                    self.members.append(member.identity)
                    return True

                def addFamily(self, member):
                    return self.addMobject(member)

                def removeMobject(self, member):
                    if member.identity not in self.members:
                        return False
                    self.members.remove(member.identity)
                    return True

                def removeFamily(self, member):
                    return self.removeMobject(member)

            class FakeStore:
                def __init__(self):
                    self.next_identity = 0

                def allocate(self):
                    value = self.next_identity
                    self.next_identity += 1
                    return value

                def createMobject(self, snapshot_json):
                    return FakeHandle(self, snapshot_json)

                def createRectangle(self, width, height):
                    return self.createMobject(json.dumps(snapshot(0.0, 0.0, width, height)))

                def createFamily(self):
                    return FakeFamilyHandle(self)

            store = FakeStore()
            handles._create_handle = store.createMobject
            handles._create_rectangle_handle = store.createRectangle
            handles._create_family_handle = store.createFamily
            handles.install()

            import noon as _base
            _base._bounds = lambda *args, **kwargs: (_ for _ in ()).throw(
                AssertionError("Python bounds computation was called")
            )

            import _manim_shared_geometry
            _manim_shared_geometry.install()
            from noon import BackgroundRectangle, BLUE, Rectangle, SurroundingRectangle, VGroup

            leaf = Rectangle(width=4.0, height=2.0)
            surround = SurroundingRectangle(leaf, buff=(0.25, 0.5), color=BLUE)
            assert isinstance(surround, Rectangle)
            assert calls[0][:3] == ("leaf-surround", 0.25, 0.5)
            assert abs(surround.width - 4.5) < 1e-9
            assert abs(surround.height - 3.0) < 1e-9

            left = Rectangle(width=2.0, height=2.0).shift((-2.0, 0.0, 0.0))
            right = Rectangle(width=4.0, height=1.0).shift((3.0, 2.0, 0.0))
            family = VGroup(left, right)
            grouped = SurroundingRectangle(family, buff=(0.25, 0.5))
            assert calls[-1][0] == "family-surround"
            assert calls[-1][1] == 2
            assert abs(grouped.width - 8.5) < 1e-9
            assert abs(grouped.height - 4.5) < 1e-9

            background = BackgroundRectangle(family, fill_opacity=0.6)
            assert calls[-1][0] == "family-background"
            assert abs(background.get_fill_opacity() - 0.6) < 1e-9
            assert abs(background.get_stroke_opacity()) < 1e-9
            assert background.style["stroke_width"] == 0.0

            family._semantic_family_handle = None
            try:
                SurroundingRectangle(family)
            except NotImplementedError as error:
                assert "shared semantic family bounds" in str(error)
            else:
                raise AssertionError("family matcher must not fall back to Python bounds")

            leaf._semantic_handle_fresh = False
            try:
                BackgroundRectangle(leaf)
            except NotImplementedError as error:
                assert "shared semantic geometry" in str(error)
            else:
                raise AssertionError("stale leaf matcher must fail closed")
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
