import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimCyclicReplaceAdapterTests(unittest.TestCase):
    def test_variadic_and_single_group_lower_to_parallel_curved_transforms(self) -> None:
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
            import math
            import sys
            import types

            noon = types.ModuleType("noon")
            noon.PI = math.pi

            class Mobject:
                def __init__(self, center):
                    self.center = tuple(center)
                def copy(self):
                    return Mobject(self.center)
                def move_to(self, other):
                    self.center = tuple(other.center if isinstance(other, Mobject) else other)
                    return self

            noon.Mobject = Mobject
            sys.modules["noon"] = noon

            compat = types.ModuleType("_manim_compat")
            class Group:
                def __init__(self, *members):
                    self.submobjects = list(members)
                def __iter__(self):
                    return iter(self.submobjects)
            compat.Group = Group
            sys.modules["_manim_compat"] = compat

            rates = types.ModuleType("_manim_rate_functions")
            rates.linear = object()
            rates.smooth = object()
            sys.modules["_manim_rate_functions"] = rates

            animate = types.ModuleType("_manim_animate")
            class Transform:
                def __init__(self, source, target, **kwargs):
                    self.source = source
                    self.target = target
                    self.anim_args = dict(kwargs)
            animate.Transform = Transform
            sys.modules["_manim_animate"] = animate

            composition = types.ModuleType("_manim_composition")
            class AnimationGroup:
                def __init__(self, *animations, **kwargs):
                    self.animations = list(animations)
                    self.group_args = dict(kwargs)
            composition.AnimationGroup = AnimationGroup
            sys.modules["_manim_composition"] = composition

            from _manim_cyclic_replace import CyclicReplace, Swap

            a = Mobject((-2.0, 0.0))
            b = Mobject((0.0, 1.0))
            c = Mobject((3.0, -1.0))
            animation = CyclicReplace(a, b, c, path_arc=-0.75, run_time=2.5)
            assert [item.target.center for item in animation.animations] == [
                b.center, c.center, a.center
            ]
            assert all(item.anim_args["path_arc"] == -0.75 for item in animation.animations)
            assert all(item.anim_args["rate_func"] is rates.linear for item in animation.animations)
            assert animation.group_args["run_time"] == 2.5
            assert animation.group_args["rate_func"] is rates.smooth
            assert animation.group_args["lag_ratio"] == 0.0

            grouped = CyclicReplace(Group(a, b, c))
            assert [item.target.center for item in grouped.animations] == [
                b.center, c.center, a.center
            ]
            swapped = Swap(a, b)
            assert [item.target.center for item in swapped.animations] == [b.center, a.center]

            try:
                CyclicReplace(a, b, path_arc=float("nan"))
            except ValueError:
                pass
            else:
                raise AssertionError("non-finite path_arc must fail")

            try:
                CyclicReplace(Group(a, b), c)
            except NotImplementedError:
                pass
            else:
                raise AssertionError("nested groups must fail closed")
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
