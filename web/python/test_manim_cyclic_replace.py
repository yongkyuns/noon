import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimCyclicReplaceTests(unittest.TestCase):
    def test_requests_are_inert_and_normalize_call_shapes(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            part for part in (str(python_dir), env.get("PYTHONPATH")) if part
        )
        source = textwrap.dedent(
            """
            import sys
            import types

            fake_js = types.ModuleType("js")
            fake_js.noonResolveAnimationOptions = object()
            fake_js.noonResolveTransformAnimationOptions = object()
            sys.modules["js"] = fake_js

            import noon
            import _manim_compat as compat
            import _manim_semantic_handles as semantic_handles
            from _manim_cyclic_replace import CyclicReplace, Swap

            a = object.__new__(noon.Mobject)
            b = object.__new__(noon.Mobject)
            c = object.__new__(noon.Mobject)

            swap = Swap(a, b)
            assert swap.mobjects == (a, b)
            assert swap.anim_args == {"path_arc": noon.PI / 2.0}

            cyclic = CyclicReplace(a, b, c, path_arc=-0.75, run_time=2.0)
            assert cyclic.mobjects == (a, b, c)
            assert cyclic.anim_args == {"run_time": 2.0, "path_arc": -0.75}

            group = object.__new__(compat.Group)
            group._semantic_family_handle = types.SimpleNamespace(memberCount=3)
            semantic_handles._group_members = lambda value: [a, b, c] if value is group else []
            grouped = CyclicReplace(group)
            assert grouped.mobjects == (a, b, c)
            assert grouped.group is group

            try:
                CyclicReplace(a)
            except ValueError:
                pass
            else:
                raise AssertionError("single leaf must fail")

            try:
                CyclicReplace(a, b, path_arc=float("nan"))
            except ValueError:
                pass
            else:
                raise AssertionError("non-finite arc must fail")

            nested = object.__new__(compat.Group)
            try:
                CyclicReplace(a, nested)
            except NotImplementedError:
                pass
            else:
                raise AssertionError("nested groups must fail closed")
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
            f"CyclicReplace request subprocess failed:\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
