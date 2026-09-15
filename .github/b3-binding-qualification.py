"""Temporary patch application for #1614; removed from the resulting product tree."""
from pathlib import Path
import subprocess


def replace_once(text, old, new):
    assert text.count(old) == 1, (old[:100], text.count(old))
    return text.replace(old, new, 1)


rust_path = Path('crates/noon-web/src/canonical_authoring_scene.rs')
python_path = Path('web/python/_manim_scene.py')
for path, expected in [(rust_path, '977bba95d1b78231a593ded9406ec49b38362ad0'), (python_path, '9ec6c243485f602afd002a3246ca993e7d20552c')]:
    actual = subprocess.check_output(['git', 'hash-object', str(path)], text=True).strip()
    assert actual == expected, (str(path), actual)

rust = rust_path.read_text()
signature = '    fn edit_membership(&mut self, batch: SceneMembershipBatch) -> Result<(), AuthoringFailure> {\n'
start = rust.index(signature)
end = rust.index('        let mut borrowed = Vec::with_capacity(batch.members.len());', start)
validation = rust[start + len(signature):end]
association = '''    /// Associate language identities with already-published membership, without
    /// replaying a semantic edit or changing the retained execution session.
    #[cfg(any(target_arch = "wasm32", test))]
    fn associate_published_mobjects(
        &mut self,
        batch: SceneMembershipBatch,
    ) -> Result<(), AuthoringFailure> {
        if batch.kind != SceneMembershipBatchKind::Add || !batch.members.is_empty() {
            return Err("published association accepts binding reservations only".into());
        }
        let player = self
            .player_ownership
            .local()
            .ok_or("published association requires the local completed execution player")?;
        if player.has_pending_live_segment() {
            return Err("published association cannot precede segment completion".into());
        }
        if player.scene_revision() != self.scene.revision() {
            return Err("published association requires a coherent execution revision".into());
        }
        let new_bindings = self.validate_membership_bindings(&batch)?;
        for (_, handle) in &batch.bindings {
            if !self.contains_mobject(handle)? {
                return Err("published association target is not in this Scene".into());
            }
        }
        // Every reservation and membership observation succeeded before either
        // direction of the derived identity registry is changed.
        for (id, node) in new_bindings {
            self.bindings.insert(id, node);
            self.identities.insert(node, id);
        }
        Ok(())
    }

'''
rust = rust[:start] + '''    fn validate_membership_bindings(
        &self,
        batch: &SceneMembershipBatch,
    ) -> Result<Vec<(ObjectId, noon_core::SemanticNodeId)>, AuthoringFailure> {
''' + validation + '''        Ok(new_bindings)
    }

''' + association + signature + '''        let new_bindings = self.validate_membership_bindings(&batch)?;
        let mut seen_nodes = batch
            .bindings
            .iter()
            .map(|(_, handle)| handle.node_id())
            .collect::<BTreeSet<_>>();
''' + rust[end:]
rust = replace_once(rust, 'mod wait_bootstrap_tests;\n', 'mod wait_bootstrap_tests;\n#[cfg(test)]\nmod completed_binding_tests;\n')
anchor = '''    /// Inert typed language-wrapper batch. Appending handles performs no semantic
'''
rust = replace_once(rust, anchor, '''    #[wasm_bindgen]
    impl CanonicalAuthoringSceneContext {
        /// Reconcile wrapper IDs only after Rust has published the completion.
        #[wasm_bindgen(js_name = associatePublishedMobjects)]
        pub fn associate_published_mobjects(
            &mut self,
            batch: WasmSceneMembershipBatch,
        ) -> Result<(), JsValue> {
            self.inner
                .associate_published_mobjects(batch.inner)
                .map_err(js_error)
        }
    }

''' + anchor)
rust_path.write_text(rust)

source = python_path.read_text()
source = replace_once(source, '''def _register_membership_wrappers(scene: _base.Scene, value: object) -> None:
    registry = _membership_registry(scene)
''', '''def _register_membership_wrappers(
    scene: _base.Scene, value: object, *, registry: dict[str, object] | None = None
) -> None:
    if registry is None:
        registry = _membership_registry(scene)
''')
source = replace_once(source, '_register_membership_wrappers(scene, member)\n', '_register_membership_wrappers(scene, member, registry=registry)\n')
helper = '''def _reconcile_completed_family_bindings(
    scene: _base.Scene, families: tuple[object, ...]
) -> None:
    """Refresh affected wrapper identities from completed Rust membership only."""
    if not families:
        return
    context = _context(scene)
    wrappers: dict[str, object] = {}
    for family in families:
        _register_membership_wrappers(scene, family, registry=wrappers)
    batch = engine_call(context.beginMembershipBatch, "add", operation="Scene.completion")
    next_object_id = scene._next_object_id
    reservations = []
    detached = []
    binding_keys: set[str] = set()
    for wrapper in wrappers.values():
        if isinstance(wrapper, _compat.Group):
            continue
        if wrapper._scene is not None and wrapper._scene is not scene:
            raise ValueError("completion wrapper belongs to another Scene")
        present = bool(engine_call(
            context.containsMobject, wrapper._semantic_handle, operation="Scene.completion"
        ))
        if present:
            next_object_id, appended = _membership_leaf_bindings(
                scene, batch, wrapper, next_object_id=next_object_id,
                key=None, binding_keys=binding_keys,
            )
            reservations.extend(appended)
        elif wrapper._scene is scene:
            detached.append(wrapper)
    # This validates the entire binding batch against published Rust membership.
    # It does not add/remove objects, relower, or repeat matching cleanup.
    engine_call(context.associatePublishedMobjects, batch, operation="Scene.completion")
    for wrapper, reservation, handle in reservations:
        _commit_typed_binding(wrapper, scene, reservation, handle)
    _membership_registry(scene).update(wrappers)
    for wrapper in detached:
        wrapper._canonical_live_target_context = context
        wrapper._scene = None


'''
source = replace_once(source, 'def _canonical_scene_mobjects(scene: _base.Scene) -> list[object]:\n', helper + 'def _canonical_scene_mobjects(scene: _base.Scene) -> list[object]:\n')
source = replace_once(source, '    tracker_associations: list[_reactive.ValueTracker] = []\n', '    tracker_associations: list[_reactive.ValueTracker] = []\n    completed_families: list[object] = []\n')
source = replace_once(source, '''            append_family_transform(
                source._semantic_family_handle,
                target._semantic_family_handle,
                float(child.run_time),
                str(child.rate_func),
                float(child.lag_ratio),
                float(child.path_arc),
            )
            return
''', '''            append_family_transform(
                source._semantic_family_handle,
                target._semantic_family_handle,
                float(child.run_time),
                str(child.rate_func),
                float(child.lag_ratio),
                float(child.path_arc),
            )
            if type(leaf) is _animate.TransformMatchingShapes:
                completed_families.extend((source, target))
            return
''')
source = replace_once(source, '''        tracker_associations,
    ) if supported else False
''', '''        tracker_associations,
        tuple(completed_families),
    ) if supported else False
''')
source = replace_once(source, '''    tracker_associations: list[_reactive.ValueTracker],
) -> _base.Scene | _SemanticContinuationAwaitable:
''', '''    tracker_associations: list[_reactive.ValueTracker],
    completed_families: tuple[object, ...] = (),
) -> _base.Scene | _SemanticContinuationAwaitable:
''')
source = replace_once(source, '''    def completed() -> None:
        for target in removals:
''', '''    def completed() -> None:
        _reconcile_completed_family_bindings(self, completed_families)
        for target in removals:
''')
compile(source, str(python_path), 'exec')
python_path.write_text(source)

Path('crates/noon-web/src/canonical_authoring_scene/completed_binding_tests.rs').write_text('''use super::*;

fn shape(context: &CanonicalAuthoringScene, x: f64) -> noon::Mobject {
    let path = noon::VectorPath::new()
        .move_to(noon::Vec2::new(-1.0, -1.0))
        .line_to(noon::Vec2::new(1.0, -0.5))
        .line_to(noon::Vec2::new(-0.25, 1.0))
        .close();
    let mut object = noon::Mobject::from_manim_geometry(
        std::rc::Rc::clone(context.scene.integration_store()),
        noon::ManimGeometryOptions::path(path).unwrap(),
    )
    .unwrap();
    object.set_translation(x, 0.0).unwrap();
    object
}

fn fixture() -> (
    CanonicalAuthoringScene,
    noon::Mobject,
    noon::MobjectFamily,
    noon::Mobject,
    noon::MobjectFamily,
) {
    let mut context = CanonicalAuthoringScene::default();
    let source_leaf = shape(&context, -2.0);
    let target_leaf = shape(&context, 4.0);
    let source = context.scene.family(&[(&source_leaf).into()]).unwrap();
    let target = context.scene.family(&[(&target_leaf).into()]).unwrap();
    context
        .edit_membership(SceneMembershipBatch {
            kind: SceneMembershipBatchKind::Add,
            members: vec![OwnedSceneMembershipMember::Family(source.clone())],
            bindings: vec![(ObjectId::new(0), source_leaf.clone())],
        })
        .unwrap();
    (context, source_leaf, source, target_leaf, target)
}

fn binding_batch(bindings: Vec<(ObjectId, noon::Mobject)>) -> SceneMembershipBatch {
    SceneMembershipBatch {
        kind: SceneMembershipBatchKind::Add,
        members: vec![],
        bindings,
    }
}

fn begin_matching(
    context: &mut CanonicalAuthoringScene,
    source: &noon::MobjectFamily,
    target: &noon::MobjectFamily,
) -> f64 {
    context
        .begin_ordinary_mixed_composition(
            noon_core::SemanticAnimationCompositionKind::Parallel,
            &[OrdinaryCompositionChild::MatchingFamilyTransformTo {
                source: source.clone(),
                target_state: target.clone(),
                options: noon_core::AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(noon_core::RateFunction::Linear),
            }],
            noon_core::AnimationOptions::new(),
            noon_core::AnimationOptions::new(),
        )
        .unwrap()
}

fn complete(context: &mut CanonicalAuthoringScene, end: f64) {
    let player = context.active_live_player().unwrap();
    player.live_advance_segment_to(end).unwrap();
    player.live_complete_segment().unwrap();
}

#[test]
fn completed_binding_preserves_matching_identity_getters_and_second_animation() {
    let (mut context, source_leaf, source, target_leaf, target) = fixture();
    let end = begin_matching(&mut context, &source, &target);
    assert!(context.mobject_layout(&target_leaf).is_err());
    complete(&mut context, end);
    assert!(!context.contains_mobject(&source_leaf).unwrap());
    assert!(context.contains_mobject(&target_leaf).unwrap());
    let revision = context.scene.revision();
    let roots = context.root_membership_keys().unwrap();
    let player_id = context.active_live_player().unwrap().ownership_identity();
    context
        .associate_published_mobjects(binding_batch(vec![(ObjectId::new(1), target_leaf.clone())]))
        .unwrap();
    assert_eq!(context.scene.revision(), revision);
    assert_eq!(context.root_membership_keys().unwrap(), roots);
    assert_eq!(context.active_live_player().unwrap().ownership_identity(), player_id);
    assert_eq!(context.bindings[&ObjectId::new(1)], target_leaf.node_id());
    assert_eq!(context.identities[&target_leaf.node_id()], ObjectId::new(1));
    assert!((context.mobject_layout(&target_leaf).unwrap().0 - 4.0).abs() < 1e-6);
    let end = context
        .begin_ordinary_mixed_composition(
            noon_core::SemanticAnimationCompositionKind::Parallel,
            &[OrdinaryCompositionChild::FamilyIndicate {
                target,
                indication: noon::IndicateOptions::default(),
                options: noon_core::AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(noon_core::RateFunction::ThereAndBack),
            }],
            noon_core::AnimationOptions::new(),
            noon_core::AnimationOptions::new(),
        )
        .unwrap();
    complete(&mut context, end);
    assert!((context.mobject_layout(&target_leaf).unwrap().0 - 4.0).abs() < 1e-6);
    context.bind_mobject(ObjectId::new(0), &source_leaf).unwrap();
    assert!(context.contains_mobject(&source_leaf).unwrap());
    assert_eq!(context.identities[&source_leaf.node_id()], ObjectId::new(0));
    assert!((context.mobject_layout(&source_leaf).unwrap().0 + 2.0).abs() < 1e-6);
}

#[test]
fn completed_binding_rejects_pending_or_transferred_player_without_bookkeeping() {
    let (mut context, _, source, target_leaf, target) = fixture();
    let end = begin_matching(&mut context, &source, &target);
    let before = context.bindings.clone();
    assert!(context
        .associate_published_mobjects(binding_batch(vec![(ObjectId::new(1), target_leaf.clone())]))
        .is_err());
    assert_eq!(context.bindings, before);
    assert!(!context.identities.contains_key(&target_leaf.node_id()));
    complete(&mut context, end);
    let player = context.take_execution_player(end, 83).unwrap();
    assert!(context
        .associate_published_mobjects(binding_batch(vec![(ObjectId::new(1), target_leaf.clone())]))
        .is_err());
    assert_eq!(context.bindings, before);
    assert!(context.player_ownership.is_transferred());
    context.return_execution_player(player).unwrap();
    context
        .associate_published_mobjects(binding_batch(vec![(ObjectId::new(1), target_leaf)]))
        .unwrap();
    assert!(context.player_ownership.is_returned());
}

#[test]
fn completed_binding_validates_entire_batch_before_committing_either_registry() {
    let (mut context, source_leaf, source, target_leaf, target) = fixture();
    let unpublished = shape(&context, 9.0);
    let foreign_context = CanonicalAuthoringScene::default();
    let foreign = shape(&foreign_context, 9.0);
    let end = begin_matching(&mut context, &source, &target);
    complete(&mut context, end);
    let bindings = context.bindings.clone();
    let identities = context.identities.clone();
    let revision = context.scene.revision();
    let roots = context.root_membership_keys().unwrap();
    for invalid in [unpublished, foreign, source_leaf, target_leaf.clone()] {
        assert!(context
            .associate_published_mobjects(binding_batch(vec![
                (ObjectId::new(1), target_leaf.clone()),
                (ObjectId::new(2), invalid),
            ]))
            .is_err());
        assert_eq!(context.bindings, bindings);
        assert_eq!(context.identities, identities);
        assert_eq!(context.scene.revision(), revision);
        assert_eq!(context.root_membership_keys().unwrap(), roots);
    }
    context
        .associate_published_mobjects(binding_batch(vec![(ObjectId::new(1), target_leaf.clone())]))
        .unwrap();
    // Repeating the exact association is harmless; assigning another wrapper ID is not.
    context
        .associate_published_mobjects(binding_batch(vec![(ObjectId::new(1), target_leaf.clone())]))
        .unwrap();
    assert!(context
        .associate_published_mobjects(binding_batch(vec![(ObjectId::new(7), target_leaf)]))
        .is_err());
    assert_eq!(context.bindings.len(), 2);
}
''')

Path('web/python/test_manim_matching_completion_bindings.py').write_text('''"""Execute production completion bookkeeping with a mocked Rust boundary.

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
''')
print('Applied B3 completion-binding product changes and regression tests.')
