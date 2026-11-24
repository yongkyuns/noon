//! Prepared semantic -> compiler -> runtime proof, not public Scene admission.
use noon_compile::{
    lower_semantic_execution_root, prepare_semantic_publication, validate_semantic_publication,
    ExecutionMutationTransaction, ExecutionPatch, SemanticExecutionIndex,
    SemanticExecutionReachability,
};
use noon_core::{
    Color, EffectUpdate, Glow, ObjectId, Pixels, Property, RateFunction,
    SemanticMutationTransaction, SemanticMutationTransactionResult, SemanticNodeCreation,
    SemanticNodeId, SemanticObjectProperty, SemanticObjectState, SemanticPaint, SemanticStore,
    SemanticVec3, StoredGeometry, TrackDefinition, TrackId, TrackTiming, TrackValues, Vec2,
};
use noon_runtime::{FrameChanges, ReplayLimits, SceneInstance};

fn circle() -> SemanticObjectState {
    SemanticObjectState::new(StoredGeometry::Circle { radius: 0.4 })
}
fn effect(intensity: f64) -> Glow {
    Glow::new(
        EffectUpdate::default()
            .radius(Pixels(3.25))
            .color(Color::RED)
            .intensity(intensity),
    )
    .unwrap()
}

// Tests coordinate the existing boundaries explicitly so no private/public Scene
// guard is bypassed by a product-side alternative session implementation.
struct Fixture {
    store: SemanticStore,
    root: SemanticNodeId,
    source: SemanticNodeId,
    index: SemanticExecutionIndex,
    reachability: SemanticExecutionReachability,
    runtime: SceneInstance,
}
impl Fixture {
    fn new(others: usize) -> Self {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let source = store.insert_semantic_object(circle());
        store.add_member(root, source).unwrap();
        for _ in 0..others {
            let object = store.insert_semantic_object(circle());
            store.add_member(root, object).unwrap();
        }
        let mut index = SemanticExecutionIndex::new();
        let lowered = lower_semantic_execution_root(&store, root, &mut index).unwrap();
        let reachability = SemanticExecutionReachability::from_root(&store, root).unwrap();
        Self {
            store,
            root,
            source,
            index,
            reachability,
            runtime: SceneInstance::from_semantic_execution(lowered),
        }
    }
    fn object(&self) -> ObjectId {
        self.index.execution_object_id(self.source).unwrap()
    }
    fn attach(&mut self, intensity: f64) -> SemanticNodeId {
        let mut tx = SemanticMutationTransaction::new();
        let pending = tx.create_effect(self.source, "glow", effect(intensity));
        self.publish(tx).resolve(pending).unwrap()
    }
    fn publish(&mut self, tx: SemanticMutationTransaction) -> SemanticMutationTransactionResult {
        let context = self.runtime.publication_context();
        let prepared = tx.prepare(&mut self.store).unwrap();
        let publication =
            prepare_semantic_publication(&prepared, &self.index, &self.reachability).unwrap();
        let mut patches = publication.value_transaction().mutations().to_vec();
        patches.extend(
            publication
                .possible_exits()
                .iter()
                .copied()
                .map(ExecutionPatch::RemoveObject),
        );
        patches.extend(publication.conservative_entry_patches(&prepared));
        self.runtime
            .preflight_authored_transaction_shape_with_resources(
                &ExecutionMutationTransaction::from_mutations(patches),
                publication.resource_additions(),
                context,
                prepared.proposed_scene_revision(),
                publication.possible_entry_count(),
                true,
            )
            .unwrap();
        let result = prepared.commit();
        let membership = self
            .reachability
            .apply_transaction_result(&self.store, &result)
            .unwrap();
        self.index.apply_impacts(&self.store, result.impacts());
        self.index.apply_reachability_update(&membership);
        let (patches, resources) = publication.bind(&result, &membership).into_parts();
        self.runtime
            .apply_authored_execution_transaction(
                &patches,
                resources,
                context,
                self.store.scene_revision(),
            )
            .unwrap();
        result
    }
}

#[test]
fn prepared_attachment_is_inert_until_commit_and_uses_the_reserved_semantic_identity() {
    let mut f = Fixture::new(0);
    let frame = f.runtime.frame().clone();
    let revision = f.store.scene_revision();
    let mut tx = SemanticMutationTransaction::new();
    let pending = tx.create_effect(f.source, "glow", effect(0.4));
    // Raw syntax admits the request; the held preparation below still owns
    // profile validation, identity reservation and atomic publication.
    validate_semantic_publication(&tx).unwrap();
    assert_eq!(f.store.scene_revision(), revision);
    assert_eq!(f.runtime.frame(), &frame);
    let prepared = tx.prepare(&mut f.store).unwrap();
    let id = prepared.planned_node_id(pending).unwrap();
    let publication = prepare_semantic_publication(&prepared, &f.index, &f.reachability).unwrap();
    let [ExecutionPatch::SetEffectStack {
        object,
        expected,
        effects: Some(value),
    }] = publication.value_transaction().mutations()
    else {
        panic!("one enrollment patch")
    };
    assert!(expected.is_empty());
    assert_eq!(*object, f.index.execution_object_id(f.source).unwrap());
    assert_eq!(value.attachments(), &[id]);
    assert_eq!(value.definitions(), &[effect(0.4).into()]);
    assert_eq!(publication.stats().object_states_lowered, 0);
    assert_eq!(prepared.store().scene_revision(), revision);
    assert_eq!(f.runtime.frame(), &frame);
    drop(prepared);
    assert_eq!(f.store.scene_revision(), revision);
    assert!(f.store.effect_by_name(f.source, "glow").unwrap().is_none());
    assert_eq!(
        f.attach(0.4),
        id,
        "aborted preparation must not consume a generation"
    );
    assert_eq!(
        f.runtime.frame().objects[0]
            .effects
            .as_ref()
            .unwrap()
            .attachments(),
        &[id]
    );
}

#[test]
fn composed_parameter_publication_reuses_its_plan_and_existing_exact_channel_scheduler() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let source = store.insert_family();
    let mut children = Vec::new();
    for _ in 0..512 {
        let child = store.insert_semantic_object(circle());
        store.add_member(source, child).unwrap();
        children.push(child);
    }
    store.add_member(root, source).unwrap();
    let mut tx = SemanticMutationTransaction::new();
    let token = tx.create_effect(source, "glow", effect(0.25));
    let attachment = tx.apply(&mut store).unwrap().resolve(token).unwrap();
    let mut index = SemanticExecutionIndex::new();
    let lowered = lower_semantic_execution_root(&store, root, &mut index).unwrap();
    let object = index.execution_object_id(source).unwrap();
    let row = lowered.compiled().object_index(object).unwrap() as usize;
    let reachability = SemanticExecutionReachability::from_root(&store, root).unwrap();
    let mut f = Fixture {
        store,
        root,
        source,
        index,
        reachability,
        runtime: SceneInstance::from_semantic_execution(lowered),
    };
    let source_plan = if let noon_core::ObjectContentRef::Composed(plan) =
        &f.runtime.frame().objects[row].content
    {
        plan.clone()
    } else {
        panic!("composed target")
    };
    {
        let publication = f.runtime.take_renderer_publication();
        assert_eq!(publication.object_index(object), Some(row));
        for &child in &children {
            let child = f.index.execution_object_id(child).unwrap();
            let index = publication.object_index(child).unwrap();
            assert_eq!(publication.frame().objects[index].id, child);
        }
        assert_eq!(publication.object_index(ObjectId::new(u64::MAX)), None);
    }
    f.runtime
        .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
            id: TrackId::new(91),
            object,
            property: Property::EffectIntensity,
            values: TrackValues::Effect {
                attachment,
                from: noon_core::EffectTrackValue::Intensity(0.25),
                to: noon_core::EffectTrackValue::Intensity(2.25),
                end_behavior: noon_core::EffectTrackEndBehavior::Sample,
            },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: Default::default(),
        }))
        .unwrap();
    f.runtime.seek(1.0).unwrap();
    f.runtime.take_frame_changes();
    f.runtime.take_spatial_changes();
    let child_before = f.runtime.frame().objects[0].clone();
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(attachment, EffectUpdate::default().radius(Pixels(6.0)));
    f.publish(tx);
    let changed = &f.runtime.frame().objects[row];
    let definition = changed
        .effects
        .as_ref()
        .unwrap()
        .get(attachment)
        .unwrap()
        .definition
        .as_glow()
        .unwrap();
    assert_eq!(definition.intensity(), 1.25);
    assert_eq!(definition.radius(), Pixels(6.0).into());
    let noon_core::ObjectContentRef::Composed(current) = &changed.content else {
        panic!("same target")
    };
    assert!(
        std::sync::Arc::ptr_eq(current, &source_plan),
        "parameter edits must not rebuild or copy the family input plan"
    );
    assert_eq!(f.runtime.frame().objects[0], child_before);
    assert_eq!(
        f.runtime.take_frame_changes(),
        FrameChanges::objects(vec![row])
    );
    assert!(f.runtime.take_spatial_changes().is_empty());
    f.runtime.seek(1.5).unwrap();
    assert_eq!(
        f.runtime.frame().objects[row]
            .effects
            .as_ref()
            .unwrap()
            .get(attachment)
            .unwrap()
            .definition
            .as_glow()
            .unwrap()
            .intensity(),
        1.75
    );
}

#[test]
fn removal_and_replacement_are_single_owner_patches_not_object_recreation() {
    let mut f = Fixture::new(0);
    let old = f.attach(0.4);
    let object = f.object();
    let original = f.runtime.frame().objects[0].clone();
    let mut tx = SemanticMutationTransaction::new();
    let new = tx.create_effect(f.source, "glow", effect(1.4));
    tx.remove_node(old);
    let prepared = tx.prepare(&mut f.store).unwrap();
    let replacement = prepared.planned_node_id(new).unwrap();
    assert_ne!(replacement, old);
    let publication = prepare_semantic_publication(&prepared, &f.index, &f.reachability).unwrap();
    assert!(publication.possible_exits().is_empty());
    assert_eq!(publication.possible_entry_count(), 0);
    assert!(
        matches!(publication.value_transaction().mutations(), [ExecutionPatch::SetEffectStack { expected, effects: Some(value), .. }] if expected.as_ref() == [old] && value.attachments() == [replacement])
    );
    let tx = prepared.into_transaction();
    f.runtime.take_frame_changes();
    f.runtime.take_spatial_changes();
    assert_eq!(f.publish(tx).resolve(new), Some(replacement));
    assert!(f.store.node(old).is_none());
    assert_eq!(f.object(), object);
    let current = &f.runtime.frame().objects[0];
    assert_eq!(current.transform, original.transform);
    assert_eq!(current.style, original.style);
    assert_eq!(current.content, original.content);
    assert_eq!(f.runtime.frame_changes(), &FrameChanges::objects(vec![0]));
    assert!(f.runtime.take_spatial_changes().is_empty());
    let mut remove = SemanticMutationTransaction::new();
    remove.remove_node(replacement);
    f.publish(remove);
    assert!(f.runtime.frame().objects[0].effects.is_none());
    assert!(f.store.effect_by_name(f.source, "glow").unwrap().is_none());
}

#[test]
fn pending_owner_and_attachment_enter_together_without_dropping_the_glow_column() {
    let mut f = Fixture::new(0);
    let mut tx = SemanticMutationTransaction::new();
    let owner = tx.create_node(SemanticNodeCreation::object(circle()));
    let attachment = tx.create_effect(owner, "glow", effect(0.7));
    tx.add_member(f.root, owner);
    let prepared = tx.prepare(&mut f.store).unwrap();
    let id = prepared.planned_node_id(attachment).unwrap();
    let publication = prepare_semantic_publication(&prepared, &f.index, &f.reachability).unwrap();
    assert!(publication.value_transaction().is_empty());
    assert_eq!(publication.possible_entry_count(), 1);
    assert!(
        matches!(publication.conservative_entry_patches(&prepared).as_slice(), [ExecutionPatch::CreateObject(object)] if object.effects.as_ref().unwrap().attachments() == [id])
    );
    let tx = prepared.into_transaction();
    let result = f.publish(tx);
    let object = f
        .index
        .execution_object_id(result.resolve(owner).unwrap())
        .unwrap();
    assert_eq!(
        f.runtime
            .effective_object(object)
            .unwrap()
            .effects
            .as_ref()
            .unwrap()
            .attachments(),
        &[result.resolve(attachment).unwrap()]
    );
}

#[test]
fn detached_edits_remain_inert_then_reentry_uses_final_attachment_generation() {
    let mut f = Fixture::new(0);
    let old = f.attach(0.4);
    let original_slot = f.runtime.frame_index_for_object(f.object()).unwrap();
    let mut detach = SemanticMutationTransaction::new();
    detach.remove_member(f.root, f.source);
    f.publish(detach);
    let mut tx = SemanticMutationTransaction::new();
    let new = tx.create_effect(f.source, "glow", effect(1.1));
    tx.remove_node(old);
    let prepared = tx.prepare(&mut f.store).unwrap();
    assert!(
        prepare_semantic_publication(&prepared, &f.index, &f.reachability)
            .unwrap()
            .value_transaction()
            .is_empty()
    );
    let tx = prepared.into_transaction();
    let new = f.publish(tx).resolve(new).unwrap();
    let mut add = SemanticMutationTransaction::new();
    add.add_member(f.root, f.source);
    f.publish(add);
    assert_eq!(
        f.runtime.frame_index_for_object(f.object()),
        Some(original_slot)
    );
    assert_eq!(
        f.runtime
            .effective_object(f.object())
            .unwrap()
            .effects
            .as_ref()
            .unwrap()
            .attachments(),
        &[new]
    );
}

#[test]
fn writing_a_retired_attachment_remains_an_atomic_semantic_error() {
    let mut f = Fixture::new(0);
    let old = f.attach(0.4);
    let frame = f.runtime.frame().clone();
    let revision = f.store.scene_revision();
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(old, EffectUpdate::default().intensity(1.2));
    tx.remove_node(old);
    assert!(tx.prepare(&mut f.store).is_err());
    assert_eq!(f.runtime.frame(), &frame);
    assert_eq!(f.store.scene_revision(), revision);
    assert_eq!(
        f.store.semantic_effect_state(old).unwrap().definition(),
        effect(0.4).into()
    );
}

#[test]
fn source_profile_is_validated_against_final_staged_style_and_content() {
    for already_attached in [false, true] {
        for edit in [0, 1, 2, 3] {
            let mut f = Fixture::new(0);
            if already_attached {
                f.attach(0.4);
            }
            let before = f.runtime.frame().clone();
            let revision = f.store.scene_revision();
            let mut tx = SemanticMutationTransaction::new();
            if !already_attached {
                tx.create_effect(f.source, "glow", effect(0.4));
            }
            match edit {
                0 => {
                    let mut style = circle().style;
                    style.fill = None;
                    tx.replace_style(f.source, style);
                }
                1 => {
                    let mut style = circle().style;
                    style.stroke = Some(SemanticPaint::Solid(Color::BLUE));
                    style.stroke_width = 0.2;
                    tx.replace_style(f.source, style);
                }
                2 => {
                    tx.replace_content(
                        f.source,
                        StoredGeometry::Line {
                            start: Vec2::ZERO,
                            end: Vec2::new(1.0, 0.0),
                        },
                    );
                }
                _ => {
                    tx.set_property(
                        f.source,
                        SemanticObjectProperty::Scale,
                        SemanticVec3::new(0.0, 1.0, 1.0),
                    );
                }
            }
            let prepared = tx.prepare(&mut f.store).unwrap();
            let publication = prepare_semantic_publication(&prepared, &f.index, &f.reachability);
            // Outlines and lines are ordinary planar capture sources. Singular
            // transforms must still reject the complete staged publication.
            assert_eq!(publication.is_err(), edit == 3);
            drop(prepared);
            assert_eq!(f.store.scene_revision(), revision);
            assert_eq!(f.runtime.frame(), &before);
        }
    }
}

#[test]
fn removing_glow_can_legally_leave_the_source_outside_the_glow_profile() {
    let mut f = Fixture::new(0);
    let old = f.attach(0.4);
    let mut tx = SemanticMutationTransaction::new();
    tx.set_property(
        f.source,
        SemanticObjectProperty::Scale,
        SemanticVec3::new(0.0, 1.0, 1.0),
    );
    tx.remove_node(old);
    f.publish(tx);
    assert!(f.runtime.frame().objects[0].effects.is_none());
    assert_eq!(f.runtime.frame().objects[0].transform.scale.x, 0.0);
}

#[test]
fn stack_profile_overflow_rejects_even_with_an_otherwise_valid_motion_prefix() {
    let mut f = Fixture::new(0);
    f.attach(0.4);
    let before = f.runtime.frame().clone();
    let revision = f.store.scene_revision();
    let mut tx = SemanticMutationTransaction::new();
    tx.set_property(
        f.source,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(1.0, 0.0, 0.0),
    );
    for i in 1..=noon_core::MAX_EFFECT_STACK_LEN {
        tx.create_effect(f.source, format!("extra{i}"), effect(1.4));
    }
    let prepared = tx.prepare(&mut f.store).unwrap();
    assert!(prepare_semantic_publication(&prepared, &f.index, &f.reachability).is_err());
    drop(prepared);
    assert_eq!(f.runtime.frame(), &before);
    assert_eq!(f.store.scene_revision(), revision);
}

#[test]
fn parameter_only_change_stays_on_the_existing_value_lane_and_noop_is_empty() {
    let mut f = Fixture::new(0);
    let old = f.attach(0.4);
    for intensity in [0.4, 0.9] {
        let mut tx = SemanticMutationTransaction::new();
        tx.update_effect(old, EffectUpdate::default().intensity(intensity));
        let prepared = tx.prepare(&mut f.store).unwrap();
        let publication =
            prepare_semantic_publication(&prepared, &f.index, &f.reachability).unwrap();
        if intensity == 0.4 {
            assert!(publication.value_transaction().is_empty());
        } else {
            assert!(
                matches!(publication.value_transaction().mutations(), [ExecutionPatch::SetEffects { effects, .. }] if effects.attachments() == [old] && effects.get(old).unwrap().definition.as_glow().unwrap().intensity() == intensity)
            );
        }
    }
}

#[test]
fn attachment_edits_are_local_among_4096_untouched_objects() {
    let mut f = Fixture::new(4096);
    let untouched = f
        .runtime
        .frame()
        .objects
        .iter()
        .skip(1)
        .cloned()
        .collect::<Vec<_>>();
    let old = f.attach(0.4);
    f.runtime.take_frame_changes();
    f.runtime.take_spatial_changes();
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(old);
    let prepared = tx.prepare(&mut f.store).unwrap();
    let publication = prepare_semantic_publication(&prepared, &f.index, &f.reachability).unwrap();
    assert_eq!(publication.stats().object_states_lowered, 0);
    assert_eq!(publication.value_transaction().mutations().len(), 1);
    assert_eq!(publication.possible_entry_count(), 0);
    assert!(publication.possible_exits().is_empty());
    let tx = prepared.into_transaction();
    f.publish(tx);
    assert_eq!(f.runtime.frame_changes(), &FrameChanges::objects(vec![0]));
    assert!(f.runtime.take_spatial_changes().is_empty());
    assert_eq!(
        f.runtime
            .frame()
            .objects
            .iter()
            .skip(1)
            .cloned()
            .collect::<Vec<_>>(),
        untouched
    );
}

#[test]
fn semantic_removal_retires_only_glow_drivers_and_replay_restores_their_identity() {
    let mut f = Fixture::new(0);
    let old = f.attach(0.4);
    let object = f.object();
    let declaration = f.runtime.frame().objects[0]
        .effects
        .as_ref()
        .unwrap()
        .get(old)
        .unwrap();
    let (property, values) = declaration
        .parameter_channels(&EffectUpdate::default().intensity(2.0))
        .unwrap()
        .pop()
        .unwrap();
    let motion = TrackDefinition {
        id: TrackId::new(2),
        object,
        property: Property::Position,
        values: TrackValues::Vec2 {
            from: Vec2::ZERO,
            to: Vec2::new(2.0, 0.0),
        },
        timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        time_map: Default::default(),
    };
    let effect = TrackDefinition {
        id: TrackId::new(1),
        object,
        property,
        values,
        timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        time_map: Default::default(),
    };
    f.runtime
        .apply_execution_transaction(&ExecutionMutationTransaction::from_mutations([
            ExecutionPatch::AddTrack(effect),
            ExecutionPatch::AddTrack(motion),
        ]))
        .unwrap();
    f.runtime
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    f.runtime.advance_to(0.25).unwrap();
    let first = f.runtime.frame().clone();
    f.runtime.advance_to(0.4).unwrap();
    let mut remove = SemanticMutationTransaction::new();
    remove.remove_node(old);
    f.publish(remove);
    f.runtime.advance_to(0.75).unwrap();
    let after = f.runtime.frame().clone();
    assert!(after.objects[0].effects.is_none());
    assert_eq!(after.objects[0].transform.translation.x, 0.75);
    f.runtime.seal_replay().unwrap();
    for _ in 0..3 {
        f.runtime.seek(0.25).unwrap();
        assert_eq!(f.runtime.frame(), &first);
        f.runtime.seek(0.75).unwrap();
        assert_eq!(f.runtime.frame(), &after);
    }
    assert!(
        f.store.node(old).is_none(),
        "replay is effective history, not resurrection of authored nodes"
    );
}

#[test]
fn cancelled_pending_attachment_is_idle_and_does_not_consume_a_semantic_id() {
    let mut f = Fixture::new(0);
    let frame = f.runtime.frame().clone();
    let revision = f.store.scene_revision();
    let mut tx = SemanticMutationTransaction::new();
    let pending = tx.create_effect(f.source, "glow", effect(0.4));
    tx.remove_node(pending);
    let prepared = tx.prepare(&mut f.store).unwrap();
    assert!(prepared.planned_node_id(pending).is_none());
    let publication = prepare_semantic_publication(&prepared, &f.index, &f.reachability).unwrap();
    assert!(publication.value_transaction().is_empty());
    assert_eq!(publication.possible_entry_count(), 0);
    let result = prepared.commit();
    assert!(result.resolve(pending).is_none());
    assert_eq!(f.store.scene_revision(), revision);
    assert_eq!(f.runtime.frame(), &frame);
}

#[test]
fn removing_the_source_retires_its_attachment_via_the_existing_object_exit() {
    let mut f = Fixture::new(0);
    let old = f.attach(0.4);
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(f.source);
    let prepared = tx.prepare(&mut f.store).unwrap();
    let publication = prepare_semantic_publication(&prepared, &f.index, &f.reachability).unwrap();
    assert!(publication.value_transaction().is_empty());
    assert_eq!(
        publication.possible_exits(),
        &[f.index.execution_object_id(f.source).unwrap()]
    );
    let tx = prepared.into_transaction();
    f.publish(tx);
    assert!(f.store.node(old).is_none());
    assert!(f.store.node(f.source).is_none());
    assert!(!f.reachability.is_object_reachable(f.source));
}

#[test]
fn shared_family_alias_retains_the_one_source_attachment_when_one_path_is_removed() {
    let mut f = Fixture::new(0);
    let old = f.attach(0.4);
    let mut tx = SemanticMutationTransaction::new();
    let alias = tx.create_node(SemanticNodeCreation::family());
    tx.add_member(f.root, alias);
    tx.add_member(alias, f.source);
    f.publish(tx);
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_member(f.root, f.source);
    f.publish(tx);
    assert!(f.reachability.is_object_reachable(f.source));
    assert_eq!(f.runtime.frame().objects.len(), 1);
    assert_eq!(
        f.runtime
            .effective_object(f.object())
            .unwrap()
            .effects
            .as_ref()
            .unwrap()
            .attachments(),
        &[old]
    );
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(old);
    f.publish(tx);
    assert!(f
        .runtime
        .effective_object(f.object())
        .unwrap()
        .effects
        .is_none());
}

#[test]
fn runtime_generation_mismatch_is_rejected_before_the_semantic_commit() {
    let mut f = Fixture::new(0);
    let old = f.attach(0.4);
    let object = f.object();
    // A desynchronized runtime must be detected; the semantic store is not used
    // as an excuse to overwrite whichever runtime attachment happens to exist.
    f.runtime
        .apply_execution_patch(&ExecutionPatch::SetEffectStack {
            object,
            expected: Box::new([old]),
            effects: None,
        })
        .unwrap();
    let frame = f.runtime.frame().clone();
    let revision = f.store.scene_revision();
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(old);
    let prepared = tx.prepare(&mut f.store).unwrap();
    let publication = prepare_semantic_publication(&prepared, &f.index, &f.reachability).unwrap();
    assert!(f
        .runtime
        .preflight_authored_transaction_shape(
            publication.value_transaction(),
            f.runtime.publication_context(),
            prepared.proposed_scene_revision(),
            0,
            true,
        )
        .is_err());
    drop(prepared);
    assert_eq!(f.store.scene_revision(), revision);
    assert!(f.store.node(old).is_some());
    assert_eq!(f.runtime.frame(), &frame);
}
