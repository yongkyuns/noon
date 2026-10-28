use std::sync::Arc;

use noon_compile::{CompiledObject, CompiledScene, ExecutionMutationTransaction, ExecutionPatch};
use noon_core::{Color, GeometryRef, GradientStop, LinearGradient, ObjectId, Style, Transform2D};

use crate::{FrameChanges, SceneInstance};

fn gradient(count: usize) -> Arc<noon_core::SemanticPaint> {
    Arc::new(noon_core::SemanticPaint::LinearGradient(Arc::new(
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
    )))
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

fn transaction(
    index: usize,
    paint: Option<Arc<noon_core::SemanticPaint>>,
) -> ExecutionMutationTransaction {
    ExecutionMutationTransaction::from_mutations([ExecutionPatch::SetStyle {
        object: ObjectId::new(index as u64),
        style: Style::default(),
        fill_paint: paint,
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
        .prepare_authored_value_publication(&edits, Default::default(), before, next_scene)
        .unwrap()
        .unwrap();
    assert!(runtime.frame().objects[50_000].fill_paint.is_none());
    assert_eq!(runtime.publication_context(), before);
    drop(prepared);
    assert!(runtime.frame_changes().is_empty());
    let prepared = runtime
        .prepare_authored_value_publication(&edits, Default::default(), before, next_scene)
        .unwrap()
        .unwrap();
    runtime.commit_prepared_authored_value_publication(prepared);
    assert!(Arc::ptr_eq(
        runtime.frame().objects[50_000].fill_paint.as_ref().unwrap(),
        &paint
    ));
    assert!(runtime.frame().objects[49_999].fill_paint.is_none());
    assert!(runtime.frame().objects[50_001].fill_paint.is_none());
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
            Default::default(),
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
    assert!(runtime.frame().objects[50_000].fill_paint.is_none());
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
            fill_paint: Some(gradient(2)),
        },
        ExecutionPatch::SetStyle {
            object: ObjectId::new(1),
            style: Style::default(),
            fill_paint: Some(gradient(17)),
        },
    ]);
    assert!(runtime
        .prepare_authored_value_publication(
            &edits,
            Default::default(),
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
            Default::default(),
            context,
            context.scene_revision().checked_next().unwrap(),
        )
        .unwrap()
        .unwrap();
    runtime.commit_prepared_authored_value_publication(prepared);
    assert_eq!(runtime.frame().objects[0].fill_paint.as_ref(), Some(&paint));
    for time in [0.5, 1.5, 0.25, 1.0, 2.0, 0.0, 1.5] {
        let object = &runtime.seek(time).unwrap().objects[0];
        assert_eq!(
            object.fill_paint.as_ref(),
            (time >= 1.0).then_some(&paint),
            "time={time}"
        );
    }
}

fn paint_track(object: u64, start: f64) -> noon_core::TrackDefinition {
    use noon_core::{
        PaintTransition, Property, RateFunction, SemanticPaint, TrackDefinition, TrackId,
        TrackTiming, TrackValues,
    };
    TrackDefinition {
        id: TrackId::new(7),
        object: ObjectId::new(object),
        property: Property::Fill,
        values: TrackValues::Paint {
            transition: Arc::new(
                PaintTransition::new(
                    Some(SemanticPaint::Solid(Color::rgb(1.0, 0.0, 0.0))),
                    1.0,
                    Some(gradient(2).as_ref().clone()),
                    0.5,
                )
                .unwrap(),
            ),
        },
        timing: TrackTiming::new(start, 1.0, RateFunction::Linear),
        time_map: Default::default(),
    }
}

#[test]
fn paint_animation_touches_one_row_in_100k_scene_and_has_no_spatial_work() {
    let mut runtime = runtime(100_000);
    runtime
        .apply_execution_patch(&ExecutionPatch::AddTrack(paint_track(50_000, 0.0)))
        .unwrap();
    runtime.take_frame_changes();
    runtime.take_spatial_changes();
    runtime.advance_to(0.5).unwrap();
    let row = &runtime.frame().objects[50_000];
    assert_eq!(row.style.fill.unwrap().alpha, 0.75);
    assert_eq!(
        row.fill_paint
            .as_ref()
            .unwrap()
            .as_linear_gradient()
            .unwrap()
            .sample_premultiplied([0.25, 0.5])
            .unwrap(),
        [0.875, 0.0, 0.125, 1.0]
    );
    assert_eq!(runtime.last_stats().groups_evaluated, 1);
    assert_eq!(
        runtime.take_frame_changes(),
        FrameChanges::objects(vec![50_000])
    );
    assert!(runtime.take_spatial_changes().is_empty());
    assert!(runtime.frame().objects[49_999].fill_paint.is_none());
    assert!(runtime.frame().objects[50_001].fill_paint.is_none());
    runtime.advance_to(0.5).unwrap();
    assert!(runtime.take_frame_changes().is_empty());
}

#[test]
fn paint_seek_matches_forward_and_recovers_source_before_reconciled_delayed_track() {
    use noon_core::{Property, TrackId};
    let mut runtime = runtime(1);
    runtime
        .apply_execution_patch(&ExecutionPatch::SetStyle {
            object: ObjectId::new(0),
            style: Style {
                fill: Some(Color::rgb(1.0, 0.0, 0.0)),
                ..Style::default()
            },
            fill_paint: None,
        })
        .unwrap();
    runtime
        .apply_execution_patch(&ExecutionPatch::AddTrack(paint_track(0, 2.0)))
        .unwrap();
    runtime.advance_to(2.5).unwrap();
    let middle = runtime.frame().objects[0].clone();
    runtime.advance_to(3.0).unwrap();
    let target = runtime.frame().objects[0].clone();
    runtime
        .apply_execution_patch(&ExecutionPatch::ReconcileTrack {
            track: TrackId::new(7),
            object: ObjectId::new(0),
            property: Property::Fill,
            end_time: 3.0,
        })
        .unwrap();
    runtime
        .apply_execution_patch(&ExecutionPatch::SetStyle {
            object: ObjectId::new(0),
            style: target.style,
            fill_paint: target.fill_paint.clone(),
        })
        .unwrap();
    for time in [2.5, 0.5, 3.0, 2.25, 1.0, 2.5, 4.0] {
        let row = &runtime.seek(time).unwrap().objects[0];
        if time < 2.0 {
            assert_eq!(row.style.fill, Some(Color::rgb(1.0, 0.0, 0.0)));
            assert!(row.fill_paint.is_none());
        } else if time == 2.5 {
            assert_eq!(row.style, middle.style);
            assert_eq!(row.fill_paint, middle.fill_paint);
        } else if time >= 3.0 {
            assert_eq!(row.style, target.style);
            assert_eq!(row.fill_paint, target.fill_paint);
        }
    }
}

#[test]
fn unsupported_source_changes_reject_in_both_transaction_orders_and_direct_edits() {
    let mut runtime = runtime(1);
    let line = ExecutionPatch::SetContent {
        object: ObjectId::new(0),
        content: GeometryRef::line(noon_core::Vec2::ZERO, noon_core::Vec2::ONE).into(),
        text_bounds: None,
    };
    let track = ExecutionPatch::AddTrack(paint_track(0, 0.0));
    let before = runtime.frame().clone();
    for mutations in [
        vec![track.clone(), line.clone()],
        vec![line.clone(), track.clone()],
    ] {
        assert!(runtime
            .apply_execution_transaction(&ExecutionMutationTransaction::from_mutations(mutations))
            .is_err());
        assert_eq!(runtime.frame(), &before);
    }
    runtime.apply_execution_patch(&track).unwrap();
    let before = runtime.frame().clone();
    assert!(runtime.apply_execution_patch(&line).is_err());
    assert_eq!(runtime.frame(), &before);
    runtime
        .apply_execution_patch(&ExecutionPatch::RemoveTrack(noon_core::TrackId::new(7)))
        .unwrap();
    runtime.apply_execution_patch(&line).unwrap();
}

#[test]
fn paint_and_glow_transactions_accept_both_orders_without_spatial_work() {
    let glow = Arc::new(noon_compile::CompiledEffect {
        attachment: noon_core::SemanticNodeId::new(7, 0),
        definition: (noon_core::Glow::new(noon_core::EffectUpdate::default().intensity(0.4))
            .unwrap())
        .into(),
    });
    let attach = ExecutionPatch::SetEffectStack {
        object: ObjectId::new(0),
        expected: Box::new([]),
        effects: Some(Arc::new(
            noon_compile::CompiledEffectStack::new([*glow]).unwrap(),
        )),
    };
    let style = ExecutionPatch::SetStyle {
        object: ObjectId::new(0),
        style: Style::default(),
        fill_paint: Some(gradient(2)),
    };
    let track = ExecutionPatch::AddTrack(paint_track(0, 0.0));
    for paint in [style, track] {
        for glow_first in [false, true] {
            let mut runtime = runtime(2);
            runtime.take_frame_changes();
            runtime.take_spatial_changes();
            let mutations = if glow_first {
                vec![attach.clone(), paint.clone()]
            } else {
                vec![paint.clone(), attach.clone()]
            };
            runtime
                .apply_execution_transaction(&ExecutionMutationTransaction::from_mutations(
                    mutations,
                ))
                .unwrap();
            let changes = runtime.take_frame_changes();
            assert_eq!(changes.object_indices(), &[0]);
            let spatial = runtime.take_spatial_changes();
            assert!(
                spatial.is_empty(),
                "paint={paint:?} glow_first={glow_first}: {spatial:?}"
            );
            runtime.seek(0.5).unwrap();
            assert_eq!(
                runtime.frame().objects[0]
                    .effects
                    .as_ref()
                    .and_then(|stack| stack.get(glow.attachment)),
                Some(*glow)
            );
            assert!(runtime.frame().objects[0].fill_paint.is_some());
            // Content remains independently checked; supported geometry can be
            // replaced without discarding either appearance input.
            runtime
                .apply_execution_patch(&ExecutionPatch::SetContent {
                    object: ObjectId::new(0),
                    content: GeometryRef::circle(0.5).into(),
                    text_bounds: None,
                })
                .unwrap();
            assert!(runtime.frame().objects[0].fill_paint.is_some());
            assert_eq!(
                runtime.frame().objects[0]
                    .effects
                    .as_ref()
                    .and_then(|stack| stack.get(glow.attachment)),
                Some(*glow)
            );
            runtime.take_spatial_changes();
            let mut style = runtime.frame().objects[0].style;
            style.stroke = Some(Color::WHITE);
            style.stroke_width = 0.25;
            runtime
                .apply_execution_patch(&ExecutionPatch::SetStyle {
                    object: ObjectId::new(0),
                    style,
                    fill_paint: runtime.frame().objects[0].fill_paint.clone(),
                })
                .unwrap();
            assert_eq!(
                runtime.take_spatial_changes().object_indices(),
                &[0],
                "stroke bounds must still invalidate spatial state"
            );
        }
    }
}

#[test]
fn bounded_rates_cannot_panic_paint_sampling_and_overshoot_rejects_atomically() {
    use noon_core::RateFunction;
    let mut runtime = runtime(1);
    let before = runtime.frame().clone();
    let mut track = paint_track(0, 0.0);
    track.timing.easing = RateFunction::EaseOutBack;
    assert!(runtime
        .apply_execution_patch(&ExecutionPatch::AddTrack(track.clone()))
        .is_err());
    assert_eq!(runtime.frame(), &before);
    for rate in [
        RateFunction::Smoothererstep,
        RateFunction::EaseOutBounce,
        RateFunction::ThereAndBack,
    ] {
        track.timing.easing = rate;
        runtime
            .apply_execution_patch(&ExecutionPatch::AddTrack(track.clone()))
            .unwrap();
        runtime.seek(0.0).unwrap();
        for step in 1..=1000 {
            runtime.advance_to(f64::from(step) / 1000.0).unwrap();
        }
        runtime
            .apply_execution_patch(&ExecutionPatch::RemoveTrack(track.id))
            .unwrap();
    }
}

#[test]
fn prepared_image_value_publication_owns_pixels_but_installs_them_only_at_commit() {
    use noon_compile::CompiledResources;
    use noon_core::{
        ImageMapping, ImagePaint, RasterImageResourceArena, RasterImageResourceLookup,
        RasterImageSampling, SemanticPaint,
    };
    let mut arena = RasterImageResourceArena::new();
    let handle = arena.intern_rgba8(1, 1, [17, 73, 191, 128]).unwrap();
    let paint = SemanticPaint::Image(Arc::new(ImagePaint::new(
        handle,
        ImageMapping::default(),
        RasterImageSampling::Linear,
    )));
    let mut additions = CompiledResources::default();
    additions.capture_paint_from_arena(&arena, &paint).unwrap();
    let edits = transaction(0, Some(Arc::new(paint)));
    let mut runtime = runtime(2);
    runtime.take_frame_changes();
    runtime.take_spatial_changes();
    let before = runtime.frame().clone();
    let context = runtime.publication_context();
    let next = context.scene_revision().checked_next().unwrap();
    assert!(runtime
        .prepare_authored_value_publication(&edits, CompiledResources::default(), context, next)
        .is_err());
    assert_eq!(runtime.frame(), &before);
    assert_eq!(runtime.publication_context(), context);
    let prepared = runtime
        .prepare_authored_value_publication(&edits, additions.clone(), context, next)
        .unwrap()
        .unwrap();
    assert!(runtime.raster_image_resources().get(handle).is_none());
    drop(prepared);
    assert_eq!(runtime.frame(), &before);
    assert!(runtime.take_frame_changes().is_empty());
    let prepared = runtime
        .prepare_authored_value_publication(&edits, additions, context, next)
        .unwrap()
        .unwrap();
    drop(arena);
    runtime.commit_prepared_authored_value_publication(prepared);
    assert_eq!(
        runtime
            .raster_image_resources()
            .get(handle)
            .unwrap()
            .rgba8(),
        &[17, 73, 191, 128]
    );
    assert_eq!(runtime.take_frame_changes(), FrameChanges::objects(vec![0]));
    assert!(runtime.take_spatial_changes().is_empty());
    assert!(runtime.frame().objects[1].fill_paint.is_none());
}
