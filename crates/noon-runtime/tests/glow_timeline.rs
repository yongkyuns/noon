//! Ordinary timeline channel tests; public Scene orchestration is still guarded.
use noon_compile::{
    CompiledGlow, CompiledObject, CompiledScene, ExecutionMutationTransaction, ExecutionPatch,
};
use noon_core::{
    Color, CompositionTimeMap, CompositionTimeMapStep, GeometryRef, Glow, GlowRadius,
    GlowTrackValue, GlowUpdate, ObjectId, Pixels, Property, RateFunction, SemanticNodeId, Style,
    TrackDefinition, TrackId, TrackTiming, TrackValues, Transform2D, Vec2,
};
use noon_runtime::SceneInstance;
use std::sync::Arc;

fn object() -> CompiledObject {
    let mut object = CompiledObject::new(
        ObjectId::new(1),
        GeometryRef::circle(0.4),
        Transform2D::IDENTITY,
        Style::default(),
    );
    object.glow = Some(Arc::new(CompiledGlow {
        attachment: SemanticNodeId::new(3, 7),
        definition: Glow::new(
            GlowUpdate::default()
                .radius(Pixels(3.25))
                .intensity(0.35)
                .color(Color::RED),
        )
        .unwrap(),
    }));
    object
}
fn tracks(
    object: &CompiledObject,
    update: GlowUpdate,
    start: f64,
    easing: RateFunction,
) -> Vec<TrackDefinition> {
    object
        .glow
        .as_ref()
        .unwrap()
        .parameter_channels(update)
        .unwrap()
        .into_iter()
        .enumerate()
        .map(|(index, (property, values))| TrackDefinition {
            id: TrackId::new(index as u64 + 1),
            object: object.id,
            property,
            values,
            timing: TrackTiming::new(start, 1.0, easing),
            time_map: CompositionTimeMap::identity(),
        })
        .collect()
}
fn value(runtime: &SceneInstance) -> Glow {
    runtime.frame().objects[0].glow.as_ref().unwrap().definition
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-12, "{a} != {b}");
}

#[test]
fn independent_glow_channels_share_motion_timing_and_rewind() {
    let object = object();
    let update = GlowUpdate::default()
        .color(Color::BLUE)
        .radius(Pixels(5.0))
        .intensity(1.2);
    let mut channels = tracks(&object, update, 0.2, RateFunction::Linear);
    channels.push(TrackDefinition {
        id: TrackId::new(10),
        object: object.id,
        property: Property::Position,
        values: TrackValues::Vec2 {
            from: Vec2::ZERO,
            to: Vec2::new(2.0, 1.0),
        },
        timing: TrackTiming::new(0.2, 1.0, RateFunction::Linear),
        time_map: Default::default(),
    });
    let compiled = CompiledScene::compile_objects(vec![object.clone()], &channels).unwrap();
    let mut runtime = SceneInstance::new(compiled.clone());
    for time in [0.0, 0.2, 0.57, 1.2, 2.0, 0.0, 0.57] {
        runtime.evaluate(time).unwrap();
        let alpha = ((time - 0.2) / 1.0).clamp(0.0, 1.0);
        let expected = update
            .prepare(object.glow.as_ref().unwrap().definition)
            .unwrap()
            .sample(alpha)
            .unwrap()
            .apply_to(object.glow.as_ref().unwrap().definition)
            .unwrap();
        assert_eq!(value(&runtime), expected);
        let mut direct = SceneInstance::new(compiled.clone());
        direct.seek(time).unwrap();
        assert_eq!(runtime.frame(), direct.frame());
        assert_eq!(
            runtime.frame().objects[0].glow.as_ref().unwrap().attachment,
            SemanticNodeId::new(3, 7)
        );
    }
    assert_eq!(
        object.glow.as_ref().unwrap().definition.intensity(),
        0.35,
        "authored source snapshot stays immutable"
    );
}

#[test]
fn unchanged_frames_are_clean_and_constant_radius_keeps_f64_precision() {
    let mut object = object();
    let radius = GlowRadius::Pixels(12.1234567890123);
    Arc::make_mut(object.glow.as_mut().unwrap()).definition = GlowUpdate::default()
        .radius(radius)
        .apply_to(object.glow.as_ref().unwrap().definition)
        .unwrap();
    let channels = tracks(
        &object,
        GlowUpdate::default().intensity(1.2),
        0.0,
        RateFunction::Linear,
    );
    let mut runtime =
        SceneInstance::new(CompiledScene::compile_objects(vec![object], &channels).unwrap());
    runtime.advance_to(0.4).unwrap();
    assert_eq!(value(&runtime).radius(), radius);
    runtime.take_frame_changes();
    let retained = runtime.frame().objects[0].glow.clone().unwrap();
    runtime.advance_to(0.4).unwrap();
    assert!(runtime.frame_changes().is_empty());
    // The existing scheduler samples an active channel again at equal time,
    // but an equal value must not clone/write its retained definition.
    assert!(Arc::ptr_eq(
        &retained,
        runtime.frame().objects[0].glow.as_ref().unwrap()
    ));
    runtime.advance_to(2.0).unwrap();
    runtime.take_frame_changes();
    runtime.advance_to(3.0).unwrap();
    assert!(runtime.frame_changes().is_empty());
    assert_eq!(runtime.last_stats().groups_evaluated, 0);
}

#[test]
fn prepared_publication_is_atomic_and_matches_direct_evaluation() {
    let object = object();
    let channels = tracks(
        &object,
        GlowUpdate::default().radius(Pixels(6.0)).intensity(2.4),
        0.0,
        RateFunction::Smooth,
    );
    let compiled = CompiledScene::compile_objects(vec![object], &channels).unwrap();
    let mut staged = SceneInstance::new(compiled.clone());
    let before = staged.frame().clone();
    let prepared = staged.prepare_advance_to(0.4).unwrap();
    assert_eq!(prepared.staged_row_count(), 1);
    assert_eq!(
        staged.frame(),
        &before,
        "preparation does not publish effective values"
    );
    drop(prepared);
    staged.advance_to_with_reactive_inputs(0.4, &[]).unwrap();
    let mut direct = SceneInstance::new(compiled);
    direct.advance_to(0.4).unwrap();
    assert_eq!(staged.frame(), direct.frame());
    let publication = staged.take_renderer_publication();
    assert!(!publication.changes().is_empty());
    assert_eq!(
        publication.frame().objects[0]
            .glow
            .as_ref()
            .unwrap()
            .definition,
        value(&direct)
    );
}

#[test]
fn removing_one_channel_restores_only_its_own_base_value() {
    let object = object();
    let channels = tracks(
        &object,
        GlowUpdate::default().radius(Pixels(6.0)).intensity(2.4),
        0.0,
        RateFunction::Linear,
    );
    let intensity = channels
        .iter()
        .find(|track| track.property == Property::GlowIntensity)
        .unwrap()
        .id;
    let mut runtime =
        SceneInstance::new(CompiledScene::compile_objects(vec![object], &channels).unwrap());
    runtime.advance_to(0.4).unwrap();
    let radius = value(&runtime).radius();
    runtime
        .apply_execution_patch(&ExecutionPatch::RemoveTrack(intensity))
        .unwrap();
    close(value(&runtime).intensity(), 0.35);
    assert_eq!(value(&runtime).radius(), radius);
    runtime.advance_to(0.8).unwrap();
    close(value(&runtime).intensity(), 0.35);
    assert!(matches!(value(&runtime).radius(), GlowRadius::Pixels(_)));
    close(value(&runtime).radius().value(), 5.45);
}

#[test]
fn wrong_generation_or_units_reject_before_transaction_prefix_commits() {
    let object = object();
    let mut track = tracks(
        &object,
        GlowUpdate::default().intensity(1.2),
        0.0,
        RateFunction::Linear,
    )
    .remove(0);
    let TrackValues::Glow { attachment, .. } = &mut track.values else {
        panic!()
    };
    *attachment = SemanticNodeId::new(3, 6);
    assert!(CompiledScene::compile_objects(vec![object.clone()], &[track.clone()]).is_err());
    let mut runtime =
        SceneInstance::new(CompiledScene::compile_objects(vec![object.clone()], &[]).unwrap());
    let before = runtime.frame().clone();
    let context = runtime.publication_context();
    let tx = ExecutionMutationTransaction::from_mutations([
        ExecutionPatch::SetTransform {
            object: object.id,
            transform: Transform2D {
                translation: Vec2::new(9.0, 2.0),
                ..Transform2D::IDENTITY
            },
        },
        ExecutionPatch::AddTrack(track),
    ]);
    assert!(runtime.apply_execution_transaction(&tx).is_err());
    assert_eq!(runtime.frame(), &before);
    assert_eq!(runtime.publication_context(), context);
    let invalid = TrackDefinition {
        id: TrackId::new(2),
        object: object.id,
        property: Property::GlowRadius,
        values: TrackValues::Glow {
            attachment: object.glow.as_ref().unwrap().attachment,
            from: GlowTrackValue::Radius(GlowRadius::Scene(1.0)),
            to: GlowTrackValue::Radius(GlowRadius::Scene(2.0)),
        },
        timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        time_map: Default::default(),
    };
    assert!(
        runtime
            .apply_execution_patch(&ExecutionPatch::AddTrack(invalid))
            .is_err(),
        "consistent track units still must match attachment units"
    );
}

#[test]
fn incompatible_parameter_values_and_discrete_updates_fail_admission() {
    let object = object();
    let glow = object.glow.as_ref().unwrap();
    assert!(glow
        .parameter_channels(GlowUpdate::default().radius(1.0))
        .is_err());
    assert!(glow
        .parameter_channels(GlowUpdate::default().source(noon_core::GlowSource::Silhouette))
        .is_err());
    for values in [
        TrackValues::Glow {
            attachment: glow.attachment,
            from: GlowTrackValue::Intensity(0.5),
            to: GlowTrackValue::Intensity(f64::NAN),
        },
        TrackValues::Glow {
            attachment: glow.attachment,
            from: GlowTrackValue::Intensity(0.5),
            to: GlowTrackValue::Intensity(8.1),
        },
        TrackValues::Glow {
            attachment: glow.attachment,
            from: GlowTrackValue::Radius(GlowRadius::Pixels(3.0)),
            to: GlowTrackValue::Radius(GlowRadius::Pixels(4.0)),
        },
        TrackValues::Glow {
            attachment: glow.attachment,
            from: GlowTrackValue::Intensity(0.5),
            to: GlowTrackValue::Color(Color::BLUE),
        },
    ] {
        let track = TrackDefinition {
            id: TrackId::new(1),
            object: object.id,
            property: Property::GlowIntensity,
            values,
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            time_map: Default::default(),
        };
        assert!(noon_core::validate_track_definition(&track).is_err());
    }
    assert!(glow
        .parameter_channels(GlowUpdate::default())
        .unwrap()
        .is_empty());
    assert_eq!(
        glow.parameter_channels(GlowUpdate::default().intensity(0.35))
            .unwrap()
            .len(),
        1,
        "explicit unchanged writes still own their channel"
    );
}

#[test]
fn returning_and_nested_rates_use_the_existing_time_map() {
    let object = object();
    let mut channels = tracks(
        &object,
        GlowUpdate::default().intensity(2.0),
        0.0,
        RateFunction::Linear,
    );
    channels[0].time_map = CompositionTimeMap::from_steps(vec![
        CompositionTimeMapStep::new(0.0, 1.0, RateFunction::ThereAndBack),
        CompositionTimeMapStep::new(0.25, 0.5, RateFunction::Linear),
    ]);
    let compiled = CompiledScene::compile_objects(vec![object], &channels).unwrap();
    let mut runtime = SceneInstance::new(compiled.clone());
    for time in [0.0, 0.15, 0.25, 0.5, 0.75, 1.0] {
        runtime.advance_to(time).unwrap();
        let mut direct = SceneInstance::new(compiled.clone());
        direct.seek(time).unwrap();
        assert_eq!(
            value(&runtime),
            value(&direct),
            "mapped glow differs at {time}"
        );
    }
    // Composition roots settle their children to the leaf's own terminal rate,
    // exactly like existing continuous channels, even for a returning parent.
    close(value(&runtime).intensity(), 2.0);
}

#[test]
fn leaf_returning_and_reverse_rates_preserve_the_existing_terminal_contract() {
    for (rate, reverse) in [
        (RateFunction::ThereAndBack, false),
        (RateFunction::Linear, true),
    ] {
        let object = object();
        let mut channels = tracks(&object, GlowUpdate::default().intensity(2.0), 0.0, rate);
        channels[0].timing.reverse_rate_function = reverse;
        let compiled = CompiledScene::compile_objects(vec![object], &channels).unwrap();
        let mut runtime = SceneInstance::new(compiled.clone());
        for time in [0.0, 0.2, 0.5, 0.8, 1.0, 2.0, 0.2] {
            runtime.evaluate(time).unwrap();
            let alpha = channels[0]
                .timing
                .evaluate_progress_f64(time.clamp(0.0, 1.0));
            close(value(&runtime).intensity(), 0.35 + (2.0 - 0.35) * alpha);
            let mut direct = SceneInstance::new(compiled.clone());
            direct.seek(time).unwrap();
            assert_eq!(runtime.frame(), direct.frame());
        }
        runtime.evaluate(2.0).unwrap();
        close(value(&runtime).intensity(), 0.35);
    }
}

#[test]
fn public_endpoint_sampler_rejects_cross_parameter_pairs() {
    assert!(GlowTrackValue::Intensity(0.5)
        .sample(GlowTrackValue::Color(Color::BLUE), 0.5)
        .is_err());
    assert!(GlowTrackValue::Intensity(0.5)
        .sample(GlowTrackValue::Intensity(1.0), f64::NAN)
        .is_err());
    assert!(GlowTrackValue::Radius(GlowRadius::Pixels(2.0))
        .sample(GlowTrackValue::Radius(GlowRadius::Scene(3.0)), 0.5)
        .is_err());
}

#[test]
fn one_animated_glow_stages_only_its_row_among_4096_unrelated_objects() {
    let glow_object = object();
    let tracks = tracks(
        &glow_object,
        GlowUpdate::default().intensity(2.0),
        0.0,
        RateFunction::Linear,
    );
    let mut objects = vec![glow_object];
    for id in 2..=4097 {
        objects.push(CompiledObject::new(
            ObjectId::new(id),
            GeometryRef::circle(0.1),
            Transform2D::IDENTITY,
            Style::default(),
        ));
    }
    let mut runtime = SceneInstance::new(CompiledScene::compile_objects(objects, &tracks).unwrap());
    runtime.advance_to(0.25).unwrap();
    runtime.take_frame_changes();
    let prepared = runtime.prepare_advance_to(0.5).unwrap();
    assert_eq!(prepared.staged_row_count(), 1);
    drop(prepared);
    runtime.advance_to(0.5).unwrap();
    assert_eq!(runtime.last_stats().groups_evaluated, 1);
    assert_eq!(
        runtime.frame_changes(),
        &noon_runtime::FrameChanges::objects(vec![0])
    );
    assert_eq!(
        runtime.frame().objects[4096].transform,
        Transform2D::IDENTITY
    );
    assert!(runtime.frame().objects[4096].glow.is_none());
}

#[test]
fn sequential_channel_capture_uses_current_value_and_replays_historical_gaps() {
    let object = object();
    let first = tracks(
        &object,
        GlowUpdate::default().intensity(1.0),
        0.0,
        RateFunction::Linear,
    )
    .remove(0);
    let mut runtime = SceneInstance::new(
        CompiledScene::compile_objects(vec![object.clone()], std::slice::from_ref(&first)).unwrap(),
    );
    runtime.advance_to(1.0).unwrap();
    let captured = runtime.frame().objects[0].glow.clone().unwrap();
    let (property, values) = captured
        .parameter_channels(GlowUpdate::default().intensity(3.0))
        .unwrap()
        .remove(0);
    let second = TrackDefinition {
        id: TrackId::new(2),
        object: object.id,
        property,
        values,
        timing: TrackTiming::new(2.0, 1.0, RateFunction::Linear),
        time_map: Default::default(),
    };
    runtime
        .apply_execution_patch(&ExecutionPatch::AddTrack(second))
        .unwrap();
    for (time, expected) in [(1.5, 1.0), (2.5, 2.0), (3.0, 3.0), (1.5, 1.0), (0.0, 0.35)] {
        runtime.evaluate(time).unwrap();
        close(value(&runtime).intensity(), expected);
    }
    assert_eq!(
        captured.definition.intensity(),
        1.0,
        "captured endpoint must not be mutated by later frames"
    );
}

#[test]
fn raw_overlapping_glow_tracks_follow_the_ordinary_channel_order_not_a_new_lease_policy() {
    // Low-level timeline tracks permit overlap with canonical ordering. Authored
    // animation driver leases are a separate admission layer, not implemented here.
    let object = object();
    let attachment = object.glow.as_ref().unwrap().attachment;
    let mut channels = Vec::new();
    for (i, start, duration, from, to) in [(1, 0.0, 2.0, 0.2, 0.8), (2, 0.5, 0.75, 0.35, 0.9)] {
        let timing = TrackTiming::new(start, duration, RateFunction::Linear);
        channels.push(TrackDefinition {
            id: TrackId::new(i),
            object: object.id,
            property: Property::GlowIntensity,
            values: TrackValues::Glow {
                attachment,
                from: GlowTrackValue::Intensity(from),
                to: GlowTrackValue::Intensity(to),
            },
            timing,
            time_map: Default::default(),
        });
        channels.push(TrackDefinition {
            id: TrackId::new(i + 10),
            object: object.id,
            property: Property::Opacity,
            values: TrackValues::Scalar {
                from: from as f32,
                to: to as f32,
            },
            timing,
            time_map: Default::default(),
        });
    }
    let mut runtime =
        SceneInstance::new(CompiledScene::compile_objects(vec![object], &channels).unwrap());
    for time in [0.0, 0.4, 0.8, 1.3, 2.5, 0.3, 0.8] {
        runtime.evaluate(time).unwrap();
        assert!(
            (value(&runtime).intensity() - f64::from(runtime.frame().objects[0].style.opacity))
                .abs()
                < 1e-6
        );
    }
}
