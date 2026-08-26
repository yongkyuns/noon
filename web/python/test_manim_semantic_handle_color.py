import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSemanticHandleColorTests(unittest.TestCase):
    def test_worker_bootstrap_preserves_independent_opacity_in_set_color(self) -> None:
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
            import sys
            import types

            fake_js = types.ModuleType("js")

            class FakeSemanticHandle:
                def __init__(self, snapshot_json):
                    self.snapshot = json.loads(snapshot_json)
                    self.broad_set_color_calls = 0

                def snapshotJson(self):
                    return json.dumps(self.snapshot, separators=(",", ":"))

                def replaceSnapshotJson(self, snapshot_json):
                    self.snapshot = json.loads(snapshot_json)

                def setFillOpacity(self, opacity):
                    if self.snapshot["style"]["fill"] is None:
                        self.snapshot["style"]["fill"] = {
                            "red": 1.0,
                            "green": 1.0,
                            "blue": 1.0,
                            "alpha": float(opacity),
                        }
                    else:
                        self.snapshot["style"]["fill"]["alpha"] = float(opacity)

                def setStrokeOpacity(self, opacity):
                    if self.snapshot["style"]["stroke"] is None:
                        self.snapshot["style"]["stroke"] = {
                            "red": 1.0,
                            "green": 1.0,
                            "blue": 1.0,
                            "alpha": float(opacity),
                        }
                    else:
                        self.snapshot["style"]["stroke"]["alpha"] = float(opacity)

                def setColor(self, red, green, blue, alpha):
                    # Mirrors the broad Rust handle operation that exposed the regression:
                    # one alpha is applied to both channels.
                    self.broad_set_color_calls += 1
                    for channel in ("fill", "stroke"):
                        if self.snapshot["style"][channel] is not None:
                            self.snapshot["style"][channel] = {
                                "red": float(red),
                                "green": float(green),
                                "blue": float(blue),
                                "alpha": float(alpha),
                            }

                def setFillColor(self, red, green, blue, alpha):
                    current = self.snapshot["style"]["fill"]
                    opacity = float(alpha) if current is None else float(current["alpha"])
                    self.snapshot["style"]["fill"] = {
                        "red": float(red),
                        "green": float(green),
                        "blue": float(blue),
                        "alpha": opacity,
                    }

                def setStrokeColor(self, red, green, blue, alpha):
                    current = self.snapshot["style"]["stroke"]
                    opacity = float(alpha) if current is None else float(current["alpha"])
                    self.snapshot["style"]["stroke"] = {
                        "red": float(red),
                        "green": float(green),
                        "blue": float(blue),
                        "alpha": opacity,
                    }

            def create_handle(snapshot_json):
                return FakeSemanticHandle(snapshot_json)

            fake_js.noonCreateAuthoringMobjectHandle = create_handle
            sys.modules["js"] = fake_js

            # Match the relevant python-worker bootstrap order exactly: the rate-function
            # adapter installs first, then semantic handles take ownership of detached
            # authoring objects.
            import _manim_compat
            _manim_compat.install()
            import _manim_rate_functions
            _manim_rate_functions.install()
            import _manim_phase_b  # noqa: F401
            import _manim_semantic_handles
            _manim_semantic_handles.install()

            import _noon_ir as _ir
            import noon as _base

            raw = _ir.Mobject(
                geometry={"rectangle": {"size": {"x": 1.0, "y": 1.0}}},
                transform={
                    "translation": {"x": 0.0, "y": 0.0},
                    "scale": {"x": 1.0, "y": 1.0},
                    "rotation": 0.0,
                },
                style={
                    "fill": {"red": 0.0, "green": 0.0, "blue": 1.0, "alpha": 0.35},
                    "stroke": {"red": 0.0, "green": 0.0, "blue": 1.0, "alpha": 0.0},
                    "stroke_width": 4.0,
                    "opacity": 1.0,
                },
            )
            mobject = _base.Mobject(raw)
            handle = mobject._semantic_handle
            mobject.set_color(_base.GREEN)

            style = mobject.style
            assert handle.broad_set_color_calls == 0
            assert abs(style["fill"]["red"] - _base.GREEN.red) < 1e-12
            assert abs(style["fill"]["green"] - _base.GREEN.green) < 1e-12
            assert abs(style["fill"]["blue"] - _base.GREEN.blue) < 1e-12
            assert abs(style["fill"]["alpha"] - 0.35) < 1e-12
            assert abs(style["stroke"]["red"] - _base.GREEN.red) < 1e-12
            assert abs(style["stroke"]["green"] - _base.GREEN.green) < 1e-12
            assert abs(style["stroke"]["blue"] - _base.GREEN.blue) < 1e-12
            assert abs(style["stroke"]["alpha"] - 0.0) < 1e-12

            # Preserve the base Mobject fallback: if neither channel exists, set_color
            # creates a fill using the requested color alpha.
            empty = _base.Mobject(
                _ir.Mobject(
                    geometry={"rectangle": {"size": {"x": 1.0, "y": 1.0}}},
                    transform={
                        "translation": {"x": 0.0, "y": 0.0},
                        "scale": {"x": 1.0, "y": 1.0},
                        "rotation": 0.0,
                    },
                    style={
                        "fill": None,
                        "stroke": None,
                        "stroke_width": 0.0,
                        "opacity": 1.0,
                    },
                )
            )
            empty.set_color(_base.GREEN)
            empty_style = empty.style
            assert empty_style["fill"] is not None
            assert abs(empty_style["fill"]["alpha"] - _base.GREEN.alpha) < 1e-12
            assert empty_style["stroke"] is None
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
            f"worker-order compatibility subprocess failed:\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
