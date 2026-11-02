use super::*;
use crate::replay::family_tests::{animation, runtime};
use noon_compile::ExecutionPatch;
use noon_core::{CompositionTimeMap, CompositionTimeMapStep, RateFunction};

fn source() -> SceneInstance {
    let mut source = runtime();
    source
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    source.advance_to(1.0).unwrap();
    source
}

fn add_mapped_plan(source: &mut SceneInstance) {
    let mut plan = animation(0.0, 8.0, true);
    plan.time_map = CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
        0.5,
        0.25,
        RateFunction::Linear,
    )]);
    source
        .apply_execution_patch(&ExecutionPatch::AddFamilyAnimation(plan))
        .unwrap();
}

#[test]
fn glyph_plan_byte_rejection_precedes_clone_and_capacity_maintenance() {
    let mut source = source();
    let (mut publisher, mut runtime, mut receiver) = source
        .create_prefix_projection_with_resource_budget(0)
        .unwrap();
    receiver.prepare_seek(&mut runtime, 0.5).unwrap();
    runtime.seek(0.5).unwrap();
    runtime.take_frame_changes();
    let before = runtime.frame().clone();
    let publication = runtime.publication_context();
    add_mapped_plan(&mut source);
    let first = source.advance_to(5.0).unwrap().clone();
    source.advance_to(6.5).unwrap();
    let bytes = source.compiled.prefix_family_payload_bytes(0).unwrap();
    assert!(bytes > 0);
    publisher.resource_budget = ResourceBudget::new(bytes - 1);
    for _ in 0..2 {
        assert!(
            matches!(source.prepare_prefix_update(&mut publisher), Err(PrefixError::ResourceBudgetExceeded { additional_bytes, .. }) if additional_bytes == bytes)
        );
        assert_eq!(publisher.frontier(), 1.0);
        assert_eq!(publisher.family_animations, 0);
        assert_eq!(publisher.extracted_resource_bytes(), 0);
    }
    publisher.resource_budget = ResourceBudget::new(bytes);
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    assert_eq!(batch.resource_bytes(), bytes);
    assert_eq!(batch.stats().resource_payloads, 1);
    assert!(batch.new_object_ids().next().is_none());
    // Permanent receiver byte rejection must also precede plan-table growth.
    assert!(matches!(
        receiver.reserve_append_capacity(&mut runtime, &batch),
        Err(PrefixError::ResourceBudgetExceeded { .. })
    ));
    assert_eq!(runtime.compiled.family_animations().len(), 0);
    assert_eq!(receiver.family_capacity, 0);
    receiver.resource_budget = ResourceBudget::new(bytes);
    assert_eq!(
        receiver.preflight_append(&runtime, &batch),
        Err(PrefixError::FamilyAnimationCapacityRequired { animations: 1 })
    );
    receiver
        .reserve_append_capacity(&mut runtime, &batch)
        .unwrap();
    assert_eq!(runtime.frame(), &before);
    assert_eq!(runtime.publication_context(), publication);
    receiver.append(&mut runtime, batch).unwrap();
    assert_eq!(runtime.frame(), &before);
    assert_eq!(runtime.publication_context(), publication);
    assert_eq!(receiver.retained_resource_bytes(), bytes);
    assert!(runtime.take_frame_changes().is_empty());
    drop(source);
    for _ in 0..2 {
        receiver.prepare_seek(&mut runtime, 5.0).unwrap();
        assert_eq!(runtime.seek(5.0).unwrap(), &first);
        receiver.prepare_seek(&mut runtime, 0.5).unwrap();
        assert_eq!(runtime.seek(0.5).unwrap(), &before);
        assert_eq!(receiver.retained_resource_bytes(), bytes);
    }
}

#[test]
fn initial_glyph_plan_is_charged_once_before_projection_clone() {
    let mut source = source();
    add_mapped_plan(&mut source);
    source.advance_to(6.5).unwrap();
    let bytes = source.compiled.prefix_family_payload_bytes(0).unwrap();
    assert!(
        matches!(source.create_prefix_projection_with_resource_budget(bytes - 1), Err(PrefixError::ResourceBudgetExceeded { additional_bytes, .. }) if additional_bytes == bytes)
    );
    let (mut publisher, mut runtime, mut receiver) = source
        .create_prefix_projection_with_resource_budget(bytes)
        .unwrap();
    assert_eq!(receiver.retained_resource_bytes(), bytes);
    assert_eq!(runtime.compiled.family_animations().len(), 1);
    source.advance_to(7.0).unwrap();
    source.seal_replay().unwrap();
    let terminal = source.prepare_prefix_finish(&mut publisher).unwrap();
    assert_eq!(terminal.resource_bytes(), 0);
    assert_eq!(terminal.stats().resource_payloads, 0);
    receiver.append(&mut runtime, terminal).unwrap();
    assert_eq!(receiver.retained_resource_bytes(), bytes);
}

#[test]
fn seek_between_reservation_and_adoption_rechecks_scheduler_capacity() {
    use noon_core::{ObjectId, Property, TrackDefinition, TrackId, TrackTiming, TrackValues};
    let mut source = source();
    source
        .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
            id: TrackId::new(7),
            object: ObjectId::new(40),
            property: Property::Opacity,
            values: TrackValues::Scalar { from: 1.0, to: 0.5 },
            timing: TrackTiming::new(1.0, 2.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        }))
        .unwrap();
    source.advance_to(6.5).unwrap();
    let (mut publisher, mut runtime, mut receiver) = source.create_prefix_projection().unwrap();
    source
        .apply_execution_patch(&ExecutionPatch::AddFamilyAnimation(animation(
            7.0, 2.0, false,
        )))
        .unwrap();
    source.advance_to(10.0).unwrap();
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    // At t=0 the earlier property channel is retired; its free group can be
    // reused by the new glyph plan without relocating scheduler storage.
    receiver
        .reserve_append_capacity(&mut runtime, &batch)
        .unwrap();
    receiver.preflight_append(&runtime, &batch).unwrap();
    receiver.prepare_seek(&mut runtime, 2.0).unwrap();
    runtime.seek(2.0).unwrap();
    let frame = runtime.frame().clone();
    let publication = runtime.publication_context();
    // Selecting the earlier channel consumes that free slot. Re-admit the same
    // batch instead of allocating unexpectedly inside ordinary adoption.
    assert_eq!(
        receiver.preflight_append(&runtime, &batch),
        Err(PrefixError::FamilyAnimationCapacityRequired { animations: 1 })
    );
    receiver
        .reserve_append_capacity(&mut runtime, &batch)
        .unwrap();
    receiver.append(&mut runtime, batch).unwrap();
    assert_eq!(runtime.frame(), &frame);
    assert_eq!(runtime.publication_context(), publication);
}
