"""Test the fixture's exact identity assertion, not Manim or Noon rendering."""
import ast
from pathlib import Path
from types import SimpleNamespace
import unittest


class Mobject:
    def __init__(self, points=0, children=()):
        self.points = points
        self.submobjects = children

    def get_num_points(self):
        return self.points


class Drawable(Mobject):
    pass


source = Path(__file__).resolve().parent.parent / "parity/manim-v0.21/core-examples/foreground_matching.py"
module = ast.parse(source.read_text())
helper = next(node for node in module.body if isinstance(node, ast.FunctionDef) and node.name == "assert_membership")
namespace = {"Mobject": Mobject}
exec(compile(ast.Module(body=[helper], type_ignores=[]), str(source), "exec"), namespace)
assert_membership = namespace["assert_membership"]


class MembershipTests(unittest.TestCase):
    def setUp(self):
        self.source, self.left, self.right = (Drawable() for _ in range(3))
        self.display = [self.source, self.left, self.right]
        self.foreground = [self.left, self.right]

    def check(self, roots, *, declarations=None, just_declared=False):
        scene = SimpleNamespace(mobjects=roots, foreground_mobjects=self.foreground if declarations is None else declarations)
        assert_membership(scene, self.display, self.foreground, just_declared=just_declared)

    def test_unique_roots_at_every_stage(self):
        for initial in (False, True):
            self.check(self.display, just_declared=initial)

    def test_exact_cairo_initial_suffix(self):
        self.check(self.display + self.foreground, just_declared=True)
        self.display = [self.left, self.source, self.right]
        self.foreground = list(self.display)
        self.check(self.display + self.foreground, just_declared=True)

    def test_initial_suffix_is_rejected_after_declaration(self):
        with self.assertRaises(AssertionError):
            self.check(self.display + self.foreground)

    def test_no_general_deduplication_or_extra_roots(self):
        for roots in (self.display + [self.source], self.display + [self.left],
                      self.display + self.foreground[::-1], self.display + self.foreground * 2,
                      self.display + [Drawable()], self.display[1:]):
            with self.subTest(roots=roots), self.assertRaises(AssertionError):
                self.check(roots, just_declared=True)

    def test_order_and_identity_stay_exact(self):
        for roots in (self.display[::-1], [Drawable(), self.left, self.right]):
            with self.subTest(roots=roots), self.assertRaises(AssertionError):
                self.check(roots, just_declared=True)

    def test_foreground_declarations_stay_exact(self):
        for declarations in ([], self.foreground[::-1], self.foreground * 2,
                             [Drawable(), self.right]):
            with self.subTest(declarations=declarations), self.assertRaises(AssertionError):
                self.check(self.display, declarations=declarations, just_declared=True)

    def test_only_inert_plain_wait_placeholder_is_excluded(self):
        self.check(self.display + [Mobject()])
        for extra in (Mobject(points=1), Mobject(children=[Drawable()]), Drawable()):
            with self.subTest(extra=extra), self.assertRaises(AssertionError):
                self.check(self.display + [extra])


if __name__ == "__main__":
    unittest.main()
