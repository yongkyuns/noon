"""Real extension guards and canonical family authoring, with no Python state mirror."""
import asyncio
import json
import unittest

import _noon_native as native
from noon_native import close_scene, run_source


class NativeFamilyTests(unittest.TestCase):
    def test_zero_rgba_paint_queries_follow_shared_manim_policy_on_both_routes(self):
        source = '''from noon import *
class ZeroPaint(Scene):
 def construct(self):
  self.object=VMobject(color=BLACK)
  self.object.set_fill(BLACK,opacity=0).set_stroke(BLACK,width=1,opacity=0)
  self.add(self.object)
  assert self.object.get_fill_color() is None
  assert self.object.get_stroke_color() is None
  self.wait(.1)
  assert self.object.get_fill_color() is None
  self.object.set_fill(BLUE,opacity=0)
  self.object.set_stroke(BLACK,opacity=.25)
  assert self.object.get_fill_color() is not None
  assert self.object.get_stroke_color().red==0
  self.wait(.1)
'''
        async def check():
            for portable in (False,True):
                scene=await run_source(source,portable=portable,sample_hz=10)
                try:
                    self.assertIsNotNone(scene.object.get_fill_color())
                    self.assertEqual(scene.object.get_stroke_color().red,0)
                finally: close_scene(scene)
        asyncio.run(check())

    def test_tiny_positive_paint_survives_wait_and_animation_zero_transitions(self):
        source = '''from noon import *
class TinyPaint(Scene):
 def construct(self):
  value=Circle().set_fill(BLACK,opacity=1e-50).set_stroke(BLACK,opacity=1e-50)
  self.add(value)
  self.wait(.1)
  self.values=[[value.get_fill_color() is None,value.get_stroke_color() is None]]
  self.play(value.animate.set_fill(opacity=0),run_time=.1)
  value.set_stroke(BLACK,opacity=0)
  self.values.append([value.get_fill_color() is None,value.get_stroke_color() is None])
  self.play(value.animate.set_fill(opacity=1e-50),run_time=.1)
  value.set_stroke(BLACK,opacity=1e-50)
  self.values.append([value.get_fill_color() is None,value.get_stroke_color() is None])
'''
        async def check():
            for portable in (False, True):
                scene = await run_source(source, portable=portable, sample_hz=10)
                try:
                    self.assertEqual(scene.values, [[False, False], [True, True], [False, False]])
                finally:
                    close_scene(scene)
        asyncio.run(check())

    @staticmethod
    def family(store, *members):
        batch = native.MembershipBatch('add')
        for member in members:
            if isinstance(member, native.FamilyHandle):
                batch.appendFamily(member)
            else:
                batch.appendMobject('', member)
        return store.family(batch, 0)

    def test_atomic_style_updates_preserve_omissions_and_reject_invalid_inputs(self):
        store = native.Store()
        child = store.geometry(native.GeometryOptions.square(2))
        family = self.family(store, child, child)
        values = (True, 0, 1, 0, 1, .75, True, 0, 0, 1, 1, .125, .625)
        for target in (child, family):
            target.setStyle(*values)
            self.assertEqual((child.fillOpacity, child.strokeWidth, child.strokeOpacity),
                             (.75, .125, .625))
            target.setStyle(False, 0, 0, 0, 1, None, False, 0, 0, 0, 1, None, None)
            for index, bad in ((1, float('nan')), (5, 2), (11, -1), (12, -1)):
                invalid = list(values)
                invalid[index] = bad
                with self.assertRaises(RuntimeError): target.setStyle(*invalid)
                self.assertEqual((child.fillOpacity, child.strokeWidth, child.strokeOpacity),
                                 (.75, .125, .625))

    def test_style_context_checks_foreign_and_retired_handles_before_mutation(self):
        left, right = native.Store(), native.Store()
        local = left.geometry(native.GeometryOptions.square(2))
        foreign = right.geometry(native.GeometryOptions.square(2))
        context = left.context()
        values = (True, 0, 1, 0, 1, .75, False, 0, 0, 0, 1, None, None)
        for own, other, suffix in ((local, foreign, ''),
                                  (self.family(left, local), self.family(right, foreign), 'Family')):
            with self.assertRaisesRegex(RuntimeError, 'foreign|different.*store|another.*store'):
                getattr(context, 'liveSet'+suffix+'Style')(other, *values)
            for pair in ((own, other), (other, own)):
                with self.assertRaisesRegex(RuntimeError, 'foreign|different.*store|another.*store'):
                    getattr(context, 'liveMatch'+suffix+'Style')(*pair)
        context.retire()
        with self.assertRaisesRegex(RuntimeError, 'retired'): context.liveSetStyle(local, *values)
        with self.assertRaisesRegex(RuntimeError, 'retired'): context.liveMatchStyle(local, local)

    def test_atomic_style_and_matching_across_barriers_on_both_native_routes(self):
        source = '''from noon import *
class Style(Scene):
 def construct(self):
  self.a=Circle().set_style(fill_color=GREEN,fill_opacity=.75,stroke_color=BLUE,stroke_width=7,stroke_opacity=.625)
  self.b=Square().match_style(self.a)
  self.family=VGroup(self.a,self.b)
  self.add(self.family)
  assert abs(self.b.get_stroke_width()-7)<1e-6
  self.wait(.1)
  self.a.set_stroke(RED,width=3,opacity=.5)
  self.b.match_style(self.a)
  assert abs(self.b.get_stroke_width()-3)<1e-6
  self.family.set_style(fill_color=YELLOW,fill_opacity=.25,stroke_width=9)
  assert abs(self.a.get_fill_opacity()-.25)<1e-6
  assert abs(self.b.get_stroke_width()-9)<1e-6
  self.other=VGroup(Circle(),Square()).match_style(self.family)
  self.add(self.other)
  assert abs(self.other[0].get_stroke_width()-9)<1e-6
  self.wait(.1)
'''
        async def check():
            for portable in (False, True):
                scene = await run_source(source, portable=portable, sample_hz=10)
                try:
                    self.assertAlmostEqual(scene.b.get_stroke_width(),9,places=5)
                    self.assertAlmostEqual(scene.a.get_fill_opacity(),.25,places=6)
                finally: close_scene(scene)
        asyncio.run(check())

    def test_family_fill_and_stroke_preserve_omitted_fields_and_reject_invalid_paint(self):
        store = native.Store()
        child = store.geometry(native.GeometryOptions.square(2))
        family = self.family(store, child, child)
        family.setFill(True, 0, 1, 0, 1, .75)
        family.setStroke(True, 1, 1, 0, 1, .25, .625)
        self.assertEqual((child.fillOpacity, child.strokeOpacity, child.strokeWidth),
                         (.75, .625, .25))
        family.setFill(False, 0, 0, 0, 1, None)
        family.setStroke(False, 0, 0, 0, 1, None, None)
        self.assertEqual((child.fillOpacity, child.strokeOpacity, child.strokeWidth),
                         (.75, .625, .25))
        for operation, arguments in (
            (family.setFill, (True, float('nan'), 0, 0, 1, .5)),
            (family.setFill, (True, 1.1, 0, 0, 1, .5)),
            (family.setFill, (True, 1, 0, 0, 1, 2)),
            (family.setStroke, (True, 1, 0, 0, 1, -1, .5)),
        ):
            with self.assertRaises(RuntimeError):
                operation(*arguments)
            self.assertEqual((child.fillOpacity, child.strokeOpacity, child.strokeWidth),
                             (.75, .625, .25))
            self.assertEqual(child.manimColor().green, 1)

    def test_authored_stroke_width_projects_the_existing_semantic_handle(self):
        child = native.Store().geometry(native.GeometryOptions.square(2))
        child.setStrokeWidth(.125)
        self.assertEqual(child.strokeWidth, .125)
        child.setStrokeWidth(.25)
        self.assertEqual(child.strokeWidth, .25)

    def test_live_family_fill_and_stroke_reject_foreign_and_retired_contexts(self):
        left, right = native.Store(), native.Store()
        local, foreign = self.family(left), self.family(right)
        context = left.context()
        for name, arguments in (
            ('liveSetFamilyFill', (True, 1, 0, 0, 1, .5)),
            ('liveSetFamilyStroke', (True, 1, 0, 0, 1, .25, .5)),
        ):
            with self.assertRaisesRegex(RuntimeError, 'foreign|different.*store|another.*store'):
                getattr(context, name)(foreign, *arguments)
        context.retire()
        for name, arguments in (
            ('liveSetFamilyFill', (True, 1, 0, 0, 1, .5)),
            ('liveSetFamilyStroke', (True, 1, 0, 0, 1, .25, .5)),
        ):
            with self.assertRaisesRegex(RuntimeError, 'retired'):
                getattr(context, name)(local, *arguments)

    def test_empty_family_shift_and_scale_do_not_move_a_late_child(self):
        store = native.Store()
        family = self.family(store)
        family.layout().shiftBy(3, -2)
        family.scale(2, 2)
        empty = family.layout()
        self.assertEqual((empty.centerX, empty.centerY, empty.width, empty.height),
                         (0, 0, 0, 0))
        child = store.geometry(native.GeometryOptions.square(2))
        batch = native.MembershipBatch('add')
        batch.appendMobject('', child)
        family.editMembership(batch)
        layout = family.layout()
        self.assertEqual((layout.centerX, layout.centerY, layout.width, layout.height),
                         (0, 0, 2, 2))

    def test_family_opacity_edits_paint_and_rejects_invalid_values_atomically(self):
        store = native.Store()
        child = store.geometry(native.GeometryOptions.square(2))
        child.setFillOpacity(.8)
        child.setStrokeOpacity(.6)
        child.setObjectOpacity(.4)
        family = self.family(store, child, child)
        family.setOpacity(.25)
        self.assertEqual((child.fillOpacity, child.strokeOpacity), (.25, .25))
        for opacity in (float('nan'), float('inf'), -1, 2):
            with self.subTest(opacity=opacity):
                with self.assertRaises(RuntimeError):
                    family.setOpacity(opacity)
                self.assertEqual((child.fillOpacity, child.strokeOpacity), (.25, .25))

    def test_live_family_opacity_rejects_foreign_and_retired_contexts(self):
        left, right = native.Store(), native.Store()
        local, foreign = self.family(left), self.family(right)
        context = left.context()
        with self.assertRaisesRegex(RuntimeError, 'foreign|different.*store|another.*store'):
            context.liveSetFamilyOpacity(foreign, .5)
        context.retire()
        with self.assertRaisesRegex(RuntimeError, 'retired'):
            context.liveSetFamilyOpacity(local, .5)

    def test_family_opacity_before_and_after_barriers_uses_effective_paint(self):
        # Generic Group stores metadata; only VGroup recursively edits paint.
        # Binary-exact initial alpha isolates dispatch from render-wire rounding.
        source = '''from noon import *
class Paint(Scene):
    def construct(self):
        self.observations=[]
        for cls in (Group,VGroup):
            leaf=Square(fill_opacity=.75,stroke_opacity=.625)
            group=cls(leaf,leaf)
            group.set_opacity(.25)
            self.observations.append([leaf.get_fill_opacity(),leaf.get_stroke_opacity()])
            self.add(group)
            self.wait(.125)
            group.set_opacity(.5)
            self.observations.append([leaf.get_fill_opacity(),leaf.get_stroke_opacity()])
            self.wait(.125)
            self.remove(group)
'''
        for portable in (False, True):
            with self.subTest(portable=portable):
                scene = asyncio.run(run_source(source, portable=portable, sample_hz=8))
                try:
                    self.assertEqual(scene.observations,
                                     [[.75,.625],[.75,.625],[.25,.25],[.5,.5]])
                    self.assertEqual(scene.time, .5)
                finally:
                    close_scene(scene)

    def test_family_scale_and_shift_visit_shared_leaves_once_and_reject_invalid_values(self):
        store = native.Store()
        child = store.geometry(native.GeometryOptions.square(2))
        family = self.family(store, child, child)
        original = family.layout()
        family.scale(2, 3)
        family.layout().shiftBy(1, 2)
        layout = family.layout()
        self.assertEqual((layout.centerX, layout.centerY, layout.width, layout.height),
                         (1, 2, 4, 6))
        self.assertEqual((original.centerX, original.centerY, original.width, original.height),
                         (0, 0, 2, 2))
        with self.assertRaises(RuntimeError):
            family.scale(float('nan'), 1)
        with self.assertRaises(RuntimeError):
            family.layout().shiftBy(0, float('nan'))
        unchanged = family.layout()
        self.assertEqual((unchanged.centerX, unchanged.centerY, unchanged.width, unchanged.height),
                         (1, 2, 4, 6))

    def test_live_family_shift_scale_reject_foreign_and_retired_contexts(self):
        left, right = native.Store(), native.Store()
        local, foreign = self.family(left), self.family(right)
        context = left.context()
        for operation in ('liveShiftFamily', 'liveScaleFamily'):
            with self.subTest(operation=operation):
                with self.assertRaisesRegex(RuntimeError, 'foreign|different.*store|another.*store'):
                    getattr(context, operation)(foreign, 1, 1)
        context.retire()
        for operation in ('liveShiftFamily', 'liveScaleFamily'):
            with self.subTest(operation=operation):
                with self.assertRaisesRegex(RuntimeError, 'retired'):
                    getattr(context, operation)(local, 1, 1)
        self.assertEqual(local.memberKeys(), [])

    def test_group_shift_and_scale_after_a_barrier_publish_effective_layout(self):
        source = '''from noon import *
class LiveFamilyLayout(Scene):
    def construct(self):
        self.observations=[]
        for cls in (Group,VGroup):
            group=cls(Square(),Square().shift(2*RIGHT))
            self.add(group)
            self.wait(.125)
            for stage in ('initial','shift','scale'):
                if stage=='shift':group.shift(RIGHT)
                elif stage=='scale':group.scale(2)
                self.observations.append([cls.__name__,stage,list(group.get_center()),group.width,group.height])
            self.wait(.125)
            self.remove(group)
'''
        # Values below come from the complete pinned Group/VGroup probe.
        expected = [[name, stage, center, width, height]
                    for name in ('Group', 'VGroup')
                    for stage, center, width, height in (
                        ('initial', [1, 0], 4, 2),
                        ('shift', [2, 0], 4, 2),
                        ('scale', [2, 0], 8, 4))]
        for portable in (False, True):
            with self.subTest(portable=portable):
                scene = asyncio.run(run_source(source, portable=portable, sample_hz=8))
                try:
                    self.assertEqual(scene.observations, expected)
                    self.assertEqual(scene.time, .5)
                finally:
                    close_scene(scene)

    def test_family_priority_anchors_preserve_root_only_and_recursive_edits(self):
        store = native.Store()
        child = store.geometry(native.GeometryOptions.square(2))
        family = self.family(store, child, child)
        root, leaf = family.layoutAnchor(None), family.layoutAnchor(-1)
        root.setZIndex(3, False)
        self.assertEqual((root.zIndex(), leaf.zIndex()), (3, 0))
        root.setZIndex(7, True)
        self.assertEqual((root.zIndex(), leaf.zIndex()), (7, 7))
        with self.assertRaises(RuntimeError):
            root.setZIndex(float('nan'), True)
        self.assertEqual((root.zIndex(), leaf.zIndex()), (7, 7))

    def test_retired_context_rejects_live_priority_reads_and_writes(self):
        store = native.Store()
        child = store.geometry(native.GeometryOptions.square(2))
        anchor = child.layoutAnchor(None)
        context = store.context()
        context.retire()
        for operation, arguments in (('liveZIndex', (anchor,)),
                                     ('liveSetZIndex', (anchor, 3, True))):
            with self.subTest(operation=operation):
                with self.assertRaisesRegex(RuntimeError, 'retired'):
                    getattr(context, operation)(*arguments)
        self.assertEqual(anchor.zIndex(), 0)

    def test_live_priority_publishes_unique_family_members_and_rejects_foreign_anchors(self):
        source = """from noon import *
import _noon_native as native
class Priority(Scene):
    def construct(self):
        first,second=Square(),Square()
        group=Group(first,second,first)
        self.add(group)
        self.wait(.125)
        self.observations=[]
        self.observations.append([group.z_index,first.z_index,second.z_index])
        group.set_z_index(3,family=False)
        self.observations.append([group.z_index,first.z_index,second.z_index])
        first.set_z_index(5)
        self.observations.append([group.z_index,first.z_index,second.z_index])
        group.set_z_index(7)
        self.observations.append([group.z_index,first.z_index,second.z_index])
        try:group.set_z_index(float('nan'))
        except NoonError as error:assert 'non-finite z-index' in str(error)
        else:raise AssertionError('nonfinite priority admitted')
        foreign_store=native.Store()
        foreign=foreign_store.geometry(native.GeometryOptions.square(2)).layoutAnchor(None)
        context=self._canonical_authoring_context
        self.foreign_rejections=0
        for method,args in (('liveZIndex',(foreign,)),('liveSetZIndex',(foreign,9,True))):
            try:getattr(context,method)(*args)
            except RuntimeError as error:
                assert 'another scene store' in str(error).lower(),str(error)
                self.foreign_rejections+=1
            else:raise AssertionError('foreign priority anchor admitted')
        assert [group.z_index,first.z_index,second.z_index]==[7,7,7]
        self.wait(.125)
"""
        expected = [[0, 0, 0], [3, 0, 0], [3, 5, 0], [7, 7, 7]]
        for portable in (False, True):
            with self.subTest(portable=portable):
                scene = asyncio.run(run_source(source, portable=portable, sample_hz=8))
                try:
                    self.assertEqual(scene.observations, expected)
                    self.assertEqual(scene.foreign_rejections, 2)
                    self.assertEqual(scene.time, .25)
                finally:
                    close_scene(scene)

    def test_live_mobject_scaling_preserves_center_and_binding_guards(self):
        left, right = native.Store(), native.Store()
        local = left.geometry(native.GeometryOptions.square(2))
        foreign = right.geometry(native.GeometryOptions.square(2))
        context = left.context()
        with self.assertRaisesRegex(RuntimeError, 'another scene store'):
            context.liveScale(foreign, 2, 2)
        context.retire()
        with self.assertRaisesRegex(RuntimeError, 'retired'):
            context.liveScale(local, 2, 2)
        source = """from noon import *
class LiveScale(Scene):
    def construct(self):
        value=Square().shift(2*RIGHT)
        self.add(value)
        self.wait(.125)
        self.observations=[[list(value.get_center()),value.width,value.height]]
        value.scale(2)
        self.observations.append([list(value.get_center()),value.width,value.height])
        self.wait(.125)
"""
        for portable in (False, True):
            with self.subTest(portable=portable):
                scene = asyncio.run(run_source(source, portable=portable, sample_hz=8))
                try:
                    self.assertEqual(scene.observations, [[[2, 0], 2, 2], [[2, 0], 4, 4]])
                    self.assertEqual(scene.time, .25)
                finally:
                    close_scene(scene)

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
        with self.assertRaises(RuntimeError):
            context.queryFamilyLayout(foreign)
        context.retire()
        with self.assertRaises(RuntimeError):
            context.liveSetFamilyOwnColor(root, 1, 0, 0, 1)
        with self.assertRaises(RuntimeError):
            context.queryFamilyLayout(root)
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


class NativeLayoutTests(unittest.TestCase):
    def test_common_layout_projection_handles_both_signed_axes_and_center(self):
        source = '''from noon import *
class NativeCriticalPoints(Scene):
    def construct(self):
        square = Square().shift(RIGHT + 2 * UP)
        self.add(square)
        group = Group(square)
        self.add(group)
        self.observations = []
        for item in (square, group):
            self.observations.append([
                item.get_x(LEFT), item.get_x(ORIGIN), item.get_x(RIGHT),
                item.get_y(DOWN), item.get_y(ORIGIN), item.get_y(UP),
                list(item.get_corner(LEFT + DOWN)),
                list(item.get_corner(RIGHT + UP)),
            ])
        self.play(square.animate.shift(RIGHT), run_time=1, rate_func=linear)
        self.effective_edges = (group.get_x(LEFT), group.get_x(RIGHT))
'''
        expected = [0.0, 1.0, 2.0, 1.0, 2.0, 3.0, [0.0, 1.0], [2.0, 3.0]]
        for portable in (False, True):
            with self.subTest(portable=portable):
                scene = asyncio.run(run_source(source, portable=portable, sample_hz=4))
                try:
                    self.assertEqual(scene.observations, [expected, expected])
                    self.assertEqual(scene.effective_edges, (1.0, 3.0))
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
        self.assertEqual(rejected.exception.code, 'animation.unsupported_reverse')

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
