import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class CompositionContinuationTests(unittest.TestCase):
    def test_active_continuation_begins_composition_without_endpoint_helper(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        existing = env.get("PYTHONPATH")
        env["PYTHONPATH"] = (
            str(python_dir)
            if not existing
            else os.pathsep.join((str(python_dir), existing))
        )
        source = textwrap.dedent(
            r'''
            import sys
            import types

            fake_js = types.ModuleType("js")
            fake_js.__getattr__ = lambda _name: (lambda *args, **kwargs: None)
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_rate_functions
            _manim_rate_functions.install()
            import _manim_phase_b
            import _manim_geometry
            import _manim_semantic_handles
            _manim_semantic_handles.install()
            import _manim_shared_geometry
            _manim_shared_geometry.install()
            import _manim_dashed_line
            import _manim_rotate
            _manim_rotate.install()
            import _manim_composition
            _manim_composition.install()
            import _manim_lifecycle
            import _manim_typst
            _manim_typst.install()
            import _manim_retained_animate
            _manim_retained_animate.install()
            import _manim_retained_state
            _manim_retained_state.install()
            import _manim_growing
            _manim_growing.install()
            import _manim_draw_border_then_fill
            _manim_draw_border_then_fill.install()
            import _manim_indication
            import _manim_reactive
            import _manim_updaters
            _manim_updaters.install()
            import _manim_camera
            _manim_camera.install()
            import _manim_canonical_scene as canonical

            candidate = object()
            scene = types.SimpleNamespace(_legacy_geometry_materialized=False)
            calls = []
            context = types.SimpleNamespace(
                beginOrdinaryComposition=lambda value: calls.append(("begin", value)),
                ordinaryPlayComposition=lambda value: calls.append(("endpoint", value)),
            )
            canonical._context = lambda _scene: context
            canonical._legacy_authored_time = lambda _scene: 0.0
            canonical._require_semantic_continuation_active = lambda _scene: calls.append(("require", None))
            canonical._prepare_semantic_continuation_callbacks = lambda _scene, _context: calls.append(("callbacks", None))
            canonical._synchronous_continuation_wait = lambda _scene: calls.append(("wait", None)) or _scene
            canonical._async_continuation_active = lambda _scene: False
            canonical._synchronous_continuation_active = lambda _scene: True
            assert canonical._play_canonical_composition(scene, candidate) is scene
            assert calls == [
                ("require", None),
                ("callbacks", None),
                ("begin", candidate),
                ("wait", None),
            ]

            calls.clear()
            canonical._synchronous_continuation_active = lambda _scene: False
            assert canonical._play_canonical_composition(scene, candidate) is scene
            assert calls == [("endpoint", candidate)]
            '''
        )
        completed = subprocess.run(
            [sys.executable, "-c", source],
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)


if __name__ == "__main__":
    unittest.main()
