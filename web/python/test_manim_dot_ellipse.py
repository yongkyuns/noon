import os
from pathlib import Path
import subprocess
import sys
import textwrap
import unittest


PYTHON_ROOT = Path(__file__).resolve().parent


class ManimDotEllipseTests(unittest.TestCase):
    def test_typed_ellipse_uses_shared_constructor_and_layout(self) -> None:
        source = textwrap.dedent(
            r"""
            import json
            import math
            import sys
            import types

            fake_js = types.ModuleType("js")
            constructor_calls = []

            class LayoutHandle:
                def __init__(self, snapshot_json):
                    self.snapshot = json.loads(snapshot_json)

                def _bounds(self):
                    transform = self.snapshot["transform"]
                    tx = float(transform["translation"]["x"])
                    ty = float(transform["translation"]["y"])
                    if abs(float(transform["rotation"])) < 1e-12:
                        width = 2.0 * abs(float(transform["scale"]["x"]))
                        height = 2.0 * abs(float(transform["scale"]["y"]))
                    else:
                        # Independent oracle values supplied by the shared Rust
                        # Ellipse policy proof; this fake tests Python routing only.
                        width = 2.3289230112388193
                        height = 2.8632927011928597
                    return (
                        tx - width / 2.0,
                        ty - height / 2.0,
                        tx + width / 2.0,
                        ty + height / 2.0,
                    )

                @property
                def centerX(self):
                    low_x, _, high_x, _ = self._bounds()
                    return (low_x + high_x) / 2.0

                @property
                def centerY(self):
                    _, low_y, _, high_y = self._bounds()
                    return (low_y + high_y) / 2.0

                @property
                def width(self):
                    low_x, _, high_x, _ = self._bounds()
                    return high_x - low_x

                @property
                def height(self):
                    _, low_y, _, high_y = self._bounds()
                    return high_y - low_y

                def criticalX(self, direction_x, direction_y):
                    del direction_y
                    low_x, _, high_x, _ = self._bounds()
                    return low_x if direction_x < 0 else high_x if direction_x > 0 else self.centerX

                def criticalY(self, direction_x, direction_y):
                    del direction_x
                    _, low_y, _, high_y = self._bounds()
                    return low_y if direction_y < 0 else high_y if direction_y > 0 else self.centerY

            import _typed_geometry_test_support as geometry_test
            geometry_test.install_js_bridge(fake_js, LayoutHandle)
            original_ellipse = fake_js.noonAuthoringGeometryOptions.ellipse
            fake_js.noonAuthoringGeometryOptions.ellipse = staticmethod(
                lambda width, height: (
                    constructor_calls.append((float(width), float(height))),
                    original_ellipse(width, height),
                )[1]
            )
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_geometry
            import _manim_semantic_handles as handles
            handles.install()
            import _manim_shared_geometry
            _manim_shared_geometry.install()

            from noon import BLUE, Ellipse

            default_ellipse = Ellipse()
            assert constructor_calls == [(2.0, 1.0)]
            assert math.isclose(default_ellipse.width, 2.0)
            assert math.isclose(default_ellipse.height, 1.0)

            rotated = Ellipse(
                width=4.0,
                height=1.5,
                color=BLUE,
                scale=(0.5, 2.0),
                rotation=math.pi / 6.0,
            )
            rotated._current_raw = lambda: (_ for _ in ()).throw(
                AssertionError("typed Ellipse layout read a Python snapshot")
            )
            assert constructor_calls[-1] == (4.0, 1.5)
            assert rotated._semantic_handle.snapshot["transform"]["scale"] == {
                "x": 1.0,
                "y": 1.5,
            }
            assert math.isclose(rotated.width, 2.3289230112388193, abs_tol=1e-12)
            assert math.isclose(rotated.height, 2.8632927011928597, abs_tol=1e-12)
            """
        )
        env = os.environ.copy()
        existing = env.get("PYTHONPATH")
        env["PYTHONPATH"] = (
            str(PYTHON_ROOT)
            if not existing
            else str(PYTHON_ROOT) + os.pathsep + existing
        )
        result = subprocess.run(
            [sys.executable, "-c", source],
            cwd=PYTHON_ROOT,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(
            result.returncode,
            0,
            msg=f"isolated compatibility probe failed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
