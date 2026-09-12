import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimBraceFacadeTests(unittest.TestCase):
    def test_brace_facade_stays_thin_over_shared_rust_geometry(self) -> None:
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

            import _typed_geometry_test_support as support

            def path_options(tag):
                value = support.FakeGeometryOptions({
                    "vector_path": {
                        "commands": [
                            {"move_to": {"to": {"x": -1.0, "y": -1.0}}},
                            {"cubic_to": {
                                "control1": {"x": -0.5, "y": -1.2},
                                "control2": {"x": 0.5, "y": -1.2},
                                "to": {"x": 1.0, "y": -1.0},
                            }},
                        ],
                        "tag": tag,
                    }
                })
                value.setStrokeWidth(0.0)
                value.setFillOpacity(1.0)
                return value

            class FakeHandle:
                def __init__(self, snapshot):
                    self.snapshot = snapshot
                def snapshotJson(self):
                    return json.dumps(self.snapshot)
                def beginBrace(self, direction_x, direction_y, buff, sharpness):
                    calls.append((
                        "brace", float(direction_x), float(direction_y),
                        float(buff), float(sharpness),
                    ))
                    return path_options("brace")

            def create_from_snapshot(value):
                return FakeHandle(json.loads(value))

            support.install_js_bridge(fake_js, create_from_snapshot)

            def brace_between_points(x1, y1, x2, y2, dx, dy, buff, sharpness):
                calls.append((
                    "between", float(x1), float(y1), float(x2), float(y2),
                    float(dx), float(dy), float(buff), float(sharpness),
                ))
                return path_options("between")

            support.install_option_factory(
                fake_js, "braceBetweenPoints", brace_between_points
            )
            sys.modules["js"] = fake_js

            import noon
            from noon import Brace, BraceBetweenPoints, Square

            target = Square(2.0)
            brace = Brace(
                target,
                direction=noon.DOWN,
                buff=0.3,
                sharpness=1.5,
                color="#58C4DD",
            )
            between = BraceBetweenPoints(
                (-2.0, 1.0, 0.0),
                (3.0, -1.0, 0.0),
                direction=noon.ORIGIN,
                buff=0.1,
                sharpness=2.5,
            )

            assert calls == [
                ("brace", 0.0, -1.0, 0.3, 1.5),
                ("between", -2.0, 1.0, 3.0, -1.0, 0.0, 0.0, 0.1, 2.5),
            ]
            assert brace.buff == 0.3
            assert brace.sharpness == 1.5
            assert brace.direction == noon.DOWN
            assert brace.style["stroke_width"] == 0.0
            assert brace.style["fill"]["alpha"] == 1.0
            assert brace.style["fill"]["red"] == noon.BLUE.red
            assert between.buff == 0.1
            assert between.sharpness == 2.5
            assert "Brace" in noon.__all__
            assert "BraceBetweenPoints" in noon.__all__

            before = list(calls)
            for thunk in (
                lambda: Brace(target, buff=float("nan")),
                lambda: Brace(target, background_stroke_width=1.0),
                lambda: BraceBetweenPoints((0, 0, 1), (1, 0, 0)),
            ):
                try:
                    thunk()
                except (TypeError, ValueError, NotImplementedError):
                    pass
                else:
                    raise AssertionError("invalid Brace input unexpectedly succeeded")
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
