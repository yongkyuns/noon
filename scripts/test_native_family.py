"""Real extension guards and canonical family authoring, with no Python state mirror."""
import asyncio
import json
import unittest

import _noon_native as native
from noon_native import close_scene, run_source


class NativeFamilyTests(unittest.TestCase):
    @staticmethod
    def family(store, *members):
        batch = native.MembershipBatch('add')
        for member in members:
            if isinstance(member, native.FamilyHandle):
                batch.appendFamily(member)
            else:
                batch.appendMobject('', member)
        return store.family(batch, 0)

    def test_root_metadata_and_copy_independence_use_real_shared_handles(self):
        store = native.Store()
        root = self.family(store)
        root.setOwnColor(1, 0, 0, 1)
        self.assertEqual((root.manimColor().red, root.manimColor().green), (1, 0))
        refs = native.MembershipBatch('add')
        refs.appendFamily(root)
        copied = root.copyFamily(refs).familyFor(root)
        copied.setOwnColor(0, 0, 1, 1)
        self.assertEqual(root.manimColor().red, 1)
        self.assertEqual(copied.manimColor().blue, 1)
        with self.assertRaises(RuntimeError):
            root.setOwnColor(float('nan'), 0, 0, 1)
        self.assertEqual(root.manimColor().red, 1)

    def test_foreign_family_creation_and_copy_reject_before_publication(self):
        left, right = native.Store(), native.Store()
        root = self.family(left)
        foreign = self.family(right)
        batch = native.MembershipBatch('add')
        batch.appendFamily(foreign)
        with self.assertRaises(RuntimeError):
            left.family(batch, 0)
        with self.assertRaises(RuntimeError):
            root.copyFamily(batch)
        self.assertEqual(root.memberKeys(), [])
        context = left.context()
        with self.assertRaises(RuntimeError):
            context.liveSetFamilyOwnColor(foreign, 1, 0, 0, 1)
        context.retire()
        with self.assertRaises(RuntimeError):
            context.liveSetFamilyOwnColor(root, 1, 0, 0, 1)
        self.assertEqual(root.manimColor().red, 1)

    def test_family_transform_and_method_completion_match_original_in_both_source_modes(self):
        # Endpoint values come from the complete pinned Manim Scene probe.
        source = '''from noon import *
import json


def rgb_hex(color):
    if hasattr(color, 'to_hex'):
        return color.to_hex().upper()
    return '#' + ''.join(f'{round(component * 255):02X}'
                          for component in (color.red, color.green, color.blue))


class FamilyTransformOwnColors(Scene):
    def construct(self):
        observations = []
        for kind in (Group, VGroup):
            for method in (False, True):
                source = kind(Square(color=RED))
                target = kind(Square(color=BLUE), color=GREEN)
                self.add(source)
                before = [rgb_hex(source.get_color()), rgb_hex(source[0].get_color())]
                self.play(source.animate.set_color(GREEN) if method else Transform(source, target),
                          run_time=1, rate_func=linear)
                after = [rgb_hex(source.get_color()), rgb_hex(source[0].get_color())]
                observations.append({'kind': kind.__name__, 'method': method,
                                     'before': before, 'after': after})
                self.remove(source)
        self.observations = observations
'''
        expected = [
            {'kind': name, 'method': method,
             'before': ['#FFFFFF', '#FC6255'],
             'after': ['#83C167', '#83C167'] if method else ['#FFFFFF', '#58C4DD']}
            for name in ('Group', 'VGroup') for method in (False, True)
        ]
        for portable in (False, True):
            with self.subTest(portable=portable):
                scene = asyncio.run(run_source(source, portable=portable, sample_hz=4))
                try:
                    self.assertEqual(scene.observations, expected)
                finally:
                    close_scene(scene)

    def test_family_transform_rejects_foreign_and_retired_context_before_writes(self):
        for method in (False, True):
            with self.subTest(method=method):
                left, right = native.Store(), native.Store()
                source, foreign = self.family(left), self.family(right)
                context = left.context()
                candidate = context.beginOrdinaryCompositionBuilder('parallel', None, 0, None)
                append = candidate.appendFamilyMethodTargetTo if method else candidate.appendFamilyTransformTo
                append(source, foreign, 1, 'linear', 0, 0)
                with self.assertRaisesRegex(RuntimeError, 'foreign|different.*store|another.*store'):
                    context.beginOrdinaryComposition(candidate)
                self.assertEqual(source.memberKeys(), [])
                self.assertEqual(source.manimColor().red, 1)
                context.retire()
                with self.assertRaisesRegex(RuntimeError, 'retired'):
                    context.beginOrdinaryComposition(candidate)

    def test_real_scene_family_admission_and_color_publication_in_both_source_modes(self):
        source = '''from noon import *
class FamilyColors(Scene):
    def construct(self):
        a = Square(color=RED)
        b = Square(color=BLUE)
        family = Group(a, b, color=GREEN)
        assert family.get_color().green > .7
        family.set_color(GREEN, family=False)
        assert a.get_color().red > .9 and b.get_color().blue > .8
        self.add(family)
        self.wait(.125)
        family.set_color(WHITE)
        assert family.get_color().red == 1 and a.get_color().red == 1
        self.colors = [family.get_color().red, a.get_color().green, b.get_color().blue]
        late = Group(a, color=BLUE)
        assert a.get_color().red == 1
        copied = late.copy()
        copied.set_color(GREEN, family=False)
        assert late.get_color().blue > .8 and copied.get_color().green > .7
        copied.add(b)
        assert len(copied.submobjects) == 2
        copied.remove(b)
        assert len(copied.submobjects) == 1
'''
        for portable in (False, True):
            with self.subTest(portable=portable):
                scene = asyncio.run(run_source(source, portable=portable, sample_hz=8))
                try:
                    self.assertEqual(scene.colors, [1, 1, 1])
                finally:
                    close_scene(scene)


class NativeCreateOptionsTests(unittest.TestCase):
    def test_create_projection_admits_reversal_without_relaxing_generic_options(self):
        args = (0, 1, 'lingering', float('nan'), float('nan'), 1,
                float('nan'), '', float('nan'))
        resolved = native.resolve_create_options(*args)
        self.assertTrue(resolved.reverseRateFunction)
        self.assertEqual(resolved.rateFunc, 'lingering')
        self.assertEqual(resolved.runTime, 1)
        with self.assertRaises(RuntimeError) as rejected:
            native.resolve_animation_options(*args)
        self.assertEqual(rejected.exception.code, 'animation.unsupported_reverse_rate_function')

    def test_lingering_reverse_create_runs_in_both_native_source_modes(self):
        source = '''from noon import *
class NativeLingering(Scene):
    def construct(self):
        moving = Square().shift(2 * LEFT)
        reversing = Square().shift(2 * LEFT + UP)
        self.add(moving)
        self.play(AnimationGroup(
            moving.animate(run_time=1, rate_func=lingering).shift(4 * RIGHT),
            Create(reversing, run_time=1, rate_func=lingering, reverse_rate_function=True),
            rate_func=linear,
        ))
        self.observed = (moving.get_x(), reversing in self.mobjects)
'''
        for portable in (False, True):
            with self.subTest(portable=portable):
                scene = asyncio.run(run_source(source, portable=portable, sample_hz=4))
                try:
                    self.assertEqual(scene.observed, (2.0, True))
                finally:
                    close_scene(scene)


if __name__ == '__main__':
    unittest.main()
