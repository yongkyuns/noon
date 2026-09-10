import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimArcFacadeTests(unittest.TestCase):
    def test_arc_constructors_forward_geometry_to_shared_rust_options(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            value for value in (str(python_dir), env.get("PYTHONPATH")) if value
        )
        source = textwrap.dedent(
            r"""
            import json
            import sys
            import types

            fake_js = types.ModuleType("js")
            calls = []

            class FakeHandle:
                def __init__(self, snapshot):
                    self.snapshot = snapshot
                def snapshotJson(self):
                    return json.dumps(self.snapshot)

            def create_from_snapshot(value):
                return FakeHandle(json.loads(value))

            import _typed_geometry_test_support as support
            support.install_js_bridge(fake_js, create_from_snapshot)

            def path_options(tag):
                return support.FakeGeometryOptions({
                    "vector_path": {
                        "commands": [
                            {"move_to": {"to": {"x": 0.0, "y": 0.0}}},
                            {"line_to": {"to": {"x": 1.0, "y": 1.0}}},
                        ],
                        "tag": tag,
                    }
                })

            def arc(radius, start_angle, angle, num_components, center_x, center_y):
                calls.append((
                    "arc", float(radius), float(start_angle), float(angle),
                    int(num_components), float(center_x), float(center_y),
                ))
                return path_options("arc")

            def arc_between_points(start_x, start_y, end_x, end_y, angle, radius, num_components):
                calls.append((
                    "arc_between_points",
                    float(start_x), float(start_y), float(end_x), float(end_y),
                    float(angle), None if radius is None else float(radius), int(num_components),
                ))
                return path_options("arc_between_points")

            support.install_option_factory(fake_js, "arc", arc)
            support.install_option_factory(fake_js, "arcBetweenPoints", arc_between_points)
            sys.modules["js"] = fake_js

            import noon
            from noon import Arc, ArcBetweenPoints

            a = Arc(
                radius=2.0,
                start_angle=-0.25,
                angle=1.5,
                num_components=7,
                arc_center=(3.0, -2.0, 0.0),
                color="#58C4DD",
            )
            b = ArcBetweenPoints(
                (-2.0, 1.0, 0.0),
                (4.0, -3.0, 0.0),
                angle=-1.25,
                radius=None,
                num_components=5,
                stroke_width=0.08,
            )
            c = ArcBetweenPoints(noon.LEFT, noon.RIGHT, radius=-2.5)

            assert calls == [
                ("arc", 2.0, -0.25, 1.5, 7, 3.0, -2.0),
                ("arc_between_points", -2.0, 1.0, 4.0, -3.0, -1.25, None, 5),
                ("arc_between_points", -1.0, 0.0, 1.0, 0.0, noon.TAU / 4.0, -2.5, 9),
            ]
            assert a.radius == 2.0
            assert a.start_angle == -0.25
            assert a.angle == 1.5
            assert a.num_components == 7
            assert a.arc_center == noon.Vec2(3.0, -2.0)
            assert a.style["stroke"]["red"] == noon.BLUE.red
            assert b.style["stroke_width"] == 0.08
            assert not hasattr(b, "radius")
            assert c.radius == 2.5
            assert "Arc" in noon.__all__ and "ArcBetweenPoints" in noon.__all__

            before = list(calls)
            for thunk in (
                lambda: Arc(num_components=1),
                lambda: Arc(angle=float("inf")),
                lambda: ArcBetweenPoints((0, 0, 1), (1, 0, 0)),
            ):
                try:
                    thunk()
                except (TypeError, ValueError, NotImplementedError):
                    pass
                else:
                    raise AssertionError("invalid Arc input unexpectedly succeeded")
            assert calls == before
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
