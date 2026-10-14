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


if __name__ == '__main__':
    unittest.main()
