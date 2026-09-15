"""Exercise production request construction with an inert Rust-builder boundary.

Native completed_binding_tests evaluates the same nested tree with real Rust
scheduling. This test checks argument scopes, not an imitation interpolation loop.
"""
import __future__
import ast
from pathlib import Path
from types import SimpleNamespace
import unittest

import _manim_animate as animate
import _manim_composition as composition
import _manim_rate_functions as rates


class Group:
    def __init__(self, scene=None):
        self._semantic_family_handle = object()
        self.submobjects = [SimpleNamespace(_scene=scene)]


class Builder:
    def __init__(self, kind, run_time, lag_ratio, play_run_time):
        self.kind, self.run_time = kind, run_time
        self.lag_ratio, self.play_run_time = lag_ratio, play_run_time
        self.rate, self.play_rate, self.children = None, None, []

    def setCompositionRateFunction(self, rate):
        self.rate = rate

    def setPlayRateFunction(self, rate):
        self.play_rate = rate

    def appendComposition(self, child):
        self.children.append(child)

    def appendMatchingFamilyTransformTo(self, *args):
        self.children.append(('matching', args))

    def appendFamilyTransformTo(self, *args):
        self.children.append(('ordinary', args))


class MatchingTimingScopeTests(unittest.TestCase):
    def setUp(self):
        self.context = SimpleNamespace(
            beginOrdinaryCompositionBuilder=Builder,
            ordinaryCanPlayComposition=lambda candidate: True,
        )
        self.scene = SimpleNamespace(_next_object_id=0)
        self.source, self.target = Group(self.scene), Group()
        source = Path(__file__).with_name('_manim_scene.py').read_text()
        tree = ast.parse(source)
        names = {
            '_build_canonical_composition_candidate', '_canonical_play_options',
            '_canonical_composition_rate_id', '_canonical_transform_options',
            '_canonical_family_transform_animation',
        }
        nodes = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in names]
        namespace = {
            node.name: (lambda *args, **kwargs: None)
            for node in tree.body if isinstance(node, ast.FunctionDef) and node.name.startswith('_canonical_')
        }
        def resolve_transform(**kwargs):
            args = kwargs['builder_args']
            return SimpleNamespace(
                run_time=kwargs['play_run_time'] if kwargs['play_run_time'] is not None else args.get('run_time', 1.0),
                rate_func=kwargs['play_easing'] or rates.easing_from_rate_func(
                    kwargs['play_rate_func'] or args.get('rate_func', rates.smooth)),
                lag_ratio=kwargs['play_lag_ratio'] or args.get('lag_ratio', 0.0),
                path_arc=kwargs['play_path_arc'] if kwargs['play_path_arc'] is not None else args.get('path_arc', 0.0),
                reverse_rate_function=False,
            )
        namespace.update(
            _base=SimpleNamespace(Transform=animate.Transform),
            _animate=animate, _composition=composition, _rate_functions=rates,
            _compat=SimpleNamespace(Group=Group, _leaf_mobjects=lambda group: group.submobjects),
            _context=lambda scene: self.context,
            _options=SimpleNamespace(builder_args=lambda animation: animation.anim_args,
                                     resolve_transform=resolve_transform),
        )
        exec(compile(ast.Module(body=nodes, type_ignores=[]), '<production matching request>', 'exec',
                     flags=__future__.annotations.compiler_flag), namespace)
        self.build = namespace['_build_canonical_composition_candidate']

    def candidate(self, constructor=None, play=None, group=None, ordinary=False):
        constructor, play = constructor or {}, play or {}
        cls = animate.Transform if ordinary else animate.TransformMatchingShapes
        animation = cls(self.source, self.target, **constructor)
        candidate = self.build(self.scene, 'parallel', (animation,), group, play)
        self.assertEqual(animation.anim_args, constructor)
        self.assertEqual(candidate[-1], () if ordinary else (self.source, self.target))
        return candidate[0]

    def test_constructor_and_play_rates_are_separate_scopes(self):
        for constructor, play, inner, outer in [
            ({}, {}, 'smooth', 'linear'),
            ({'rate_func': rates.linear}, {}, 'linear', 'linear'),
            ({}, {'rate_func': rates.linear}, 'smooth', 'linear'),
            ({}, {'rate_func': rates.smooth}, 'smooth', 'smooth'),
            ({'rate_func': rates.linear}, {'rate_func': rates.smooth}, 'linear', 'smooth'),
        ]:
            with self.subTest(constructor=constructor, play=play):
                candidate = self.candidate(constructor, play)
                nested = candidate.children[0]
                self.assertIsInstance(nested, Builder)
                self.assertEqual(candidate.rate, 'linear')
                self.assertEqual(nested.rate, outer)
                self.assertEqual(nested.children[0][0], 'matching')
                self.assertEqual(nested.children[0][1][3], inner)

    def test_play_duration_rescales_group_not_constructor_child(self):
        candidate = self.candidate({'run_time': 3}, {'run_time': 2})
        nested = candidate.children[0]
        self.assertEqual(nested.run_time, 2)
        self.assertIsNone(nested.play_run_time)  # appendComposition consumes only group options.
        self.assertEqual(nested.children[0][1][2], 3)

    def test_explicit_parent_keeps_its_own_rate_scope(self):
        parent = composition.AnimationGroup(rate_func=rates.smooth)
        candidate = self.candidate(group=parent, play={'rate_func': rates.linear})
        self.assertEqual(candidate.rate, 'smooth')
        self.assertEqual(candidate.play_rate, 'linear')
        nested = candidate.children[0]
        self.assertEqual(nested.rate, 'linear')
        self.assertEqual(nested.children[0][1][3], 'smooth')

    def test_ordinary_family_transform_retains_flat_play_precedence(self):
        candidate = self.candidate({'rate_func': rates.smooth}, {'rate_func': rates.linear}, ordinary=True)
        kind, args = candidate.children[0]
        self.assertEqual(kind, 'ordinary')
        self.assertEqual(args[3], 'linear')

    def test_constructor_path_arc_stays_on_inner_transform(self):
        candidate = self.candidate({'path_arc': 0.5}, {'run_time': 2})
        self.assertEqual(candidate.children[0].children[0][1][5], 0.5)

    def test_play_level_path_arc_does_not_leak_into_inner_transform(self):
        with self.assertRaisesRegex(NotImplementedError, 'path_arc'):
            self.candidate(play={'path_arc': 0.5})


if __name__ == '__main__':
    unittest.main()
