//! Public native authoring -> canonical publication -> actual worker codec ->
//! installed frame/resource owner. Pixel qualification remains a separate gate.
use super::*;
use crate::{
    RetainedFamilyExecutionDeltaEncoder, RetainedResourceRetirements,
    TransportComposedSourceHandle, TransportSlotId,
};
use noon::{
    effects::{Blur, EffectUpdate, Pixels},
    ExecutionSession, Mobject, MobjectFamily, Scene,
};
use noon_compile::CompiledComposedSourceErrorKind as Kind;
use noon_core::{
    ComposedSource, ObjectContentRef, ObjectId, SemanticMutationTransaction, SemanticNodeId,
};
use std::sync::Arc;

fn empty() -> RetainedResourceBundle {
    RetainedResourceBundle::capture(
        [],
        &noon_core::TextResourceArena::new(),
        &noon_core::GeometryResourceArena::new(),
        &noon_core::FontResourceArena::new(),
    )
    .unwrap()
}
fn encode(
    encoder: &mut RetainedFamilyExecutionDeltaEncoder,
    session: &mut ExecutionSession,
    snapshot: bool,
) -> RetainedFamilyExecutionDeltaEnvelope {
    let camera = session.inspection_camera().unwrap();
    let publication = session.take_renderer_publication();
    let frame = publication.frame();
    let planned = publication.planned_family_frame();
    let mut delta = if snapshot {
        encoder
            .encode_planned_snapshot_indices_with_context(
                &planned,
                publication.family_animation_plans(),
                camera,
                publication.context(),
                publication
                    .painter_order()
                    .iter()
                    .map(|&index| index as usize),
            )
            .unwrap()
    } else {
        encoder
            .encode_planned_incremental_with_painter_order_and_context(
                &planned,
                publication.family_animation_plans(),
                publication.changes(),
                camera,
                publication.context(),
                publication.painter_order(),
            )
            .unwrap()
            .unwrap()
    };
    let texts = delta
        .retained
        .objects
        .iter()
        .filter_map(|row| {
            frame
                .objects
                .iter()
                .find(|source| source.id == row.object)?
                .content
                .text()
        })
        .collect::<Vec<_>>();
    encoder
        .attach_resource_additions(
            &mut delta,
            texts,
            publication.text_resources(),
            publication.geometry_resources(),
            publication.font_resources(),
            publication.raster_image_resources(),
        )
        .unwrap();
    encoder.compact_dense_rows(&mut delta);
    delta
}
fn apply(
    mirror: &mut InstalledRetainedExecutionMirror,
    delta: &RetainedFamilyExecutionDeltaEnvelope,
) -> Result<(RetainedTransportApplyOutcome, FrameChanges), InstalledExecutionError> {
    mirror.apply_json(&serde_json::to_string(delta).unwrap())
}
fn source_handle(
    delta: &RetainedFamilyExecutionDeltaEnvelope,
    owner: ObjectId,
) -> TransportComposedSourceHandle {
    match delta
        .retained
        .objects
        .iter()
        .find(|row| row.object == owner)
        .unwrap()
        .content
    {
        TransportObjectContent::Composed { source } => source,
        _ => panic!("composed row"),
    }
}
fn plan(mirror: &InstalledRetainedExecutionMirror, owner: ObjectId) -> &Arc<ComposedSource> {
    let index = mirror.wire.frame_index_for_object(owner).unwrap();
    match &mirror.frame().unwrap().objects[index].content {
        ObjectContentRef::Composed(source) => source,
        _ => panic!("live composed plan"),
    }
}
fn assert_coherent(session: &mut ExecutionSession, mirror: &InstalledRetainedExecutionMirror) {
    let publication = session.take_renderer_publication();
    let expected = publication
        .painter_order()
        .iter()
        .map(|&i| publication.frame().objects[i as usize].id)
        .collect::<Vec<_>>();
    let actual = mirror
        .painter_order()
        .iter()
        .map(|&i| mirror.frame().unwrap().objects[i as usize].id)
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    for &index in mirror.painter_order() {
        let actual = &mirror.frame().unwrap().objects[index as usize];
        let source = publication
            .frame()
            .objects
            .iter()
            .find(|row| row.id == actual.id)
            .unwrap();
        assert_eq!(actual.effects, source.effects);
        assert_eq!(actual.transform, source.transform);
        assert_eq!(actual.style, source.style);
        assert_eq!(actual.draw_enabled, source.draw_enabled);
        assert_eq!(
            actual.content.composed_source(),
            source.content.composed_source()
        );
        if let Some(expected) = source.content.image() {
            let actual = actual.content.image().expect("same image content kind");
            assert_eq!(actual.sampling(), expected.sampling());
            assert_eq!(
                mirror.resources().images().get(actual.resource()).unwrap(),
                publication
                    .raster_image_resources()
                    .get(expected.resource())
                    .unwrap()
            );
        } else {
            assert_eq!(actual.content.geometry(), source.content.geometry());
        }
    }
    let changes = FrameChanges::default();
    let resolved = mirror.renderer_publication(&changes).unwrap();
    assert!(
        std::ptr::eq(resolved.frame(), mirror.frame().unwrap()),
        "renderer borrows the installed frame"
    );
}

struct Fixture {
    scene: Scene,
    session: ExecutionSession,
    encoder: RetainedFamilyExecutionDeltaEncoder,
    mirror: InstalledRetainedExecutionMirror,
    inner: MobjectFamily,
    outer: MobjectFamily,
    leaves: Vec<Mobject>,
    inner_id: ObjectId,
    outer_id: ObjectId,
    effect: SemanticNodeId,
    snapshot: RetainedFamilyExecutionDeltaEnvelope,
}
impl Fixture {
    fn new() -> Self {
        let mut scene = Scene::new();
        let a = scene.circle(0.4).unwrap();
        let b = scene.image_rgba8(1, 1, vec![64, 128, 192, 96]).unwrap();
        let c = scene.circle(0.3).unwrap();
        let outside = scene.square(0.3).unwrap();
        let inner = scene.family(&[(&a).into(), (&b).into()]).unwrap();
        let outer = scene.family(&[(&inner).into(), (&c).into()]).unwrap();
        scene
            .add_effect(
                &inner,
                Blur::new(EffectUpdate::default().radius(Pixels(2.0))).unwrap(),
                "inner",
            )
            .unwrap();
        scene
            .set_glow(&outer, EffectUpdate::default().radius(Pixels(3.0)))
            .unwrap();
        scene
            .add_many(&[(&outer).into(), (&outside).into()])
            .unwrap();
        let effect = inner.get_effect("inner").unwrap().node_id();
        let mut session = scene.execution_session().unwrap();
        let inner_id = session.execution_object_id(inner.node_id()).unwrap();
        let outer_id = session.execution_object_id(outer.node_id()).unwrap();
        let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(73);
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(&empty().encode_binary().unwrap())
                .unwrap();
        let snapshot = encode(&mut encoder, &mut session, true);
        assert_eq!(
            snapshot
                .resource_additions
                .as_ref()
                .unwrap()
                .composed_source_count(),
            2
        );
        apply(&mut mirror, &snapshot).unwrap();
        Self {
            scene,
            session,
            encoder,
            mirror,
            inner,
            outer,
            leaves: vec![a, b, c, outside],
            inner_id,
            outer_id,
            effect,
            snapshot,
        }
    }
    fn parameter(&mut self) -> RetainedFamilyExecutionDeltaEnvelope {
        let mut tx = SemanticMutationTransaction::new();
        tx.update_effect(self.effect, EffectUpdate::default().radius(Pixels(4.0)));
        self.scene.live(&mut self.session).apply(tx).unwrap();
        encode(&mut self.encoder, &mut self.session, false)
    }
    fn slot(&self, id: ObjectId) -> TransportSlotId {
        self.snapshot
            .retained
            .objects
            .iter()
            .find(|row| row.object == id)
            .unwrap()
            .slot
    }
}

#[test]
fn nested_public_groups_cross_worker_codec_reuse_parameter_resources_and_retire_on_detach() {
    let mut f = Fixture::new();
    assert_coherent(&mut f.session, &f.mirror);
    let inner_key = source_handle(&f.snapshot, f.inner_id);
    assert_eq!(inner_key.version, 0);
    assert_eq!(inner_key.owner, f.inner_id);
    assert_eq!(plan(&f.mirror, f.inner_id).parent_scope(), Some(f.outer_id));
    assert!(Arc::ptr_eq(
        plan(&f.mirror, f.inner_id),
        f.mirror.resources().composed_source(inner_key).unwrap()
    ));
    let original = Arc::downgrade(plan(&f.mirror, f.inner_id));
    let parameter = f.parameter();
    assert_eq!(parameter.retained.objects.len(), 1);
    assert_eq!(source_handle(&parameter, f.inner_id), inner_key);
    assert!(parameter.resource_additions.is_none());
    assert!(!serde_json::to_string(&parameter)
        .unwrap()
        .contains("\"members\""));
    apply(&mut f.mirror, &parameter).unwrap();
    assert!(Arc::ptr_eq(
        &original.upgrade().unwrap(),
        plan(&f.mirror, f.inner_id)
    ));
    assert_eq!(
        f.mirror.wire.composed_validation_stats(),
        noon_compile::ComposedValidationStats::default()
    );
    assert_coherent(&mut f.session, &f.mirror);
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_member(f.scene.root(), f.outer.node_id());
    f.scene.live(&mut f.session).apply(tx).unwrap();
    let detached = encode(&mut f.encoder, &mut f.session, false);
    assert_eq!(detached.resource_retirements.composed_sources.len(), 2);
    apply(&mut f.mirror, &detached).unwrap();
    assert_eq!(f.mirror.resources().composed_source_count(), 0);
    assert!(
        original.upgrade().is_none(),
        "retired frame tombstones do not retain source lists"
    );
    assert!(f.mirror.resource_roots.is_empty());
    assert_coherent(&mut f.session, &f.mirror);
    let mut tx = SemanticMutationTransaction::new();
    tx.add_member(f.scene.root(), f.outer.node_id());
    f.scene.live(&mut f.session).apply(tx).unwrap();
    let readded = encode(&mut f.encoder, &mut f.session, false);
    assert_eq!(
        source_handle(&readded, f.inner_id).version,
        readded.retained.sequence
    );
    assert_eq!(
        f.session.execution_object_id(f.inner.node_id()),
        Some(f.inner_id)
    );
    apply(&mut f.mirror, &readded).unwrap();
    assert_coherent(&mut f.session, &f.mirror);
}

#[test]
fn malformed_installed_plan_rolls_back_frame_resources_and_sequence_then_same_key_retries() {
    let mut f = Fixture::new();
    let mut delta = f.parameter();
    let old = source_handle(&delta, f.inner_id);
    let old_plan = plan(&f.mirror, f.inner_id).clone();
    let key = TransportComposedSourceHandle {
        version: delta.retained.sequence,
        ..old
    };
    delta.retained.objects[0].content = TransportObjectContent::Composed { source: key };
    delta.resource_retirements.composed_sources.push(old);
    let mut members = old_plan.members().to_vec();
    members[0] = ObjectId::new(members[0].get() ^ (1 << 32));
    let invalid = ComposedSource::new(members.clone(), old_plan.parent_scope()).unwrap();
    delta.resource_additions = Some(empty().with_composed_sources([(key, &invalid)]).unwrap());
    let before = f.mirror.frame().unwrap().clone();
    let context = f.mirror.publication_context();
    assert!(
        matches!(apply(&mut f.mirror, &delta), Err(InstalledExecutionError::Transport(RetainedExecutionTransportError::InvalidComposedSource(error))) if error.kind == Kind::UnknownInput(members[0]))
    );
    assert_eq!(f.mirror.frame().unwrap(), &before);
    assert_eq!(f.mirror.publication_context(), context);
    assert_eq!(f.mirror.wire.applied_sequence(), Some(0));
    assert_eq!(f.mirror.resources().composed_source_count(), 2);
    assert!(f.mirror.resources().composed_source(key).is_none());
    assert!(
        matches!(f.mirror.wire.resolve_transport_object_state(&delta.retained.objects[0]), Err(RetainedExecutionTransportError::UnknownComposedSource(rejected)) if rejected == key)
    );
    delta.resource_additions = Some(
        empty()
            .with_composed_sources([(key, old_plan.as_ref())])
            .unwrap(),
    );
    apply(&mut f.mirror, &delta).unwrap();
    assert_eq!(f.mirror.wire.applied_sequence(), Some(1));
    assert_eq!(f.mirror.resources().composed_source_count(), 2);
    assert!(f.mirror.resources().composed_source(old).is_none());
    assert_coherent(&mut f.session, &f.mirror);
}

#[test]
fn interleaved_painter_splice_rejects_before_any_worker_publication() {
    let mut f = Fixture::new();
    let mut delta = f.parameter();
    delta.retained.objects.clear();
    delta.family_states.clear();
    let ids = f
        .leaves
        .iter()
        .map(|leaf| f.session.execution_object_id(leaf.node_id()).unwrap())
        .collect::<Vec<_>>();
    delta.retained.painter_order = Some(crate::RetainedPainterOrderDelta {
        start: 0,
        end: 4,
        slots: [ids[0], ids[3], ids[1], ids[2]]
            .into_iter()
            .map(|id| f.slot(id))
            .collect(),
    });
    let before = f.mirror.frame().unwrap().clone();
    let order = f.mirror.painter_order().to_vec();
    assert!(
        matches!(apply(&mut f.mirror, &delta), Err(InstalledExecutionError::Transport(RetainedExecutionTransportError::InvalidComposedSource(error))) if error.kind == Kind::NonContiguousInput)
    );
    assert_eq!(f.mirror.frame().unwrap(), &before);
    assert_eq!(f.mirror.painter_order(), order);
    assert_eq!(f.mirror.wire.applied_sequence(), Some(0));
}

#[test]
fn source_handle_provenance_unknown_keys_and_live_retirement_fail_atomically() {
    let mut f = Fixture::new();
    let valid = f.parameter();
    let key = source_handle(&valid, f.inner_id);
    let before = f.mirror.frame().unwrap().clone();
    for wrong in [
        TransportComposedSourceHandle { session: 74, ..key },
        TransportComposedSourceHandle {
            owner: ObjectId::new(key.owner.get() ^ (1 << 32)),
            ..key
        },
        TransportComposedSourceHandle { version: 99, ..key },
    ] {
        let mut invalid = valid.clone();
        invalid.retained.objects[0].content = TransportObjectContent::Composed { source: wrong };
        assert!(apply(&mut f.mirror, &invalid).is_err());
        assert_eq!(f.mirror.frame().unwrap(), &before);
        assert_eq!(f.mirror.wire.applied_sequence(), Some(0));
    }
    let mut unknown = valid.clone();
    let uninstalled = TransportComposedSourceHandle { version: 1, ..key };
    unknown.retained.objects[0].content = TransportObjectContent::Composed {
        source: uninstalled,
    };
    unknown.resource_retirements.composed_sources.push(key);
    assert!(
        matches!(apply(&mut f.mirror, &unknown), Err(InstalledExecutionError::Transport(RetainedExecutionTransportError::UnknownComposedSource(rejected))) if rejected == uninstalled)
    );
    let mut invalid = valid.clone();
    invalid.resource_retirements = RetainedResourceRetirements {
        composed_sources: vec![key],
        ..Default::default()
    };
    assert!(
        matches!(apply(&mut f.mirror, &invalid), Err(InstalledExecutionError::Resource(RetainedResourceTransportError::RetiredLiveComposedSource(rejected))) if rejected == key)
    );
    assert_eq!(f.mirror.frame().unwrap(), &before);
    apply(&mut f.mirror, &valid).unwrap();
    assert_coherent(&mut f.session, &f.mirror);
}

#[test]
fn parent_cycle_and_retirement_of_unchanged_child_parent_reject_atomically() {
    let mut f = Fixture::new();
    let valid = f.parameter();
    let before = f.mirror.frame().unwrap().clone();
    let old_key = source_handle(&f.snapshot, f.outer_id);
    let key = TransportComposedSourceHandle {
        version: 1,
        ..old_key
    };
    let old_plan = plan(&f.mirror, f.outer_id);
    let cyclic = ComposedSource::new(old_plan.members().iter().copied(), Some(f.inner_id)).unwrap();
    let mut cycle = valid.clone();
    let mut outer_row = f
        .snapshot
        .retained
        .objects
        .iter()
        .find(|row| row.object == f.outer_id)
        .unwrap()
        .clone();
    outer_row.content = TransportObjectContent::Composed { source: key };
    cycle.retained.objects.push(outer_row);
    cycle.resource_retirements.composed_sources.push(old_key);
    cycle.resource_additions = Some(empty().with_composed_sources([(key, &cyclic)]).unwrap());
    assert!(
        matches!(apply(&mut f.mirror, &cycle), Err(InstalledExecutionError::Transport(RetainedExecutionTransportError::InvalidComposedSource(error))) if error.kind == Kind::ParentCycle)
    );
    assert_eq!(f.mirror.frame().unwrap(), &before);
    assert_eq!(f.mirror.resources().composed_source_count(), 2);
    let mut removed = valid.clone();
    removed.retained.objects.clear();
    removed.family_states.clear();
    removed.retained.removed_slots.push(f.slot(f.outer_id));
    let order = f
        .mirror
        .painter_order()
        .iter()
        .map(|&index| f.mirror.frame().unwrap().objects[index as usize].id)
        .collect::<Vec<_>>();
    let rank = order.iter().position(|&id| id == f.outer_id).unwrap();
    removed.retained.painter_order = Some(crate::RetainedPainterOrderDelta {
        start: rank as u32,
        end: order.len() as u32,
        slots: order[rank..]
            .iter()
            .filter(|&&id| id != f.outer_id)
            .map(|&id| f.slot(id))
            .collect(),
    });
    removed.resource_retirements.composed_sources.push(old_key);
    assert!(
        matches!(apply(&mut f.mirror, &removed), Err(InstalledExecutionError::Transport(RetainedExecutionTransportError::InvalidComposedSource(error))) if error.kind == Kind::InvalidParent(f.outer_id))
    );
    assert_eq!(f.mirror.frame().unwrap(), &before);
    assert_eq!(f.mirror.wire.applied_sequence(), Some(0));
    apply(&mut f.mirror, &valid).unwrap();
}

#[test]
fn source_additions_cannot_preload_unused_keys_or_omit_released_plan_retirement() {
    let mut f = Fixture::new();
    let valid = f.parameter();
    let old_key = source_handle(&valid, f.inner_id);
    let old_plan = plan(&f.mirror, f.inner_id).clone();
    let new_key = TransportComposedSourceHandle {
        version: 1,
        ..old_key
    };
    let mut missing = valid.clone();
    missing.retained.objects[0].content = TransportObjectContent::Composed { source: new_key };
    missing.resource_additions = Some(
        empty()
            .with_composed_sources([(new_key, old_plan.as_ref())])
            .unwrap(),
    );
    assert!(
        matches!(apply(&mut f.mirror, &missing), Err(InstalledExecutionError::Resource(RetainedResourceTransportError::MissingComposedSourceRetirement(rejected))) if rejected == old_key)
    );
    let mut unused = valid.clone();
    let unused_key = TransportComposedSourceHandle {
        owner: ObjectId::new(900_000),
        version: 1,
        ..old_key
    };
    unused.resource_additions = Some(
        empty()
            .with_composed_sources([(unused_key, old_plan.as_ref())])
            .unwrap(),
    );
    assert!(
        matches!(apply(&mut f.mirror, &unused), Err(InstalledExecutionError::Resource(RetainedResourceTransportError::InvalidComposedSource { handle, .. })) if handle == unused_key)
    );
    let mut future = missing.clone();
    let future_key = TransportComposedSourceHandle {
        version: 2,
        ..old_key
    };
    future.retained.objects[0].content = TransportObjectContent::Composed { source: future_key };
    future.resource_retirements.composed_sources.push(old_key);
    future.resource_additions = Some(
        empty()
            .with_composed_sources([(future_key, old_plan.as_ref())])
            .unwrap(),
    );
    assert!(
        matches!(apply(&mut f.mirror, &future), Err(InstalledExecutionError::Resource(RetainedResourceTransportError::InvalidComposedSource { handle, .. })) if handle == future_key)
    );
    assert_eq!(f.mirror.resources().composed_source_count(), 2);
    assert_eq!(f.mirror.wire.applied_sequence(), Some(0));
    apply(&mut f.mirror, &valid).unwrap();
}

#[test]
fn membership_churn_retires_both_nested_plans_and_keeps_resource_closure_bounded() {
    let mut f = Fixture::new();
    let mut current = f.leaves[0].node_id();
    for step in 0..128 {
        let old_inner = Arc::downgrade(plan(&f.mirror, f.inner_id));
        let old_outer = Arc::downgrade(plan(&f.mirror, f.outer_id));
        let mut tx = SemanticMutationTransaction::new();
        let next = tx.create_node(noon_core::SemanticNodeCreation::object(
            noon_core::SemanticObjectState::new(noon_core::StoredGeometry::Circle {
                radius: 0.25 + step as f32 * 0.001,
            }),
        ));
        tx.add_member(f.inner.node_id(), next);
        tx.remove_node(current);
        current = f
            .scene
            .live(&mut f.session)
            .apply(tx)
            .unwrap()
            .resolve(next)
            .unwrap();
        let delta = encode(&mut f.encoder, &mut f.session, false);
        assert_eq!(
            delta
                .resource_additions
                .as_ref()
                .unwrap()
                .composed_source_count(),
            2
        );
        assert_eq!(delta.resource_retirements.composed_sources.len(), 2);
        assert_eq!(
            source_handle(&delta, f.inner_id).version,
            delta.retained.sequence
        );
        apply(&mut f.mirror, &delta).unwrap();
        assert!(old_inner.upgrade().is_none() && old_outer.upgrade().is_none());
        assert_eq!(f.mirror.resources().composed_source_count(), 2);
        assert_eq!(f.mirror.resources().image_count(), 1);
        assert_coherent(&mut f.session, &f.mirror);
    }
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_member(f.scene.root(), f.outer.node_id());
    f.scene.live(&mut f.session).apply(tx).unwrap();
    let delta = encode(&mut f.encoder, &mut f.session, false);
    apply(&mut f.mirror, &delta).unwrap();
    assert_eq!(f.mirror.resources().composed_source_count(), 0);
    assert_eq!(f.mirror.resources().image_count(), 0);
    assert!(f.mirror.resource_roots.is_empty());
}

#[test]
fn dense_style_updates_preserve_new_and_changed_effect_definitions_as_full_rows() {
    let f = Fixture::new();
    let source = f
        .snapshot
        .retained
        .objects
        .iter()
        .find(|row| {
            row.object
                == f.session
                    .execution_object_id(f.leaves[0].node_id())
                    .unwrap()
        })
        .unwrap();
    let mut delta = f.snapshot.clone();
    delta.retained.objects = (0..128)
        .map(|i| {
            let mut row = source.clone();
            row.slot = TransportSlotId {
                slot: i,
                generation: 0,
            };
            row.object = ObjectId::new(1000 + i as u64);
            row.order = i;
            row
        })
        .collect();
    delta.family_states.clear();
    delta.family_plans.clear();
    delta.resource_additions = None;
    let mut compactor = RetainedFamilyExecutionDeltaEncoder::new(73);
    let mut mirror = RetainedExecutionFrameMirror::default();
    compactor.compact_dense_rows(&mut delta);
    mirror.apply(delta.retained.clone()).unwrap();
    delta.retained.snapshot = false;
    delta.retained.sequence = 1;
    for row in &mut delta.retained.objects {
        row.style.opacity = 0.8;
    }
    let mut seed = delta.clone();
    compactor.compact_dense_rows(&mut seed);
    mirror.apply(seed.retained).unwrap();
    for step in 0..2 {
        delta.retained.sequence += 1;
        for row in &mut delta.retained.objects {
            row.style.opacity -= 0.1;
        }
        let mut effect = f
            .snapshot
            .retained
            .objects
            .iter()
            .find(|row| row.object == f.inner_id)
            .unwrap()
            .effects[0];
        effect.definition = crate::TransportEffectDefinition::Blur {
            radius: noon_core::EffectRadius::Pixels(2.0 + step as f64),
        };
        delta.retained.objects[0].effects = vec![effect];
        let mut compact = delta.clone();
        compactor.compact_dense_rows(&mut compact);
        assert_eq!(compact.retained.objects.len(), 1);
        assert_eq!(compact.retained.objects[0].effects, [effect]);
        assert_eq!(compact.retained.object_patches.len(), 127);
        mirror.apply(compact.retained).unwrap();
        let installed = mirror.frame().unwrap().objects[0]
            .effects
            .as_ref()
            .unwrap()
            .get(effect.attachment)
            .unwrap();
        assert_eq!(
            installed.definition,
            Blur::new(EffectUpdate::default().radius(Pixels(2.0 + step as f64)))
                .unwrap()
                .into()
        );
    }
    delta.retained.sequence += 1;
    for row in &mut delta.retained.objects {
        row.style.opacity -= 0.1;
    }
    delta.retained.objects[1].draw_enabled = false;
    let mut compact = delta.clone();
    compactor.compact_dense_rows(&mut compact);
    assert_eq!(compact.retained.objects.len(), 1);
    assert_eq!(compact.retained.objects[0].slot.slot, 1);
    assert_eq!(compact.retained.object_patches.len(), 127);
    mirror.apply(compact.retained).unwrap();
    assert!(!mirror.frame().unwrap().objects[1].draw_enabled);
}

#[test]
fn one_atomic_delta_resolves_pending_equal_interval_scopes_before_members_and_parent() {
    let scene = Scene::new();
    let mut session = scene.execution_session().unwrap();
    let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(74);
    let mut mirror =
        InstalledRetainedExecutionMirror::from_bundle_bytes(&empty().encode_binary().unwrap())
            .unwrap();
    let initial = encode(&mut encoder, &mut session, true);
    apply(&mut mirror, &initial).unwrap();
    let mut tx = SemanticMutationTransaction::new();
    let outer = tx.create_node(noon_core::SemanticNodeCreation::family());
    let inner = tx.create_node(noon_core::SemanticNodeCreation::family());
    let a = tx.create_node(noon_core::SemanticNodeCreation::object(
        noon_core::SemanticObjectState::new(noon_core::StoredGeometry::Circle { radius: 0.4 }),
    ));
    let b = tx.create_node(noon_core::SemanticNodeCreation::object(
        noon_core::SemanticObjectState::new(noon_core::StoredGeometry::Circle { radius: 0.3 }),
    ));
    tx.create_effect(outer, "soften", Blur::default());
    tx.create_effect(inner, "soften", Blur::default());
    tx.add_member(inner, a)
        .add_member(inner, b)
        .add_member(outer, inner)
        .add_member(scene.root(), outer);
    let committed = scene.live(&mut session).apply(tx).unwrap();
    let outer_id = session
        .execution_object_id(committed.resolve(outer).unwrap())
        .unwrap();
    let inner_id = session
        .execution_object_id(committed.resolve(inner).unwrap())
        .unwrap();
    let mut delta = encode(&mut encoder, &mut session, false);
    delta.retained.objects.reverse();
    apply(&mut mirror, &delta).unwrap();
    assert_eq!(
        plan(&mirror, inner_id).members(),
        plan(&mirror, outer_id).members()
    );
    assert_eq!(plan(&mirror, inner_id).parent_scope(), Some(outer_id));
    assert_coherent(&mut session, &mirror);
}
