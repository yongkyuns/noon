import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimLayoutAlignmentTests(unittest.TestCase):
    def test_next_to_move_to_and_arrange_match_manim_semantics(self) -> None:
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
            import _manim_compat

            _manim_compat.install()
            import _manim_phase_b  # noqa: F401 - installs exact Manim layout bounds
            import _manim_semantic_handles  # noqa: F401 - installs shared placement

            from noon import DOWN, LEFT, Rectangle, RIGHT, Square, UP, UR, VGroup

            target = Square(2.0)
            diagonal = Square(2.0).next_to(target, UR, buff=0.25)
            assert abs(diagonal.get_center().x - 2.25) < 1e-12
            assert abs(diagonal.get_center().y - 2.25) < 1e-12

            reference = Rectangle(width=2.0, height=3.0)
            top_aligned = Square(1.0).next_to(
                reference, RIGHT, buff=0.4, aligned_edge=UP
            )
            assert abs(top_aligned.get_top().y - reference.get_top().y) < 1e-12
            assert abs(top_aligned.get_left().x - reference.get_right().x - 0.4) < 1e-12

            moved = Square(1.0).move_to(reference, aligned_edge=UP + LEFT)
            assert abs(moved.get_top().y - reference.get_top().y) < 1e-12
            assert abs(moved.get_left().x - reference.get_left().x) < 1e-12

            masked = Square(1.0).shift(DOWN * 2.0).next_to(
                reference,
                RIGHT,
                buff=0.5,
                aligned_edge=UP,
                coor_mask=(1.0, 0.0, 0.0),
            )
            assert abs(masked.get_center().y + 2.0) < 1e-12
            assert abs(masked.get_left().x - reference.get_right().x - 0.5) < 1e-12

            short = Rectangle(width=1.0, height=1.0)
            tall = Rectangle(width=1.0, height=3.0)
            wide = Rectangle(width=2.0, height=2.0)
            group = VGroup(short, tall, wide).arrange(
                RIGHT,
                buff=0.3,
                aligned_edge=UP,
                center=False,
            )
            assert group is not None
            assert abs(short.get_top().y - tall.get_top().y) < 1e-12
            assert abs(tall.get_top().y - wide.get_top().y) < 1e-12
            assert abs(tall.get_left().x - short.get_right().x - 0.3) < 1e-12
            assert abs(wide.get_left().x - tall.get_right().x - 0.3) < 1e-12
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
            f"compatibility subprocess failed:\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
