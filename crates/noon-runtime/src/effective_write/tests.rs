use noon_compile::{CompiledObject, CompiledResources, CompiledScene, ExecutionMutationTransaction};
use noon_core::{CompositionTimeMap, GeometryRef, RateFunction, TrackDefinition, TrackId, TrackTiming, TrackValues};

use super::*;
use crate::{EvaluationError, PreparedFrameCommitError, ReplayLimits, SceneInstance};

fn object() -> ObjectId { ObjectId::new(1) }

fn base_style() -> Style {
    Style { fill: Some(Color::BLUE), stroke: Some(Color::WHITE), stroke_width: 3.0, opacity: 0.8, ..Style::default() }
}

fn scene(objects: usize, tracks: &[TrackDefinition]) -> CompiledScene {
    CompiledScene::compile_objects(
        (1..=objects).map(|id| CompiledObject::new(
            ObjectId::new(id as u64), GeometryRef::circle(1.0),
            Transform2D::IDENTITY, base_style(),
        )).collect(), tracks,
    ).unwrap()
}

fn commit(instance: &mut SceneInstance, time: f64, writes: &[EffectivePropertyWrite]) {
    let phase = instance.prepare_advance_to(time).unwrap();
    let batch = instance.prepare_effective_property_batch(writes).unwrap();
    instance.commit_prepared_frame(phase, batch).unwrap();
}

fn track(id: u64, property: Property, values: TrackValues) -> TrackDefinition {
    TrackDefinition { id: TrackId::new(id), object: object(), property, values,
        timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear), time_map: CompositionTimeMap::default() }
}

#[test]
fn scale_and_fill_preserve_newly_evaluated_translation_rotation_and_opacity() {
    let mut instance = SceneInstance::new(scene(1, &[
        track(1, Property::Position, TrackValues::Vec2 { from: Vec2::ZERO, to: Vec2::new(8.0, 4.0) }),
        track(2, Property::Rotation, TrackValues::Scalar { from: 0.0, to: 2.0 }),
        track(3, Property::Opacity, TrackValues::Scalar { from: 0.8, to: 0.4 }),
    ]));
    instance.take_frame_changes();
    let before = instance.publication_context();
    // Prepare the narrow driver while the published object is still at t=0.
    // Applying it after timeline preparation must not restore that old snapshot.
    let writes = [
        EffectivePropertyWrite::Scale { object: object(), scale: Vec2::new(1.2, 1.2) },
        EffectivePropertyWrite::Fill { object: object(), fill: Some(Color::YELLOW) },
    ];
    let batch = instance.prepare_effective_property_batch(&writes).unwrap();
    let phase = instance.prepare_advance_to(1.0).unwrap();
    instance.commit_prepared_frame(phase, batch).unwrap();
    let row = instance.effective_object(object()).unwrap();
    assert_eq!(row.transform.translation, Vec2::new(4.0, 2.0));
    assert_eq!(row.transform.rotation, 1.0);
    assert_eq!(row.transform.scale, Vec2::new(1.2, 1.2));
    assert_eq!(row.style.fill, Some(Color::YELLOW));
    assert_eq!(row.style.stroke, base_style().stroke);
    assert_eq!(row.style.stroke_width, base_style().stroke_width);
    assert!((row.style.opacity - 0.6).abs() < 1e-6);
    let after = instance.publication_context();
    assert_eq!(after.scene_revision(), before.scene_revision());
    assert_eq!(after.execution_revision(), before.execution_revision());
    assert_eq!(after.frame_epoch(), before.frame_epoch().checked_next().unwrap());
    assert_eq!(instance.take_frame_changes().object_indices(), &[0]);
    assert_eq!(instance.compiled.objects()[0].base_transform, Transform2D::IDENTITY);
    assert_eq!(instance.compiled.objects()[0].base_style, base_style());
}

fn assign_expected(transform: &mut Transform2D, style: &mut Style, write: EffectivePropertyWrite) {
    match write {
        EffectivePropertyWrite::Transform { transform: value, .. } => *transform = value,
        EffectivePropertyWrite::Style { style: value, .. } => *style = value,
        EffectivePropertyWrite::Translation { translation, .. } => transform.translation = translation,
        EffectivePropertyWrite::Rotation { rotation, .. } => transform.rotation = rotation,
        EffectivePropertyWrite::Scale { scale, .. } => transform.scale = scale,
        EffectivePropertyWrite::Fill { fill, .. } => style.fill = fill,
        EffectivePropertyWrite::Stroke { stroke, .. } => style.stroke = stroke,
        EffectivePropertyWrite::StrokeWidth { stroke_width, .. } => style.stroke_width = stroke_width,
        EffectivePropertyWrite::Opacity { opacity, .. } => style.opacity = opacity,
    }
}

#[test]
fn mixed_whole_and_component_writes_preserve_supplied_order() {
    let object = object();
    let pool = [
        EffectivePropertyWrite::Transform { object, transform: Transform2D { translation: Vec2::new(7.0, 2.0), rotation: 0.5, scale: Vec2::new(3.0, 4.0) } },
        EffectivePropertyWrite::Transform { object, transform: Transform2D::IDENTITY },
        EffectivePropertyWrite::Style { object, style: Style { fill: Some(Color::RED), stroke: None, opacity: 0.3, ..base_style() } },
        EffectivePropertyWrite::Style { object, style: base_style() },
        EffectivePropertyWrite::Translation { object, translation: Vec2::new(-3.0, 5.0) },
        EffectivePropertyWrite::Rotation { object, rotation: -0.7 },
        EffectivePropertyWrite::Scale { object, scale: Vec2::new(1.2, 1.2) },
        EffectivePropertyWrite::Scale { object, scale: Vec2::new(0.5, 2.0) },
        EffectivePropertyWrite::Fill { object, fill: Some(Color::YELLOW) },
        EffectivePropertyWrite::Fill { object, fill: None },
        EffectivePropertyWrite::Stroke { object, stroke: Some(Color::GREEN) },
        EffectivePropertyWrite::StrokeWidth { object, stroke_width: 8.0 },
        EffectivePropertyWrite::Opacity { object, opacity: 0.4 },
    ];
    let compiled = scene(1, &[]);
    for a in pool { for b in pool { for c in pool {
        let writes = [a, b, c];
        let mut expected_transform = Transform2D::IDENTITY;
        let mut expected_style = base_style();
        for write in writes { assign_expected(&mut expected_transform, &mut expected_style, write); }
        let mut instance = SceneInstance::new(compiled.clone());
        commit(&mut instance, 0.0, &writes);
        let row = instance.effective_object(object).unwrap();
        assert_eq!(row.transform, expected_transform, "{writes:?}");
        assert_eq!(row.style, expected_style, "{writes:?}");
    } } }
}

#[test]
fn all_invalid_components_are_rejected_even_when_later_superseded() {
    let object = object();
    let invalid = [
        EffectivePropertyWrite::Translation { object, translation: Vec2::new(f32::NAN, 0.0) },
        EffectivePropertyWrite::Rotation { object, rotation: f32::INFINITY },
        EffectivePropertyWrite::Scale { object, scale: Vec2::new(1.0, f32::NEG_INFINITY) },
        EffectivePropertyWrite::Fill { object, fill: Some(Color { alpha: f32::NAN, ..Color::RED }) },
        EffectivePropertyWrite::Stroke { object, stroke: Some(Color { red: f32::INFINITY, ..Color::RED }) },
        EffectivePropertyWrite::StrokeWidth { object, stroke_width: f32::NAN },
        EffectivePropertyWrite::Opacity { object, opacity: f32::NAN },
    ];
    for write in invalid {
        let mut instance = SceneInstance::new(scene(1, &[]));
        instance.take_frame_changes();
        let before = instance.frame().clone();
        let publication = instance.publication_context();
        assert!(instance.prepare_effective_property_batch(&[
            write,
            EffectivePropertyWrite::Transform { object, transform: Transform2D::IDENTITY },
            EffectivePropertyWrite::Style { object, style: base_style() },
        ]).is_err(), "{write:?}");
        assert_eq!(instance.frame(), &before);
        assert_eq!(instance.publication_context(), publication);
        assert!(instance.take_frame_changes().is_empty());
    }
}

#[test]
fn finite_style_components_keep_existing_unclamped_runtime_semantics() {
    let mut instance = SceneInstance::new(scene(1, &[]));
    let object = object();
    let style = Style { opacity: 1.1, stroke_width: -1.0, ..base_style() };
    let mut whole = instance.clone();
    commit(&mut whole, 0.0, &[EffectivePropertyWrite::Style { object, style }]);
    commit(&mut instance, 0.0, &[
        EffectivePropertyWrite::Opacity { object, opacity: style.opacity },
        EffectivePropertyWrite::StrokeWidth { object, stroke_width: style.stroke_width },
    ]);
    assert_eq!(instance.frame().objects[0].style, whole.frame().objects[0].style);
}

#[test]
fn authored_transaction_and_narrow_carry_forward_merge_at_commit() {
    let mut instance = SceneInstance::new(scene(1, &[]));
    let expected = instance.publication_context();
    let batch = instance.prepare_effective_property_batch(&[
        EffectivePropertyWrite::Scale { object: object(), scale: Vec2::new(1.2, 1.2) },
        EffectivePropertyWrite::Fill { object: object(), fill: Some(Color::YELLOW) },
    ]).unwrap();
    let transform = Transform2D { translation: Vec2::new(2.0, 3.0), rotation: 0.7, ..Transform2D::IDENTITY };
    let style = Style { stroke: Some(Color::RED), opacity: 0.3, ..base_style() };
    let transaction = ExecutionMutationTransaction::from_mutations([
        ExecutionPatch::SetTransform { object: object(), transform },
        ExecutionPatch::SetStyle { object: object(), style },
    ]);
    instance.apply_authored_execution_transaction_with_effective(
        &transaction, CompiledResources::default(), Some(batch), expected,
        expected.scene_revision().checked_next().unwrap(),
    ).unwrap();
    let row = instance.effective_object(object()).unwrap();
    assert_eq!(row.transform, Transform2D { scale: Vec2::new(1.2, 1.2), ..transform });
    assert_eq!(row.style, Style { fill: Some(Color::YELLOW), ..style });
    assert_eq!(instance.compiled.objects()[0].base_transform, transform);
    assert_eq!(instance.compiled.objects()[0].base_style, style);
}

#[test]
fn same_time_narrow_write_touches_one_row_among_ten_thousand() {
    let mut instance = SceneInstance::new(scene(10_000, &[]));
    instance.take_frame_changes();
    let before = instance.publication_context();
    let phase = instance.prepare_advance_to(0.0).unwrap();
    assert_eq!(phase.staged_row_count(), 0);
    assert_eq!(phase.evaluation_stats().groups_evaluated, 0);
    let batch = instance.prepare_effective_property_batch(&[
        EffectivePropertyWrite::Fill { object: ObjectId::new(5001), fill: Some(Color::YELLOW) },
    ]).unwrap();
    assert_eq!(batch.len(), 1);
    instance.commit_prepared_frame(phase, batch).unwrap();
    assert_eq!(instance.take_frame_changes().object_indices(), &[5000]);
    assert_eq!(instance.frame().time, 0.0);
    assert_eq!(instance.publication_context().scene_revision(), before.scene_revision());
    assert_eq!(instance.publication_context().execution_revision(), before.execution_revision());
}

#[test]
fn same_value_component_does_not_publish_another_frame_epoch() {
    let mut instance = SceneInstance::new(scene(1, &[]));
    instance.take_frame_changes();
    let before = instance.publication_context();
    commit(&mut instance, 0.0, &[
        EffectivePropertyWrite::Scale { object: object(), scale: Vec2::ONE },
        EffectivePropertyWrite::Fill { object: object(), fill: base_style().fill },
    ]);
    assert_eq!(instance.publication_context(), before);
    assert!(instance.take_frame_changes().is_empty());
}

#[test]
fn new_components_do_not_bypass_unknown_or_retired_identity_validation() {
    let mut instance = SceneInstance::new(scene(1, &[]));
    assert!(instance.prepare_effective_property_batch(&[
        EffectivePropertyWrite::Scale { object: ObjectId::new(2), scale: Vec2::ONE },
    ]).is_err());
    instance.apply_execution_patch(&ExecutionPatch::RemoveObject(object())).unwrap();
    assert!(instance.prepare_effective_property_batch(&[
        EffectivePropertyWrite::Fill { object: object(), fill: Some(Color::YELLOW) },
    ]).is_err());
}

#[test]
fn stale_or_foreign_component_batch_cannot_overwrite_a_new_frame() {
    let mut instance = SceneInstance::new(scene(1, &[]));
    let mut foreign = instance.clone();
    let batch = instance.prepare_effective_property_batch(&[
        EffectivePropertyWrite::Scale { object: object(), scale: Vec2::new(1.2, 1.2) },
    ]).unwrap();
    let foreign_phase = foreign.prepare_advance_to(0.0).unwrap();
    assert!(matches!(foreign.commit_prepared_frame(foreign_phase, batch.clone()), Err(PreparedFrameCommitError::ForeignRuntime { .. })));
    commit(&mut instance, 0.0, &[
        EffectivePropertyWrite::Rotation { object: object(), rotation: 0.5 },
    ]);
    let frame = instance.frame().clone();
    let phase = instance.prepare_advance_to(0.0).unwrap();
    assert!(matches!(instance.commit_prepared_frame(phase, batch), Err(PreparedFrameCommitError::StalePublication { .. })));
    assert_eq!(instance.frame(), &frame);
}

#[test]
fn scoped_write_vocabulary_does_not_unseal_replay() {
    let mut instance = SceneInstance::new(scene(1, &[]));
    instance.begin_replay_retention(ReplayLimits::default()).unwrap();
    instance.seal_replay().unwrap();
    assert!(matches!(instance.prepare_advance_to(0.0), Err(EvaluationError::ReplaySealed)));
    assert!(instance.replay_is_sealed());
}
