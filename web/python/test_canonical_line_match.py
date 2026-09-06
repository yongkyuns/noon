import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class CanonicalLineMatchTests(unittest.TestCase):
    def test_callback_local_line_is_identity_free_and_stages_transform_only(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            filter(None, (str(python_dir), env.get("PYTHONPATH")))
        )
        source = textwrap.dedent(
            """
            import json
            import sys
            import types
            from types import SimpleNamespace

            allocations = []

            class LineHandle:
                def __init__(self, start, end):
                    self.semanticSlot = len(allocations) + 10
                    self.semanticGeneration = 0
                    self.start = start
                    self.end = end
                    allocations.append(self)

            fake_js = types.ModuleType("js")
            fake_js.noonCreateAuthoringMobjectHandle = lambda snapshot: (_ for _ in ()).throw(
                AssertionError("generic snapshot construction is forbidden")
            )
            fake_js.noonCreateAuthoringLineHandle = lambda x1, y1, x2, y2: LineHandle(
                (x1, y1), (x2, y2)
            )
            fake_js.noonResolveAnimationOptions = lambda *args: None
            sys.modules["js"] = fake_js

            import _manim_compat as manim
            manim.install()
            import _manim_phase_b  # installs the complete compatibility Mobject surface
            import _manim_semantic_handles as handles
            handles.install()
            assert handles._create_line_handle is not None
            assert manim.Line.__init__ is handles._line_init
            import _manim_geometry  # installs match_points
            import _manim_updaters as updaters
            updaters.install()

            line = manim.Line((-1.0, 0.0), (1.0, 0.0))
            assert len(allocations) == 1, len(allocations)
            scene = manim.Scene()
            line._scene = scene
            line._object = SimpleNamespace(id=0)
            frame = {
                "time": 0.5,
                "delta_time": 0.25,
                "token": {"runtime": 1, "publication": {}, "sequence": 2},
                "objects": [{
                    "node": {"slot": line._semantic_handle.semanticSlot, "generation": 0},
                    "transform": {
                        "translation": {"x": 0.0, "y": 0.0},
                        "rotation": 0.0,
                        "scale": {"x": 1.0, "y": 1.0},
                    },
                    "style": {
                        "fill": None,
                        "stroke": {"red": 1.0, "green": 0.0, "blue": 0.0, "alpha": 1.0},
                        "stroke_width": 0.04,
                        "stroke_width_mode": "screen_space",
                        "stroke_join": "miter",
                        "stroke_cap": "butt",
                        "opacity": 1.0,
                    },
                    "bounds": {"min": {"x": -1.0, "y": 0.0}, "max": {"x": 1.0, "y": 0.0}},
                }],
            }

            class Operations:
                def callbackLineTarget(self, x1, y1, x2, y2):
                    return (x1, y1, x2, y2)

                def callbackMatchLineTransform(self, source, target):
                    assert source is line._semantic_handle
                    assert target == (2.0, 3.0, 4.0, 5.0)
                    return SimpleNamespace(
                        translationX=3.0,
                        translationY=4.0,
                        rotation=0.7853981633974483,
                        scaleX=1.4142135623730951,
                        scaleY=1.4142135623730951,
                    )

            context = updaters._CanonicalCallbackContext(frame, Operations())
            updaters._ACTIVE_CONTEXTS[id(scene)] = context
            token = updaters._ACTIVE_CANONICAL_CONTEXT.set(context)
            try:
                target = manim.Line((2.0, 3.0), (4.0, 5.0))
                assert len(allocations) == 1
                assert target._semantic_handle is None
                line.match_points(target)
            finally:
                updaters._ACTIVE_CANONICAL_CONTEXT.reset(token)
                updaters._ACTIVE_CONTEXTS.pop(id(scene), None)

            writes = context.effective_batch()["writes"]
            assert len(writes) == 1
            assert writes[0]["kind"] == "transform"
            assert writes[0]["transform"]["translation"] == {"x": 3.0, "y": 4.0}
            assert "style" not in writes[0]
            try:
                target.to_ir()
            except RuntimeError as error:
                assert "cannot escape" in str(error)
            else:
                raise AssertionError("callback-local Line escaped into snapshot APIs")
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
            f"canonical Line match subprocess failed:\n{completed.stdout}\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
