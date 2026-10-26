use std::sync::Arc;

use noon_compile::{CompiledObject, CompiledScene, ExecutionMutationTransaction, ExecutionPatch};
use noon_core::{Color, GeometryRef, GradientStop, LinearGradient, ObjectId, Style, Transform2D};

use crate::{FrameChanges, SceneInstance};

fn gradient(count: usize) -> Arc<LinearGradient> {
    Arc::new(
        LinearGradient::new(
            [0.0, 0.5],
            [1.0, 0.5],
            (0..count).map(|i| {
                GradientStop::new(
                    i as f64 / (count - 1) as f64,
                    if i == 0 {
                        Color::rgb(1.0, 0.0, 0.0)
                    } else {
                        Color::rgb(0.0, 0.0, 1.0)
                    },
                )
                .unwrap()
            }),
        )
        .unwrap(),
    )
}

fn runtime(count: usize) -> SceneInstance {
    SceneInstance::new(
        CompiledScene::compile_objects(
            (0..count)
                .map(|index| {
                    CompiledObject::new(
                        ObjectId::new(index as u64),
                        GeometryRef::rectangle(2.0, 1.0),
                        Transform2D::IDENTITY,
                        Style::default(),
                    )
                })
                .collect(),
            &[],
        )
        .unwrap(),
    )
}

fn transaction(index: usize, paint: Option<Arc<LinearGradient>>) -> ExecutionMutationTransaction {
    ExecutionMutationTransaction::from_mutations([ExecutionPatch::SetStyle {
        object: ObjectId::new(index as u64),
        style: Style::default(),
        fill_gradient: paint,
    }])
}

#[test]
fn gradient_only_prepared_publication_is_atomic_and_local_in_100k_scene() {
    let mut runtime = runtime(100_000);
    runtime.take_frame_changes();
    runtime.take_spatial_changes();
    let paint = gradient(2);
    let edits = transaction(50_000, Some(paint.clone()));
    let before = runtime.publication_context();
    let next_scene = before.scene_revision().checked_next().unwrap();
    let prepared = runtime
        .prepare_authored_value_publication(&edits, before, next_scene)
        .unwrap()
        .unwrap();
    assert!(runtime.frame().objects[50_000].fill_gradient.is_none());
    assert_eq!(runtime.publication_context(), before);
    drop(prepared);
    assert!(runtime.frame_changes().is_empty());
    let prepared = runtime
        .prepare_authored_value_publication(&edits, before, next_scene)
        .unwrap()
        .unwrap();
    runtime.commit_prepared_authored_value_publication(prepared);
    assert!(Arc::ptr_eq(
        runtime.frame().objects[50_000]
            .fill_gradient
            .as_ref()
            .unwrap(),
        &paint
    ));
    assert!(runtime.frame().objects[49_999].fill_gradient.is_none());
    assert!(runtime.frame().objects[50_001].fill_gradient.is_none());
    assert_eq!(
        runtime.take_frame_changes(),
        FrameChanges::objects(vec![50_000])
    );
    assert!(runtime.take_spatial_changes().is_empty());
    assert_eq!(runtime.publication_context().scene_revision(), next_scene);
    assert_ne!(
        runtime.publication_context().frame_epoch(),
        before.frame_epoch()
    );
    let context = runtime.publication_context();
    let prepared = runtime
        .prepare_authored_value_publication(
            &edits,
            context,
            context.scene_revision().checked_next().unwrap(),
        )
        .unwrap()
        .unwrap();
    runtime.commit_prepared_authored_value_publication(prepared);
    assert!(
        runtime.take_frame_changes().is_empty(),
        "equal paint and style must stay pixel-idempotent"
    );
    runtime
        .apply_execution_transaction(&transaction(50_000, None))
        .unwrap();
    assert!(runtime.frame().objects[50_000].fill_gradient.is_none());
    assert_eq!(
        runtime.take_frame_changes(),
        FrameChanges::objects(vec![50_000])
    );
}

#[test]
fn invalid_gradient_in_late_patch_preserves_all_earlier_values() {
    let mut runtime = runtime(2);
    runtime.take_frame_changes();
    let before = runtime.frame().clone();
    let context = runtime.publication_context();
    let edits = ExecutionMutationTransaction::from_mutations([
        ExecutionPatch::SetStyle {
            object: ObjectId::new(0),
            style: Style::default(),
            fill_gradient: Some(gradient(2)),
        },
        ExecutionPatch::SetStyle {
            object: ObjectId::new(1),
            style: Style::default(),
            fill_gradient: Some(gradient(17)),
        },
    ]);
    assert!(runtime
        .prepare_authored_value_publication(
            &edits,
            context,
            context.scene_revision().checked_next().unwrap()
        )
        .is_err());
    assert!(runtime.apply_execution_transaction(&edits).is_err());
    assert_eq!(runtime.frame(), &before);
    assert_eq!(runtime.publication_context(), context);
    assert!(runtime.take_frame_changes().is_empty());
}

#[test]
fn gradient_base_survives_completed_solid_history_and_out_of_order_seek() {
    use noon_core::{Property, RateFunction, TrackDefinition, TrackId, TrackTiming, TrackValues};
    let mut runtime = runtime(1);
    runtime
        .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
            id: TrackId::new(1),
            object: ObjectId::new(0),
            property: Property::Fill,
            values: TrackValues::Color {
                from: Some(Color::RED),
                to: Some(Color::BLUE),
            },
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            time_map: Default::default(),
        }))
        .unwrap();
    runtime.advance_to(1.0).unwrap();
    runtime
        .apply_execution_patch(&ExecutionPatch::ReconcileTrack {
            track: TrackId::new(1),
            object: ObjectId::new(0),
            property: Property::Fill,
            end_time: 1.0,
        })
        .unwrap();
    let paint = gradient(2);
    let context = runtime.publication_context();
    let prepared = runtime
        .prepare_authored_value_publication(
            &transaction(0, Some(paint.clone())),
            context,
            context.scene_revision().checked_next().unwrap(),
        )
        .unwrap()
        .unwrap();
    runtime.commit_prepared_authored_value_publication(prepared);
    assert_eq!(
        runtime.frame().objects[0].fill_gradient.as_ref(),
        Some(&paint)
    );
    for time in [0.5, 1.5, 0.25, 1.0, 2.0, 0.0, 1.5] {
        let object = &runtime.seek(time).unwrap().objects[0];
        assert_eq!(
            object.fill_gradient.as_ref(),
            (time >= 1.0).then_some(&paint),
            "time={time}"
        );
    }
}
