"""Test the fixture's exact identity assertion, not Manim or Noon rendering."""
import ast
from pathlib import Path
from types import SimpleNamespace
import unittest


class Mobject:
    def __init__(self, points=0, children=()):
        self.points = points
        self.submobjects = children
        self.updaters = []

    def get_num_points(self):
        return self.points


class Drawable(Mobject):
    pass


class VGroup(Mobject):
    pass


class FadeOut:
    def __init__(self, mobject):
        self.mobject = mobject


class ReferenceMatching:
    __module__ = "manim.animation.transform_matching_parts"

    def __init__(self, empty):
        self.animations = [object(), FadeOut(empty), object()]


source = Path(__file__).resolve().parent.parent / "parity/manim-v0.21/core-examples/foreground_matching.py"
module = ast.parse(source.read_text())
helpers = [node for node in module.body if isinstance(node, ast.FunctionDef)
           and node.name in ("assert_membership", "reference_empty_fade_source")]
namespace = {"Mobject": Mobject, "VGroup": VGroup, "FadeOut": FadeOut,
             "TransformMatchingShapes": ReferenceMatching}
exec(compile(ast.Module(body=helpers, type_ignores=[]), str(source), "exec"), namespace)
assert_membership = namespace["assert_membership"]
reference_empty_fade_source = namespace["reference_empty_fade_source"]


class MembershipTests(unittest.TestCase):
    def setUp(self):
        self.source, self.left, self.right = (Drawable() for _ in range(3))
        self.display = [self.source, self.left, self.right]
        self.foreground = [self.left, self.right]

    def check(self, roots, *, declarations=None, just_declared=False, empty_fade_source=None):
        scene = SimpleNamespace(mobjects=roots, foreground_mobjects=self.foreground if declarations is None else declarations)
        assert_membership(scene, self.display, self.foreground, just_declared=just_declared,
                          empty_fade_source=empty_fade_source)

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


    def test_only_pre_recorded_empty_reference_source_is_accepted(self):
        empty = VGroup()
        matching = ReferenceMatching(empty)
        self.assertIs(reference_empty_fade_source(matching), empty)
        # No debris is needed by Noon; Cairo may retain exactly this one root.
        for roots in (self.display, [empty] + self.display,
                      [empty] + self.display + [Mobject()]):
            self.check(roots, empty_fade_source=empty)
        with self.assertRaises(AssertionError):
            self.check([empty] + self.display)

    def test_unrelated_repeated_or_reordered_empty_roots_still_fail(self):
        empty = VGroup()
        for roots in ([VGroup()] + self.display, [empty, empty] + self.display,
                      self.display + [empty], [empty] + self.display[::-1],
                      [empty] + self.display + self.foreground,
                      [empty, VGroup()] + self.display):
            with self.subTest(roots=roots), self.assertRaises(AssertionError):
                self.check(roots, empty_fade_source=empty)

    def test_reference_allowance_never_hides_authored_identity_or_declaration_errors(self):
        empty = VGroup()
        for roots in ([empty] + self.display[1:], [empty, Drawable(), self.left, self.right]):
            with self.subTest(roots=roots), self.assertRaises(AssertionError):
                self.check(roots, empty_fade_source=empty)
        with self.assertRaises(AssertionError):
            self.check([empty] + self.display, declarations=self.foreground[::-1],
                       empty_fade_source=empty)
        with self.assertRaises(AssertionError):
            self.check([empty] + self.display, just_declared=True, empty_fade_source=empty)

    def test_reference_source_must_remain_exactly_inert(self):
        for empty in (VGroup(points=1), VGroup(children=[Drawable()]), Drawable()):
            with self.subTest(empty=empty), self.assertRaises(AssertionError):
                self.check([empty] + self.display, empty_fade_source=empty)
        empty = VGroup()
        empty.updaters.append(lambda _: None)
        with self.assertRaises(AssertionError):
            self.check([empty] + self.display, empty_fade_source=empty)

    def test_reference_origin_is_checked_before_any_allowance(self):
        class NoonMatching:
            pass
        self.assertIsNone(reference_empty_fade_source(NoonMatching()))
        for empty in (VGroup(points=1), VGroup(children=[Drawable()]), Drawable()):
            with self.subTest(empty=empty), self.assertRaises(AssertionError):
                reference_empty_fade_source(ReferenceMatching(empty))
        wrong_child = ReferenceMatching(VGroup())
        wrong_child.animations[1] = SimpleNamespace(mobject=VGroup())
        with self.assertRaises(AssertionError):
            reference_empty_fade_source(wrong_child)
        missing_child = ReferenceMatching(VGroup())
        missing_child.animations.pop()
        with self.assertRaises(AssertionError):
            reference_empty_fade_source(missing_child)

    def test_fixture_records_reference_identity_before_play_and_checks_every_boundary(self):
        exercise = next(node for node in module.body if isinstance(node, ast.FunctionDef)
                        and node.name == "exercise")
        calls = [node.value for node in exercise.body if isinstance(node, ast.Expr)
                 and isinstance(node.value, ast.Call)]
        plays = [call for call in calls if isinstance(call.func, ast.Attribute)
                 and call.func.attr == "play"]
        self.assertEqual(len(plays), 1)
        self.assertEqual(ast.unparse(plays[0].args[0]), "matching")
        record = next(node for node in exercise.body if isinstance(node, ast.Assign)
                      and any(isinstance(target, ast.Name) and target.id == "empty_fade_source"
                              for target in node.targets))
        self.assertEqual(ast.unparse(record.value), "reference_empty_fade_source(matching)")
        self.assertLess(record.lineno, plays[0].lineno)
        postchecks = [call for call in calls if isinstance(call.func, ast.Name)
                      and call.func.id == "assert_membership" and call.lineno > plays[0].lineno]
        self.assertEqual(len(postchecks), 4)
        for call in postchecks:
            self.assertEqual({kw.arg: ast.unparse(kw.value) for kw in call.keywords},
                             {"empty_fade_source": "empty_fade_source"})


if __name__ == "__main__":
    unittest.main()
