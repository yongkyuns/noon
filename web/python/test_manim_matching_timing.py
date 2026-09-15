"""Exercise the production matching transport branch with an inert Rust-boundary double."""
import ast
import math
from pathlib import Path
from types import SimpleNamespace as NS
import unittest


class Builder:
    def __init__(self, duration=None, lag=0.0):
        self.duration, self.lag, self.rate = duration, lag, None
        self.children = []

    def setCompositionRateFunction(self, rate):
        self.rate = rate

    def appendComposition(self, child):
        self.children.append(child)

    def appendMatchingFamilyTransformTo(self, *args):
        self.children.append(("matching", args))

    def appendFamilyTransformTo(self, *args):
        self.children.append(("structural", args))


class Matching:
    def __init__(self, **kwargs):
        self.anim_args = kwargs


class MatchingTimingScopeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        path = Path(__file__).with_name("_manim_scene.py")
        tree = ast.parse(path.read_text())
        candidate = next(n for n in tree.body if isinstance(n, ast.FunctionDef)
                         and n.name == "_build_canonical_composition_candidate")
        branch = next(n for n in ast.walk(candidate) if isinstance(n, ast.If)
                      and ast.unparse(n.test) == "family_transform is not None")
        params = "context builder family_transform child_kwargs completed_families".split()
        route = ast.FunctionDef(name="route", args=ast.arguments(
            posonlyargs=[], args=[ast.arg(arg=p) for p in params], kwonlyargs=[],
            kw_defaults=[], defaults=[]), body=branch.body, decorator_list=[])
        helpers = [n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name in {
            "_canonical_play_options", "_canonical_composition_rate_id"}]
        assert len(helpers) == 2
        module = ast.Module(body=helpers + [route], type_ignores=[])
        cls.module = ast.fix_missing_locations(module)

    def route(self, constructor=None, play=None, matching=True):
        self.resolutions = []
        def resolve(leaf, kwargs, **unused):
            self.resolutions.append(dict(kwargs))
            opts = dict(leaf.anim_args)
            opts.update(kwargs)
            return NS(run_time=opts.get("run_time", opts.get("duration", 1.0)),
                      rate_func=opts.get("rate_func", opts.get("easing", "smooth")),
                      lag_ratio=opts.get("lag_ratio", 0.0), path_arc=opts.get("path_arc", 0.0))
        namespace = {"_animate": NS(TransformMatchingShapes=Matching),
                     "_canonical_transform_options": resolve, "math": math,
                     "_rate_functions": NS(easing_from_rate_func=lambda value: value)}
        exec(compile(self.module, "_manim_scene.py:matching-transport", "exec"), namespace)
        context = NS(beginOrdinaryCompositionBuilder=lambda kind, duration, lag, play_duration:
                     Builder(duration, lag))
        leaf = Matching(**(constructor or {})) if matching else NS(anim_args=constructor or {})
        source, target = (NS(_semantic_family_handle=object()) for _ in range(2))
        parent, completed = Builder(), []
        namespace["route"](context, parent, (source, target, leaf), play or {}, completed)
        return parent, completed

    def test_constructor_child_and_play_group_keep_distinct_rates_and_durations(self):
        cases = [({}, {}, "smooth", "linear", 1.0, None),
                 ({"rate_func": "linear"}, {}, "linear", "linear", 1.0, None),
                 ({}, {"rate_func": "linear", "run_time": 2}, "smooth", "linear", 1.0, 2),
                 ({"rate_func": "linear", "run_time": 3},
                  {"rate_func": "smooth", "run_time": 2}, "linear", "smooth", 3, 2),
                 ({}, {"rate_func": "smooth"}, "smooth", "smooth", 1.0, None)]
        for ctor, play, child_rate, outer_rate, child_duration, outer_duration in cases:
            with self.subTest(constructor=ctor, play=play):
                parent, completed = self.route(ctor, play)
                outer = parent.children[0]
                self.assertIsInstance(outer, Builder)
                self.assertEqual((outer.duration, outer.lag, outer.rate),
                                 (outer_duration, 0.0, outer_rate))
                tag, args = outer.children[0]
                self.assertEqual((tag, args[2], args[3]), ("matching", child_duration, child_rate))
                self.assertEqual(self.resolutions, [{}])
                self.assertEqual(len(completed), 2)

    def test_constructor_family_lag_and_path_remain_child_options(self):
        parent, _ = self.route({"lag_ratio": 0.2, "path_arc": 1.5})
        self.assertEqual(parent.children[0].children[0][1][4:], (0.2, 1.5))
        self.assertEqual(parent.children[0].lag, 0.0)

    def test_unsupported_outer_lag_or_path_is_not_applied_to_matching_children(self):
        for option in ("lag_ratio", "path_arc"):
            with self.subTest(option=option), self.assertRaises(NotImplementedError):
                self.route(play={option: 0.2})

    def test_conflicting_outer_duration_aliases_fail(self):
        with self.assertRaises(ValueError):
            self.route(play={"duration": 1, "run_time": 2})

    def test_structural_transform_transport_is_unchanged(self):
        parent, completed = self.route({"rate_func": "smooth"},
                                       {"rate_func": "linear", "run_time": 2}, False)
        tag, args = parent.children[0]
        self.assertEqual((tag, args[2], args[3]), ("structural", 2, "linear"))
        self.assertEqual(completed, [])
        self.assertEqual(self.resolutions, [{"rate_func": "linear", "run_time": 2}])


if __name__ == "__main__":
    unittest.main()
