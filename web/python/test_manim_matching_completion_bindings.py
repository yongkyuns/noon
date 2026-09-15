"""Execute production completion bookkeeping with a mocked Rust boundary.

The native completed_binding_tests exercise real Rust matching, membership,
getters and continuation. These tests isolate Python identity/atomicity rules.
"""
import ast
import __future__
from dataclasses import dataclass
from pathlib import Path
from types import SimpleNamespace
import unittest


@dataclass
class ExportObject:
    id: int
    owner: object


class Mobject:
    def __init__(self, slot):
        self._semantic_handle = SimpleNamespace(semanticSlot=slot, semanticGeneration=0)
        self._scene = None
        self._object = None

    def _bind(self, scene, obj):
        self._scene, self._object = scene, obj


class Group:
    def __init__(self, slot, *members):
        self._semantic_family_handle = SimpleNamespace(semanticSlot=slot, semanticGeneration=0)
        self.submobjects = members


def leaves(value):
    if isinstance(value, Group):
        return [leaf for child in value.submobjects for leaf in leaves(child)]
    return [value]


class Indicate:
    def __init__(self, target):
        self.mobject = target


class Batch:
    def __init__(self):
        self.bindings = []

    def reserveMobjectBinding(self, wrapper_id, handle):
        self.bindings.append((int(wrapper_id), handle.semanticSlot))


class Context:
    def __init__(self, source, target):
        self.source, self.target = source, target
        self.bindings = {0: source.submobjects[0]._semantic_handle.semanticSlot}
        self.present = set(self.bindings.values())
        self.roots = ['100:0']
        self.pending = False
        self.reject_start = False
        self.reject_association = False
        self.keep_source = False

    def beginMembershipBatch(self, kind):
        assert kind == 'add'
        return Batch()

    def containsMobject(self, handle):
        return handle.semanticSlot in self.present

    def rootMembershipKeys(self):
        return self.roots

    def beginOrdinaryComposition(self, candidate):
        if self.reject_start:
            raise ValueError('rejected activation')
        self.pending = True

    def finish(self):
        self.pending = False
        self.present = {leaf._semantic_handle.semanticSlot for leaf in leaves(self.target)}
        if self.keep_source:
            self.present.update(self.bindings.values())
        self.roots = ['101:0']

    def ordinaryPlayComposition(self, candidate):
        self.beginOrdinaryComposition(candidate)
        self.finish()

    def associatePublishedMobjects(self, batch):
        if self.pending or self.reject_association:
            raise ValueError('rejected association')
        staged = dict(self.bindings)
        for wrapper_id, slot in batch.bindings:
            if slot not in self.present:
                raise ValueError('unpublished membership')
            if wrapper_id in staged and staged[wrapper_id] != slot:
                raise ValueError('conflicting wrapper identity')
            if slot in staged.values() and staged.get(wrapper_id) != slot:
                raise ValueError('conflicting semantic identity')
            staged[wrapper_id] = slot
        self.bindings = staged

    def editMembership(self, *args):
        raise AssertionError('completion must not replay membership')


class Pending:
    def __init__(self, scene, completed):
        self.scene, self.completed = scene, completed

    def finish(self):
        self.scene._canonical_authoring_context.finish()
        self.completed()


class MatchingCompletionBindingTests(unittest.TestCase):
    def setUp(self):
        names = {
            '_context', '_TypedBindingReservation', '_reserve_typed_binding',
            '_commit_typed_binding', '_record_mobject_binding', '_semantic_wrapper_key',
            '_membership_registry', '_register_membership_wrappers',
            '_membership_wrapper_leaves', '_membership_leaf_bindings',
            '_reconcile_completed_family_bindings', '_canonical_scene_mobjects',
            '_play_canonical_composition', '_canonical_indicate_animation',
        }
        tree = ast.parse(Path(__file__).with_name('_manim_scene.py').read_text())
        selected = [node for node in tree.body if getattr(node, 'name', None) in names]
        self.assertEqual({node.name for node in selected}, names)
        self.ns = {
            '__name__': __name__, 'dataclass': dataclass,
            '_base': SimpleNamespace(Scene=object, Mobject=Mobject),
            '_compat': SimpleNamespace(Group=Group, _leaf_mobjects=leaves),
            '_animate': SimpleNamespace(Indicate=Indicate),
            '_ir': SimpleNamespace(Object=ExportObject, _authoring_key=lambda label, key, default: default if key is None else key),
            'engine_call': lambda method, *args, operation=None: method(*args),
            '_start_default_synchronous_continuation': lambda scene: None,
            '_semantic_continuation_active': lambda scene: scene.mode != 'endpoint',
            '_require_semantic_continuation_active': lambda scene: None,
            '_prepare_semantic_continuation_callbacks': lambda scene, context: None,
            '_async_continuation_active': lambda scene: scene.mode == 'async',
            '_synchronous_continuation_active': lambda scene: scene.mode == 'sync',
            '_synchronous_continuation_wait': lambda scene: scene._canonical_authoring_context.finish(),
            '_continuation_awaitable': Pending,
        }
        exec(compile(ast.Module(body=selected, type_ignores=[]), '_manim_scene.py', 'exec', flags=__future__.annotations.compiler_flag), self.ns)
        self.source_leaf = Mobject(10)
        self.target_leaves = (Mobject(20), Mobject(21))
        self.source = Group(100, self.source_leaf)
        self.target = Group(101, Group(102, *self.target_leaves))
        self.context = Context(self.source, self.target)
        self.scene = SimpleNamespace(
            _canonical_authoring_context=self.context, _owner=object(),
            _next_object_id=0, _object_keys={}, _object_key_ids={}, _binding_handles={},
            mode='endpoint',
        )
        reservation = self.ns['_reserve_typed_binding'](self.source_leaf, self.scene, self.source_leaf._semantic_handle, None)
        self.ns['_commit_typed_binding'](self.source_leaf, self.scene, reservation, self.source_leaf._semantic_handle)
        self.ns['_register_membership_wrappers'](self.scene, self.source)

    def play(self):
        return self.ns['_play_canonical_composition'](
            self.scene, object(), [], [], [], [], (self.source, self.target)
        )

    def snapshot(self):
        return (
            self.scene._next_object_id, dict(self.scene._object_keys),
            dict(self.scene._object_key_ids), dict(self.scene._binding_handles),
            dict(self.scene._canonical_membership_wrappers),
            tuple((leaf._scene, leaf._object) for leaf in (self.source_leaf, *self.target_leaves)),
        )

    def assert_completed(self):
        self.assertIsNone(self.source_leaf._scene)
        self.assertIs(self.source_leaf._canonical_live_target_context, self.context)
        for leaf in self.target_leaves:
            self.assertIs(leaf._scene, self.scene)
            self.assertEqual(self.context.bindings[leaf._object.id], leaf._semantic_handle.semanticSlot)
        self.assertEqual(self.ns['_canonical_scene_mobjects'](self.scene), [self.target])
        self.assertEqual(self.ns['_canonical_indicate_animation'](self.scene, Indicate(self.target)), (self.target, True))
        self.assertEqual(self.context.roots, ['101:0'])

    def test_endpoint_and_synchronous_completion_rebind_original_target(self):
        for mode in ('endpoint', 'sync'):
            with self.subTest(mode=mode):
                self.setUp()
                self.scene.mode = mode
                with self.assertRaises(ValueError):
                    self.ns['_canonical_indicate_animation'](self.scene, Indicate(self.target))
                self.assertIs(self.play(), self.scene)
                self.assert_completed()

    def test_async_pending_completion_does_not_prebind_target(self):
        self.scene.mode = 'async'
        before = self.snapshot()
        pending = self.play()
        self.assertTrue(self.context.pending)
        self.assertEqual(self.snapshot(), before)
        pending.finish()
        self.assert_completed()

    def test_rejected_activation_keeps_wrapper_bookkeeping_unchanged(self):
        self.context.reject_start = True
        before = self.snapshot()
        with self.assertRaises(ValueError):
            self.play()
        self.assertEqual(self.snapshot(), before)

    def test_rejected_binding_batch_does_not_commit_a_target_prefix(self):
        self.context.reject_association = True
        before = self.snapshot()
        with self.assertRaises(ValueError):
            self.play()
        self.assertEqual(self.snapshot(), before)
        self.assertEqual(self.context.bindings, {0: 10})

    def test_early_reconciliation_is_rejected_without_binding_changes(self):
        self.scene.mode = 'async'
        pending = self.play()
        before = self.snapshot()
        with self.assertRaises(ValueError):
            pending.completed()
        self.assertEqual(self.snapshot(), before)
        pending.finish()
        self.assert_completed()

    def test_source_readd_reuses_its_original_derived_identity(self):
        original = self.source_leaf._object
        self.play()
        reservation = self.ns['_reserve_typed_binding'](self.source_leaf, self.scene, self.source_leaf._semantic_handle, None)
        self.assertTrue(reservation.reuse_existing_identity)
        self.assertIs(reservation.object, original)

    def test_still_present_source_alias_remains_bound(self):
        self.context.keep_source = True
        self.play()
        self.assertIs(self.source_leaf._scene, self.scene)
        self.assertEqual(self.ns['_canonical_indicate_animation'](self.scene, Indicate(self.target)), (self.target, True))

    def test_absent_intermediate_target_does_not_gain_a_binding(self):
        intermediate = Group(103, Mobject(30))
        self.context.finish()
        self.ns['_reconcile_completed_family_bindings'](self.scene, (self.source, intermediate, self.target, self.target))
        self.assertIsNone(intermediate.submobjects[0]._scene)
        self.assertIsNone(intermediate.submobjects[0]._object)
        self.assert_completed()
        self.assertEqual(self.scene._next_object_id, 3)


if __name__ == '__main__':
    unittest.main()
