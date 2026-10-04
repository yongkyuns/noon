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
            scene = LinearTransformationScene()
            scene.foreground_plane = object.__new__(_manim_compat.Group)
            scene.moving_vectors = []
            captured = {}
            scene.play = lambda *animations, **kwargs: captured.update(
                animations=animations, kwargs=kwargs
            )
            scene.apply_matrix([[0.0, 1.0], [1.0, 0.0]])
            assert calls == [([0.0, 1.0, 1.0, 0.0], 2, 2)]
            assert captured["kwargs"] == {"run_time": 3.0}
            animation, = captured["animations"]
            assert animation.anim_args == {"run_time": 3.0, "path_arc": 0.0}
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
