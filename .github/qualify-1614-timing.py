from pathlib import Path
import sys

TEST = r'''"""Exercise the production matching transport branch with an inert Rust-boundary double."""
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
'''

OLD = '''            source, target, leaf = family_transform
            child = _canonical_transform_options(
                leaf, child_kwargs, allow_family_lag=True
            )
            if child is None:
                raise NotImplementedError("unsupported canonical family Transform options")
            if child.rate_func not in ("linear", "smooth"):
                raise NotImplementedError(
                    "canonical family Transform requires linear or smooth easing"
                )
            append_family_transform = (
                builder.appendMatchingFamilyTransformTo
                if type(leaf) is _animate.TransformMatchingShapes
                else builder.appendFamilyTransformTo
            )
            append_family_transform(
                source._semantic_family_handle,
                target._semantic_family_handle,
                float(child.run_time),
                str(child.rate_func),
                float(child.lag_ratio),
                float(child.path_arc),
            )
            if type(leaf) is _animate.TransformMatchingShapes:
                completed_families.extend((source, target))
            return'''

NEW = '''            source, target, leaf = family_transform
            matching = type(leaf) is _animate.TransformMatchingShapes
            # Manim constructor options belong to the Transform/Fade children;
            # Scene.play options belong to their enclosing AnimationGroup.
            child = _canonical_transform_options(
                leaf, {} if matching else child_kwargs, allow_family_lag=True
            )
            if child is None:
                raise NotImplementedError("unsupported canonical family Transform options")
            if child.rate_func not in ("linear", "smooth"):
                raise NotImplementedError(
                    "canonical family Transform requires linear or smooth easing"
                )
            destination = builder
            if matching:
                if any(child_kwargs.get(name) not in (None, 0.0)
                       for name in ("lag_ratio", "path_arc")):
                    raise NotImplementedError(
                        "TransformMatchingShapes does not yet support play-level lag or path options"
                    )
                outer_kwargs = dict(child_kwargs)
                outer_kwargs.pop("path_arc", None)
                outer_duration = _canonical_play_options(outer_kwargs)
                destination = context.beginOrdinaryCompositionBuilder(
                    "parallel", outer_duration, 0.0, None,
                )
                destination.setCompositionRateFunction(
                    _canonical_composition_rate_id(child_kwargs) or "linear"
                )
            append_family_transform = (
                destination.appendMatchingFamilyTransformTo
                if matching else destination.appendFamilyTransformTo
            )
            append_family_transform(
                source._semantic_family_handle,
                target._semantic_family_handle,
                float(child.run_time),
                str(child.rate_func),
                float(child.lag_ratio),
                float(child.path_arc),
            )
            if matching:
                builder.appendComposition(destination)
                completed_families.extend((source, target))
            return'''

NATIVE = r'''

#[test]
fn matching_nested_rate_scopes_preserve_child_defaults_and_outer_duration() {
    use noon_core::{AnimationOptions, RateFunction, SemanticAnimationCompositionKind};
    // Independent values from ManimCE v0.21 smooth at alpha=.25 and smooth(smooth(.25)).
    for (inner, outer, progress) in [
        (RateFunction::Linear, RateFunction::Linear, 0.25),
        (RateFunction::Smooth, RateFunction::Linear, 0.07010371654510815),
        (RateFunction::Linear, RateFunction::Smooth, 0.07010371654510815),
        (RateFunction::Smooth, RateFunction::Smooth, 0.0067987788242606885),
    ] {
        let (mut context, source_leaf, source, target_leaf, target) = fixture();
        let end = context
            .begin_ordinary_mixed_composition(
                SemanticAnimationCompositionKind::Parallel,
                &[OrdinaryCompositionChild::Composition {
                    kind: SemanticAnimationCompositionKind::Parallel,
                    children: vec![OrdinaryCompositionChild::MatchingFamilyTransformTo {
                        source,
                        target_state: target,
                        options: AnimationOptions::new().run_time(3.0).rate_func(inner),
                    }],
                    options: AnimationOptions::new().run_time(2.0).rate_func(outer),
                }],
                AnimationOptions::new().rate_func(RateFunction::Linear),
                AnimationOptions::new(),
            )
            .unwrap();
        assert!((end - 2.0).abs() < 1e-9);
        context.active_live_player().unwrap().live_advance_segment_to(0.5).unwrap();
        let actual = context.mobject_layout(&source_leaf).unwrap().0;
        assert!((actual - (-2.0 + 6.0 * progress)).abs() < 1e-5,
                "{inner:?}/{outer:?}: {actual}");
        complete(&mut context, end);
        assert!(!context.contains_mobject(&source_leaf).unwrap());
        assert!(context.contains_mobject(&target_leaf).unwrap());
    }
}
'''

root = Path('.')
test = root / 'web/python/test_manim_matching_timing.py'
if not test.exists():
    test.write_text(TEST)
if '--test-only' in sys.argv:
    raise SystemExit(0)
scene = root / 'web/python/_manim_scene.py'
text = scene.read_text()
assert text.count(OLD) == 1, 'exact matching transport block changed'
scene.write_text(text.replace(OLD, NEW))
native = root / 'crates/noon-web/src/canonical_authoring_scene/completed_binding_tests.rs'
text = native.read_text()
assert 'matching_nested_rate_scopes' not in text
native.write_text(text + NATIVE)
