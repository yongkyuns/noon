import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimVectorSpaceTests(unittest.TestCase):
    def test_lts_rejects_unimplemented_coordinates_and_ghost_history(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            value for value in (str(python_dir), env.get("PYTHONPATH")) if value
        )
        source = textwrap.dedent(
            """
            from noon import LinearTransformationScene, VectorScene

            scene = LinearTransformationScene()
            assert isinstance(scene, VectorScene)
            assert scene.include_background_plane
            assert scene.include_foreground_plane
            assert scene.show_basis_vectors
            assert scene.moving_vectors == []

            for options in ({"show_coordinates": True}, {"leave_ghost_vectors": True}):
                try:
                    LinearTransformationScene(**options)
                except NotImplementedError:
                    pass
                else:
                    raise AssertionError(f"unsupported option was silently accepted: {options}")
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source], cwd=python_dir, env=env,
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(
            completed.returncode, 0,
            msg=f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )

    def test_lts_apply_matrix_uses_rust_path_arc_and_manim_three_second_default(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            value for value in (str(python_dir), env.get("PYTHONPATH")) if value
        )
        source = textwrap.dedent(
            """
            import sys
            import types

            calls = []
            fake_js = types.ModuleType("js")
            def path_arc(values, rows, columns):
                calls.append((list(values), rows, columns))
                return 0.0
            fake_js.noonLinearTransformationPathArc = path_arc
            sys.modules["js"] = fake_js

            from noon import LinearTransformationScene
            import _manim_compat
            import _manim_animate
            import _manim_arrow

            class Shaft:
                def get_stroke_width(self):
                    return 8.5

            source_arrow_type = _manim_arrow.Arrow
            source_arrow = object.__new__(source_arrow_type)
            source_arrow._shaft = Shaft()
            assert source_arrow.get_stroke_width() == 8.5

            class Aggregate:
                def matrixTransformedEndpoints(self, values, rows, columns, x, y):
                    return (0.0, 0.0, 1.0, 1.0)

            source_arrow._semantic_arrow_handle = Aggregate()
            source_arrow.get_color = lambda: "yellow"

            target_arrows = []
            class TargetArrow:
                def __init__(self, *args, **kwargs):
                    self.args = args
                    self.kwargs = kwargs
                    target_arrows.append(self)

            _manim_arrow.Arrow = TargetArrow

            class Animation:
                def __init__(self, *args, **kwargs):
                    self.args = args
                    self.anim_args = kwargs

            _manim_animate.ApplyMatrix = Animation
            _manim_animate.Transform = Animation

            scene = LinearTransformationScene()
            scene.foreground_plane = object.__new__(_manim_compat.Group)
            scene.moving_vectors = [source_arrow]
            captured = {}
            scene.play = lambda *animations, **kwargs: captured.update(
                animations=animations, kwargs=kwargs
            )
            scene.apply_matrix([[0.0, 1.0], [1.0, 0.0]])
            assert calls == [([0.0, 1.0, 1.0, 0.0], 2, 2)]
            assert captured["kwargs"] == {"run_time": 3.0}
            plane_animation, vector_animation = captured["animations"]
            assert plane_animation.anim_args["about_point"] == (0.0, 0.0)
            assert plane_animation.anim_args["run_time"] == 3.0
            assert plane_animation.anim_args["path_arc"] == 0.0
            assert vector_animation.anim_args == {"run_time": 3.0, "path_arc": 0.0}
            assert target_arrows[0].args == ((0.0, 0.0), (1.0, 1.0))
            assert target_arrows[0].kwargs["stroke_width"] == 8.5
            assert vector_animation.args == (source_arrow, target_arrows[0])
            # The flattened length is four, but these are not two valid rows.
            # Reject before querying Rust or preparing any target objects.
            try:
                scene.apply_matrix([[1.0, 2.0, 3.0], [4.0]])
            except ValueError as error:
                assert "equal lengths" in str(error)
            else:
                raise AssertionError("ragged matrix was silently reinterpreted")
            assert calls == [([0.0, 1.0, 1.0, 0.0], 2, 2)]
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source], cwd=python_dir, env=env,
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(
            completed.returncode, 0,
            msg=f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
