import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedSectorTests(unittest.TestCase):
    def test_sector_family_uses_shared_rust_constructors(self) -> None:
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
            import math
            import sys
            import types

            fake_js = types.ModuleType("js")
            calls = []

            WHITE = (1.0, 1.0, 1.0)

            def style():
                return {
                    "fill": {"red": 1.0, "green": 1.0, "blue": 1.0, "alpha": 1.0},
                    "stroke": {"red": 1.0, "green": 1.0, "blue": 1.0, "alpha": 1.0},
                    "stroke_width": 0.0,
                    "stroke_width_mode": "screen_space",
                    "stroke_join": "miter",
                    "stroke_cap": "butt",
                    "opacity": 1.0,
                }

            def snapshot(tag):
                return {
                    "geometry": {"vector_path": {"commands": [
                        {"move_to": {"to": {"x": 1.0, "y": 0.0}}},
                        {"line_to": {"to": {"x": 0.0, "y": 1.0}}},
                        "close",
                    ]}},
                    "transform": {
                        "translation": {"x": 0.0, "y": 0.0},
                        "rotation": 0.0,
                        "scale": {"x": 1.0, "y": 1.0},
                    },
                    "style": style(),
                    "debug_tag": tag,
                }

            class FakeHandle:
                def __init__(self, value):
                    self.snapshot = value
                def snapshotJson(self):
                    value = dict(self.snapshot)
                    value.pop("debug_tag", None)
                    return json.dumps(value)
                def setStrokeWidth(self, value):
                    self.snapshot["style"]["stroke_width"] = float(value)
                def setFillOpacity(self, value):
                    self.snapshot["style"]["fill"]["alpha"] = float(value)
                def setStrokeOpacity(self, value):
                    self.snapshot["style"]["stroke"]["alpha"] = float(value)
                def setFillColor(self, r, g, b, a):
                    alpha = self.snapshot["style"]["fill"]["alpha"]
                    self.snapshot["style"]["fill"] = {
                        "red": float(r), "green": float(g), "blue": float(b), "alpha": alpha,
                    }
                def setStrokeColor(self, r, g, b, a):
                    alpha = self.snapshot["style"]["stroke"]["alpha"]
                    self.snapshot["style"]["stroke"] = {
                        "red": float(r), "green": float(g), "blue": float(b), "alpha": alpha,
                    }

            def generic_snapshot(value):
                return FakeHandle(json.loads(value))

            def annular_sector(inner, outer, angle, start, components, cx, cy):
                calls.append((
                    "annular_sector", float(inner), float(outer), float(angle), float(start),
                    int(components), float(cx), float(cy),
                ))
                return FakeHandle(snapshot("annular_sector"))

            def sector(radius, angle, start, components, cx, cy):
                calls.append((
                    "sector", float(radius), float(angle), float(start), int(components),
                    float(cx), float(cy),
                ))
                return FakeHandle(snapshot("sector"))

            def annulus(inner, outer, components, cx, cy):
                calls.append((
                    "annulus", float(inner), float(outer), int(components), float(cx), float(cy),
                ))
                return FakeHandle(snapshot("annulus"))

            fake_js.noonCreateAuthoringMobjectHandle = generic_snapshot
            fake_js.noonCreateAuthoringAnnularSectorHandle = annular_sector
            fake_js.noonCreateAuthoringSectorHandle = sector
            fake_js.noonCreateAuthoringAnnulusHandle = annulus
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_phase_b
            import _manim_geometry
            import _manim_semantic_handles as handles
            handles.install()

            _manim_compat._ir.Path = lambda *args, **kwargs: (_ for _ in ()).throw(
                AssertionError("Python Path geometry constructor was called")
            )

            import _manim_shared_geometry
            _manim_shared_geometry.install()
            from noon import AnnularSector, Annulus, BLUE, Sector

            annular = AnnularSector(
                inner_radius=0.5,
                outer_radius=1.75,
                angle=1.2,
                start_angle=0.3,
                fill_opacity=0.4,
                stroke_width=2.0,
                color=BLUE,
                num_components=7,
                arc_center=(2.0, -1.0, 0.0),
            )
            circle_sector = Sector(
                radius=2.5,
                angle=0.8,
                start_angle=0.1,
                num_components=6,
                arc_center=(-1.0, 2.0, 0.0),
            )
            ring = Annulus(
                inner_radius=0.75,
                outer_radius=1.5,
                num_components=10,
                arc_center=(3.0, 4.0, 0.0),
                mark_paths_closed=True,
            )

            assert calls == [
                ("annular_sector", 0.5, 1.75, 1.2, 0.3, 7, 2.0, -1.0),
                ("sector", 2.5, 0.8, 0.1, 6, -1.0, 2.0),
                ("annulus", 0.75, 1.5, 10, 3.0, 4.0),
            ]
            assert annular.inner_radius == 0.5
            assert annular.outer_radius == 1.75
            assert annular.num_components == 7
            assert annular.arc_center.x == 2.0 and annular.arc_center.y == -1.0
            assert math.isclose(annular.style["fill"]["alpha"], 0.4)
            assert annular.style["stroke_width"] == 2.0
            assert math.isclose(annular.style["fill"]["red"], BLUE.red)
            assert math.isclose(annular.style["stroke"]["blue"], BLUE.blue)
            assert circle_sector.inner_radius == 0.0
            assert circle_sector.outer_radius == 2.5
            assert ring.mark_paths_closed is True
            assert "vector_path" in annular.geometry
            assert "vector_path" in circle_sector.geometry
            assert "vector_path" in ring.geometry

            try:
                AnnularSector(num_components=1)
            except ValueError as error:
                assert "at least 2" in str(error)
            else:
                raise AssertionError("invalid num_components was accepted")
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
