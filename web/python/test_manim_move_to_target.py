import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimMoveToTargetTests(unittest.TestCase):
    def test_exact_example_preserves_target_transform_contract(self) -> None:
        python_dir = Path(__file__).resolve().parent
        repo_root = python_dir.parent.parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            part for part in (str(python_dir), env.get("PYTHONPATH", "")) if part
        )
        source = textwrap.dedent(
            f"""
            import runpy
            import _manim_compat; _manim_compat.install()
            import _manim_rate_functions; _manim_rate_functions.install()
            from noon import Circle, MoveToTarget, RIGHT, Scene, Transform, UP, VGroup

            missing = Circle()
            try:
                MoveToTarget(missing)
                raise AssertionError("missing target must fail")
            except ValueError as error:
                assert str(error) == "MoveToTarget called on mobject without attribute 'target'"

            group = VGroup(Circle(), Circle())
            group.generate_target = lambda: None
            try:
                MoveToTarget(group)
                raise AssertionError("group target must fail")
            except NotImplementedError:
                pass

            namespace = runpy.run_path({str(repo_root / "web/python/examples/manim_example_move_to_target.py")!r})
            scene = namespace["MoveToTargetExample"]()
            scene.construct()
            assert abs(scene.time - 1.0) < 1e-12
            tracks = [t for t in scene._tracks if t["property"] == "transform"]
            assert len(tracks) == 1
            assert abs(tracks[0]["timing"]["duration"] - 1.0) < 1e-12
            target = tracks[0]["values"]["object"]["to"]
            assert abs(target["transform"]["translation"]["x"] - 2.0) < 1e-12
            assert abs(target["transform"]["translation"]["y"] - 1.0) < 1e-12
            assert abs(target["transform"]["scale"]["x"] - 0.5) < 1e-12

            c = Circle(); c.generate_target(); pending = MoveToTarget(c); c.target.shift(RIGHT)
            scene2 = Scene(); scene2.add(c); scene2.play(pending)
            target2 = [t for t in scene2._tracks if t["property"] == "transform"][0]["values"]["object"]["to"]
            assert abs(target2["transform"]["translation"]["x"] - 1.0) < 1e-12

            # Canonical installation supplies this factory. `generate_target` must
            # select it rather than Python's ordinary `copy`, so MoveToTarget
            # receives the opaque target-editor result.
            class CanonicalSource:
                def __init__(self):
                    self.calls = []
                def _copy_for_animate_target(self):
                    self.calls.append("target-editor")
                    return object()
                def copy(self):
                    raise AssertionError("generate_target must not use raw copy")

            canonical = CanonicalSource()
            captured = _manim_compat._mobject_generate_target(canonical)
            assert captured is canonical.target
            assert canonical.calls == ["target-editor"]

            # Unsupported subclasses reuse the aligned builder only for option
            # parsing. The canonical affine classifier must reject them before
            # asking their lazy target property to materialize (ShrinkToCenter
            # on Text is one such caller).
            import sys
            import types
            js = types.ModuleType("js")
            for name in (
                "noonResolveAnimationOptions",
                "noonResolveUniformCompositionSchedule",
                "noonResolveLifecyclePlan",
                "noonValidatePresenceTransition",
                "noonResolveCompositionSchedule",
            ):
                setattr(js, name, lambda *args: None)
            sys.modules["js"] = js
            import _manim_phase_b
            import _manim_geometry
            import _manim_semantic_handles; _manim_semantic_handles.install()
            import _manim_shared_geometry; _manim_shared_geometry.install()
            import _manim_dashed_line; _manim_dashed_line.install()
            import _manim_animate
            import _manim_rotate; _manim_rotate.install()
            import _manim_composition; _manim_composition.install()
            import _manim_lifecycle
            import _manim_typst; _manim_typst.install()
            import _manim_retained_animate; _manim_retained_animate.install()
            import _manim_retained_state; _manim_retained_state.install()
            import _manim_growing; _manim_growing.install()
            import _manim_draw_border_then_fill; _manim_draw_border_then_fill.install()
            import _manim_indication; _manim_indication.install()
            import _manim_reactive
            import _manim_updaters; _manim_updaters.install()
            import _manim_camera; _manim_camera.install()
            import _manim_canonical_scene

            class DeferredUnsupported(_manim_animate._AlignedAnimationBuilder):
                @property
                def target(self):
                    raise AssertionError("unsupported target must stay inert")

            deferred = object.__new__(DeferredUnsupported)
            deferred.source = Circle()
            assert _manim_canonical_scene._canonical_affine_animation(
                Scene(), deferred
            ) is None
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source],
            cwd=repo_root,
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)


if __name__ == "__main__":
    unittest.main()
