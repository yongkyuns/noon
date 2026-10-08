//! Existing-attachment base edits use the same authored value/publication lane
//! as ordinary transforms and styles. Full Scene activation is still guarded.
use noon_compile::{
    CompilePatchError, CompiledGlow, CompiledObject, CompiledScene, ExecutionMutationTransaction,
    ExecutionPatch,
};
use noon_core::{
    Color, GeometryRef, Glow, GlowSource, GlowUpdate, ObjectId, Pixels, RateFunction,
    SemanticNodeId, Style, TrackDefinition, TrackId, TrackTiming, Transform2D, Vec2,
};
use noon_runtime::{FrameChanges, SceneInstance};
use std::sync::Arc;

fn object() -> CompiledObject {
    let mut object = CompiledObject::new(
        ObjectId::new(1),
        GeometryRef::circle(0.4),
        Transform2D::IDENTITY,
        Style::default(),
    );
    object.glow = Some(Arc::new(CompiledGlow {
        attachment: SemanticNodeId::new(7, 3),
        definition: Glow::new(
            GlowUpdate::default()
                .radius(Pixels(3.25))
                .color(Color::RED)
                .intensity(0.4),
        )
        .unwrap(),
    }));
    object
}
fn update(object: &CompiledObject, patch: GlowUpdate) -> Arc<CompiledGlow> {
    let original = object.glow.as_ref().unwrap();
    Arc::new(CompiledGlow {
        attachment: original.attachment,
        definition: patch.apply_to(original.definition).unwrap(),
    })
}
fn value(runtime: &SceneInstance) -> Glow {
    runtime.frame().objects[0].glow.as_ref().unwrap().definition
}
fn tx(patches: Vec<ExecutionPatch>) -> ExecutionMutationTransaction {
    ExecutionMutationTransaction::from_mutations(patches)
}
fn publish(runtime: &mut SceneInstance, transaction: &ExecutionMutationTransaction) {
    let context = runtime.publication_context();
    let prepared = runtime
        .prepare_authored_value_publication(
            transaction,
            context,
            context.scene_revision().checked_next().unwrap(),
        )
        .unwrap()
        .expect("existing glow is an ordinary local value");
    runtime.commit_prepared_authored_value_publication(prepared);
}
fn runtime(object: CompiledObject) -> SceneInstance {
    SceneInstance::new(CompiledScene::compile_objects(vec![object], &[]).unwrap())
}
fn intensity_track(object: &CompiledObject) -> TrackDefinition {
    let (property, values) = object
        .glow
        .as_ref()
        .unwrap()
        .parameter_channels(GlowUpdate::default().intensity(2.0))
        .unwrap()
        .pop()
        .unwrap();
    TrackDefinition {
        id: TrackId::new(1),
        object: object.id,
        property,
        values,
        timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        time_map: Default::default(),
    }
}

#[test]
fn prepared_glow_is_atomic_sparse_and_does_not_dirty_geometry() {
    let object = object();
    let original = object.glow.clone().unwrap();
    let changed = update(
        &object,
        GlowUpdate::default()
            .color(Color::BLUE)
            .radius(Pixels(4.1234567890123))
            .intensity(1.4),
    );
    let mut runtime = runtime(object);
    runtime.take_frame_changes();
    runtime.take_spatial_changes();
    let before = runtime.frame().clone();
    let context = runtime.publication_context();
    let transaction = tx(vec![ExecutionPatch::SetGlow {
        object: ObjectId::new(1),
        glow: changed.clone(),
    }]);
    let prepared = runtime
        .prepare_authored_value_publication(
            &transaction,
            context,
            context.scene_revision().checked_next().unwrap(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(runtime.frame(), &before);
    assert_eq!(runtime.publication_context(), context);
    drop(prepared);
    assert!(runtime.frame_changes().is_empty());
    publish(&mut runtime, &transaction);
    assert_eq!(value(&runtime), changed.definition);
    assert_eq!(original.definition.intensity(), 0.4);
    assert_eq!(runtime.frame_changes(), &FrameChanges::objects(vec![0]));
    assert!(runtime.take_spatial_changes().is_empty());
    assert_eq!(
        runtime.frame().objects[0].transform,
        before.objects[0].transform
    );
    assert_eq!(runtime.frame().objects[0].style, before.objects[0].style);
    assert_eq!(
        runtime.publication_context().scene_revision(),
        context.scene_revision().checked_next().unwrap()
    );
    assert_eq!(
        runtime.publication_context().execution_revision(),
        context.execution_revision().checked_next().unwrap()
    );
    assert!(Arc::ptr_eq(
        runtime.frame().objects[0].glow.as_ref().unwrap(),
        &changed
    ));
}

#[test]
fn final_glow_base_coalesces_independently_of_style_and_transform() {
    let object = object();
    let first = update(&object, GlowUpdate::default().intensity(1.0));
    let last = update(
        &object,
        GlowUpdate::default().intensity(1.7).color(Color::BLUE),
    );
    let mut runtime = runtime(object);
    let style = Style {
        fill: Some(Color::BLUE),
        ..Style::default()
    };
    let transform = Transform2D {
        translation: Vec2::new(0.25, -0.75),
        ..Transform2D::IDENTITY
    };
    publish(
        &mut runtime,
        &tx(vec![
            ExecutionPatch::SetGlow {
                object: ObjectId::new(1),
                glow: first,
            },
            ExecutionPatch::SetStyle {
                object: ObjectId::new(1),
                style,
            },
            ExecutionPatch::SetGlow {
                object: ObjectId::new(1),
                glow: last.clone(),
            },
            ExecutionPatch::SetTransform {
                object: ObjectId::new(1),
                transform,
            },
        ]),
    );
    assert_eq!(value(&runtime), last.definition);
    assert_eq!(runtime.frame().objects[0].style, style);
    assert_eq!(runtime.frame().objects[0].transform, transform);
}

#[test]
fn rejected_rebinding_and_discrete_changes_cannot_commit_a_prefix_or_hide_as_superseded() {
    let object = object();
    let good = update(&object, GlowUpdate::default().intensity(2.0));
    let mut rebound = (*good).clone();
    rebound.attachment = SemanticNodeId::new(7, 4);
    let mut runtime = runtime(object.clone());
    runtime.take_frame_changes();
    let original = runtime.frame().clone();
    let context = runtime.publication_context();
    for bad in [
        Arc::new(rebound),
        update(
            &object,
            GlowUpdate::default().source(GlowSource::Silhouette),
        ),
        update(&object, GlowUpdate::default().radius(0.3)),
    ] {
        let transaction = tx(vec![
            ExecutionPatch::SetTransform {
                object: object.id,
                transform: Transform2D {
                    translation: Vec2::new(99.0, 1.0),
                    ..Transform2D::IDENTITY
                },
            },
            ExecutionPatch::SetGlow {
                object: object.id,
                glow: bad,
            },
            ExecutionPatch::SetGlow {
                object: object.id,
                glow: good.clone(),
            },
        ]);
        assert!(runtime
            .prepare_authored_value_publication(
                &transaction,
                context,
                context.scene_revision().checked_next().unwrap()
            )
            .is_err());
        assert!(runtime.apply_execution_transaction(&transaction).is_err());
        assert_eq!(runtime.frame(), &original);
        assert_eq!(runtime.publication_context(), context);
        assert!(runtime.frame_changes().is_empty());
    }
    let mut plain = object.clone();
    plain.glow = None;
    let mut empty = self::runtime(plain);
    assert_eq!(
        empty
            .apply_execution_patch(&ExecutionPatch::SetGlow {
                object: object.id,
                glow: good
            })
            .unwrap_err(),
        CompilePatchError::InvalidGlowUpdate { object: object.id }
    );
}

#[test]
fn active_channel_keeps_its_value_and_removal_reveals_updated_base() {
    let object = object();
    let track = intensity_track(&object);
    let compiled = CompiledScene::compile_objects(vec![object.clone()], &[track]).unwrap();
    let mut direct = SceneInstance::new(compiled.clone());
    let mut prepared = SceneInstance::new(compiled);
    direct.advance_to(0.4).unwrap();
    prepared.advance_to(0.4).unwrap();
    let changed = update(
        &object,
        GlowUpdate::default()
            .color(Color::BLUE)
            .radius(Pixels(6.125))
            .intensity(0.15),
    );
    let patch = ExecutionPatch::SetGlow {
        object: object.id,
        glow: changed.clone(),
    };
    direct.apply_execution_patch(&patch).unwrap();
    publish(&mut prepared, &tx(vec![patch]));
    assert_eq!(direct.frame(), prepared.frame());
    assert!((value(&prepared).intensity() - 1.04).abs() < 1e-12);
    assert_eq!(value(&prepared).color(), Color::BLUE);
    assert_eq!(value(&prepared).radius(), Pixels(6.125).into());
    // Seek follows existing track semantics, never an extra effects clock.
    for time in [0.0, 0.8, 1.0, 0.4] {
        direct.seek(time).unwrap();
        prepared.seek(time).unwrap();
        assert_eq!(direct.frame(), prepared.frame());
    }
    prepared
        .apply_execution_patch(&ExecutionPatch::RemoveTrack(TrackId::new(1)))
        .unwrap();
    assert_eq!(value(&prepared), changed.definition);
}

#[test]
fn exact_noop_and_stale_context_preserve_shared_values_and_versions() {
    let object = object();
    let mut runtime = runtime(object.clone());
    runtime.take_frame_changes();
    runtime.take_spatial_changes();
    let context = runtime.publication_context();
    let retained = runtime.frame().objects[0].glow.clone().unwrap();
    let transaction = tx(vec![ExecutionPatch::SetGlow {
        object: object.id,
        glow: object.glow.clone().unwrap(),
    }]);
    let prepared = runtime
        .prepare_authored_value_publication(&transaction, context, context.scene_revision())
        .unwrap()
        .unwrap();
    runtime.commit_prepared_authored_value_publication(prepared);
    assert_eq!(runtime.publication_context(), context);
    assert!(runtime.frame_changes().is_empty());
    assert!(runtime.take_spatial_changes().is_empty());
    assert!(Arc::ptr_eq(
        &retained,
        runtime.frame().objects[0].glow.as_ref().unwrap()
    ));
    runtime.advance_to(0.1).unwrap();
    assert!(runtime
        .prepare_authored_value_publication(&transaction, context, context.scene_revision())
        .is_err());
}

#[test]
fn one_live_glow_preserves_4096_unrelated_rows_and_resource_identities() {
    let object = object();
    let mut objects = vec![object.clone()];
    for id in 2..=4097 {
        objects.push(CompiledObject::new(
            ObjectId::new(id),
            GeometryRef::circle(0.1),
            Transform2D::IDENTITY,
            Style::default(),
        ));
    }
    let mut runtime = SceneInstance::new(CompiledScene::compile_objects(objects, &[]).unwrap());
    runtime.take_frame_changes();
    runtime.take_spatial_changes();
    let tail = runtime.frame().objects[1..].to_vec();
    let changed = update(&object, GlowUpdate::default().intensity(1.8));
    publish(
        &mut runtime,
        &tx(vec![ExecutionPatch::SetGlow {
            object: object.id,
            glow: changed,
        }]),
    );
    assert_eq!(runtime.frame_changes(), &FrameChanges::objects(vec![0]));
    assert!(runtime.take_spatial_changes().is_empty());
    assert_eq!(&runtime.frame().objects[1..], tail.as_slice());
    assert_eq!(runtime.last_patch_stats().objects_recomputed, 0);
}

#[test]
fn direct_value_publication_marks_only_render_changes() {
    let object = object();
    let mut runtime = runtime(object.clone());
    runtime.take_frame_changes();
    runtime.take_spatial_changes();
    runtime
        .apply_execution_patch(&ExecutionPatch::SetGlow {
            object: object.id,
            glow: update(&object, GlowUpdate::default().intensity(1.8)),
        })
        .unwrap();
    assert_eq!(runtime.frame_changes(), &FrameChanges::objects(vec![0]));
    assert!(runtime.take_spatial_changes().is_empty());
}

#[test]
fn reconciled_endpoint_uses_the_prepared_new_base_not_the_previous_compiled_value() {
    let object = object();
    let track = intensity_track(&object);
    let property = track.property;
    let mut runtime =
        SceneInstance::new(CompiledScene::compile_objects(vec![object.clone()], &[track]).unwrap());
    runtime.seek(1.0).unwrap();
    runtime
        .apply_execution_patch(&ExecutionPatch::ReconcileTrack {
            track: TrackId::new(1),
            object: object.id,
            property,
            end_time: 1.0,
        })
        .unwrap();
    let changed = update(&object, GlowUpdate::default().intensity(0.72));
    publish(
        &mut runtime,
        &tx(vec![ExecutionPatch::SetGlow {
            object: object.id,
            glow: changed.clone(),
        }]),
    );
    assert_eq!(value(&runtime), changed.definition);
    runtime.seek(0.5).unwrap();
    assert!((value(&runtime).intensity() - 1.2).abs() < 1e-12);
    runtime.seek(1.0).unwrap();
    assert_eq!(value(&runtime), changed.definition);
}

#[test]
fn glow_completion_preflight_keeps_generation_overlap_and_exact_endpoint_checks() {
    let object = object();
    let compiled = CompiledScene::compile_objects(vec![object.clone()], &[]).unwrap();
    let valid = intensity_track(&object);
    compiled
        .preflight_reconcilable_track_additions(std::slice::from_ref(&valid))
        .unwrap();
    let mut invalid = valid.clone();
    if let noon_core::TrackValues::Glow {
        ref mut attachment, ..
    } = invalid.values
    {
        *attachment = SemanticNodeId::new(7, 4);
    }
    assert!(compiled
        .preflight_reconcilable_track_additions(&[invalid])
        .is_err());
    let mut overlapping = valid.clone();
    overlapping.id = TrackId::new(2);
    assert!(compiled
        .preflight_reconcilable_track_additions(&[valid.clone(), overlapping])
        .is_err());
    let mut runtime = SceneInstance::new(
        CompiledScene::compile_objects(vec![object.clone()], std::slice::from_ref(&valid)).unwrap(),
    );
    let before = runtime.frame().clone();
    let transaction = tx(vec![
        ExecutionPatch::SetGlow {
            object: object.id,
            glow: update(&object, GlowUpdate::default().intensity(0.7)),
        },
        ExecutionPatch::ReconcileTrack {
            track: valid.id,
            object: object.id,
            property: valid.property,
            end_time: 0.9,
        },
    ]);
    assert!(runtime.apply_execution_transaction(&transaction).is_err());
    assert_eq!(runtime.frame(), &before);
}
