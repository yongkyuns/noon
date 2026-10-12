use noon_compile::{
    lower_semantic_execution_root, CompiledObject, CompiledResources, CompiledScene,
    ExecutionMutationTransaction, SemanticExecutionIndex,
};
use noon_core::{
    CompositionTimeMap, GeometryRef, RateFunction, SemanticMutationTransaction,
    SemanticObjectState, SemanticOrientation, SemanticSpatialCompositionDomain, SemanticStore,
    SemanticTransform, SemanticVec3, StoredGeometry, TrackDefinition, TrackId, TrackTiming,
    TrackValues,
};

use super::*;
use crate::{EvaluationError, PreparedFrameCommitError, ReplayLimits, SceneInstance};

fn object() -> ObjectId {
    ObjectId::new(1)
}

fn base_style() -> Style {
    Style {
        fill: Some(Color::BLUE),
        stroke: Some(Color::WHITE),
        stroke_width: 3.0,
        opacity: 0.8,
        ..Style::default()
    }
}

fn scene(objects: usize, tracks: &[TrackDefinition]) -> CompiledScene {
    CompiledScene::compile_objects(
        (1..=objects)
            .map(|id| {
                CompiledObject::new(
                    ObjectId::new(id as u64),
                    GeometryRef::circle(1.0),
                    Transform2D::IDENTITY,
                    base_style(),
                )
            })
            .collect(),
        tracks,
    )
    .unwrap()
}

fn commit(instance: &mut SceneInstance, time: f64, writes: &[EffectivePropertyWrite]) {
    let phase = instance.prepare_advance_to(time).unwrap();
    let batch = instance.prepare_effective_property_batch(writes).unwrap();
    instance.commit_prepared_frame(phase, batch).unwrap();
}

fn track(id: u64, property: Property, values: TrackValues) -> TrackDefinition {
    TrackDefinition {
        id: TrackId::new(id),
        object: object(),
        property,
        values,
        timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        time_map: CompositionTimeMap::default(),
    }
}

#[test]
fn scale_and_fill_preserve_newly_evaluated_translation_rotation_and_opacity() {
    let mut instance = SceneInstance::new(scene(
        1,
        &[
            track(
                1,
                Property::Position,
                TrackValues::Vec2 {
                    from: Vec2::ZERO,
                    to: Vec2::new(8.0, 4.0),
                },
            ),
            track(
                2,
                Property::Rotation,
                TrackValues::Scalar { from: 0.0, to: 2.0 },
            ),
            track(
                3,
                Property::Opacity,
                TrackValues::Scalar { from: 0.8, to: 0.4 },
            ),
        ],
    ));
    instance.take_frame_changes();
    let before = instance.publication_context();
    // Prepare the narrow driver while the published object is still at t=0.
    // Applying it after timeline preparation must not restore that old snapshot.
    let writes = [
        EffectivePropertyWrite::Scale {
            object: object(),
            scale: Vec2::new(1.2, 1.2),
        },
        EffectivePropertyWrite::Fill {
            object: object(),
            fill: Some(Color::YELLOW),
        },
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
    assert_eq!(
        after.frame_epoch(),
        before.frame_epoch().checked_next().unwrap()
    );
    assert_eq!(instance.take_frame_changes().object_indices(), &[0]);
    assert_eq!(
        instance.compiled.objects()[0].base_transform,
        Transform2D::IDENTITY
    );
    assert_eq!(instance.compiled.objects()[0].base_style, base_style());
}

fn assign_expected(
    transform: &mut Transform2D,
    style: &mut Style,
    presence: &mut bool,
    write: EffectivePropertyWrite,
) {
    match write {
        EffectivePropertyWrite::Presence {
            presence: value, ..
        } => *presence = value,
        EffectivePropertyWrite::Transform {
            transform: value, ..
        } => *transform = value,
        EffectivePropertyWrite::WorldTransform { .. } => {}
        EffectivePropertyWrite::Style { style: value, .. } => *style = value,
        EffectivePropertyWrite::Translation { translation, .. } => {
            transform.translation = translation
        }
        EffectivePropertyWrite::Rotation { rotation, .. } => transform.rotation = rotation,
        EffectivePropertyWrite::Scale { scale, .. } => transform.scale = scale,
        EffectivePropertyWrite::Fill { fill, .. } => style.fill = fill,
        EffectivePropertyWrite::Stroke { stroke, .. } => style.stroke = stroke,
        EffectivePropertyWrite::StrokeWidth { stroke_width, .. } => {
            style.stroke_width = stroke_width
        }
        EffectivePropertyWrite::Opacity { opacity, .. } => style.opacity = opacity,
    }
}

#[test]
fn mixed_whole_and_component_writes_preserve_supplied_order() {
    let object = object();
    let pool = [
        EffectivePropertyWrite::Presence {
            object,
            presence: false,
        },
        EffectivePropertyWrite::Presence {
            object,
            presence: true,
        },
        EffectivePropertyWrite::Transform {
            object,
            transform: Transform2D {
                translation: Vec2::new(7.0, 2.0),
                rotation: 0.5,
                scale: Vec2::new(3.0, 4.0),
            },
        },
        EffectivePropertyWrite::Transform {
            object,
            transform: Transform2D::IDENTITY,
        },
        EffectivePropertyWrite::Style {
            object,
            style: Style {
                fill: Some(Color::RED),
                stroke: None,
                opacity: 0.3,
                ..base_style()
            },
        },
        EffectivePropertyWrite::Style {
            object,
            style: base_style(),
        },
        EffectivePropertyWrite::Translation {
            object,
            translation: Vec2::new(-3.0, 5.0),
        },
        EffectivePropertyWrite::Rotation {
            object,
            rotation: -0.7,
        },
        EffectivePropertyWrite::Scale {
            object,
            scale: Vec2::new(1.2, 1.2),
        },
        EffectivePropertyWrite::Scale {
            object,
            scale: Vec2::new(0.5, 2.0),
        },
        EffectivePropertyWrite::Fill {
            object,
            fill: Some(Color::YELLOW),
        },
        EffectivePropertyWrite::Fill { object, fill: None },
        EffectivePropertyWrite::Stroke {
            object,
            stroke: Some(Color::GREEN),
        },
        EffectivePropertyWrite::StrokeWidth {
            object,
            stroke_width: 8.0,
        },
        EffectivePropertyWrite::Opacity {
            object,
            opacity: 0.4,
        },
    ];
    let compiled = scene(1, &[]);
    for a in pool {
        for b in pool {
            for c in pool {
                let writes = [a, b, c];
                let mut expected_transform = Transform2D::IDENTITY;
                let mut expected_style = base_style();
                let mut expected_presence = true;
                for write in writes {
                    assign_expected(
                        &mut expected_transform,
                        &mut expected_style,
                        &mut expected_presence,
                        write,
                    );
                }
                let mut instance = SceneInstance::new(compiled.clone());
                commit(&mut instance, 0.0, &writes);
                let row = instance.effective_object(object).unwrap();
                assert_eq!(row.transform, expected_transform, "{writes:?}");
                assert_eq!(row.style, expected_style, "{writes:?}");
                assert_eq!(instance.frame.presences[0], expected_presence, "{writes:?}");
            }
        }
    }
}

#[test]
fn all_invalid_components_are_rejected_even_when_later_superseded() {
    let object = object();
    let invalid = [
        EffectivePropertyWrite::Translation {
            object,
            translation: Vec2::new(f32::NAN, 0.0),
        },
        EffectivePropertyWrite::Rotation {
            object,
            rotation: f32::INFINITY,
        },
        EffectivePropertyWrite::Scale {
            object,
            scale: Vec2::new(1.0, f32::NEG_INFINITY),
        },
        EffectivePropertyWrite::Fill {
            object,
            fill: Some(Color {
                alpha: f32::NAN,
                ..Color::RED
            }),
        },
        EffectivePropertyWrite::Stroke {
            object,
            stroke: Some(Color {
                red: f32::INFINITY,
                ..Color::RED
            }),
        },
        EffectivePropertyWrite::StrokeWidth {
            object,
            stroke_width: f32::NAN,
        },
        EffectivePropertyWrite::Opacity {
            object,
            opacity: f32::NAN,
        },
    ];
    for write in invalid {
        let mut instance = SceneInstance::new(scene(1, &[]));
        instance.take_frame_changes();
        let before = instance.frame().clone();
        let publication = instance.publication_context();
        assert!(
            instance
                .prepare_effective_property_batch(&[
                    write,
                    EffectivePropertyWrite::Transform {
                        object,
                        transform: Transform2D::IDENTITY
                    },
                    EffectivePropertyWrite::Style {
                        object,
                        style: base_style()
                    },
                ])
                .is_err(),
            "{write:?}"
        );
        assert_eq!(instance.frame(), &before);
        assert_eq!(instance.publication_context(), publication);
        assert!(instance.take_frame_changes().is_empty());
    }
}

#[test]
fn finite_style_components_keep_existing_unclamped_runtime_semantics() {
    let mut instance = SceneInstance::new(scene(1, &[]));
    let object = object();
    let style = Style {
        opacity: 1.1,
        stroke_width: -1.0,
        ..base_style()
    };
    let mut whole = instance.clone();
    commit(
        &mut whole,
        0.0,
        &[EffectivePropertyWrite::Style { object, style }],
    );
    commit(
        &mut instance,
        0.0,
        &[
            EffectivePropertyWrite::Opacity {
                object,
                opacity: style.opacity,
            },
            EffectivePropertyWrite::StrokeWidth {
                object,
                stroke_width: style.stroke_width,
            },
        ],
    );
    assert_eq!(
        instance.frame().objects[0].style,
        whole.frame().objects[0].style
    );
}

#[test]
fn authored_transaction_and_narrow_carry_forward_merge_at_commit() {
    let mut instance = SceneInstance::new(scene(1, &[]));
    let expected = instance.publication_context();
    let batch = instance
        .prepare_effective_property_batch(&[
            EffectivePropertyWrite::Scale {
                object: object(),
                scale: Vec2::new(1.2, 1.2),
            },
            EffectivePropertyWrite::Fill {
                object: object(),
                fill: Some(Color::YELLOW),
            },
        ])
        .unwrap();
    let transform = Transform2D {
        translation: Vec2::new(2.0, 3.0),
        rotation: 0.7,
        ..Transform2D::IDENTITY
    };
    let style = Style {
        stroke: Some(Color::RED),
        opacity: 0.3,
        ..base_style()
    };
    let transaction = ExecutionMutationTransaction::from_mutations([
        ExecutionPatch::SetTransform {
            object: object(),
            transform,
        },
        ExecutionPatch::SetStyle {
            object: object(),
            style,
        },
    ]);
    instance
        .apply_authored_execution_transaction_with_effective(
            &transaction,
            CompiledResources::default(),
            Some(batch),
            expected,
            expected.scene_revision().checked_next().unwrap(),
        )
        .unwrap();
    let row = instance.effective_object(object()).unwrap();
    assert_eq!(
        row.transform,
        Transform2D {
            scale: Vec2::new(1.2, 1.2),
            ..transform
        }
    );
    assert_eq!(
        row.style,
        Style {
            fill: Some(Color::YELLOW),
            ..style
        }
    );
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
    let batch = instance
        .prepare_effective_property_batch(&[EffectivePropertyWrite::Fill {
            object: ObjectId::new(5001),
            fill: Some(Color::YELLOW),
        }])
        .unwrap();
    assert_eq!(batch.len(), 1);
    instance.commit_prepared_frame(phase, batch).unwrap();
    assert_eq!(instance.take_frame_changes().object_indices(), &[5000]);
    assert_eq!(instance.frame().time, 0.0);
    assert_eq!(
        instance.publication_context().scene_revision(),
        before.scene_revision()
    );
    assert_eq!(
        instance.publication_context().execution_revision(),
        before.execution_revision()
    );
}

#[test]
fn same_value_component_does_not_publish_another_frame_epoch() {
    let mut instance = SceneInstance::new(scene(1, &[]));
    instance.take_frame_changes();
    let before = instance.publication_context();
    commit(
        &mut instance,
        0.0,
        &[
            EffectivePropertyWrite::Scale {
                object: object(),
                scale: Vec2::ONE,
            },
            EffectivePropertyWrite::Fill {
                object: object(),
                fill: base_style().fill,
            },
        ],
    );
    assert_eq!(instance.publication_context(), before);
    assert!(instance.take_frame_changes().is_empty());
}

#[test]
fn new_components_do_not_bypass_unknown_or_retired_identity_validation() {
    let mut instance = SceneInstance::new(scene(1, &[]));
    assert!(instance
        .prepare_effective_property_batch(&[EffectivePropertyWrite::Scale {
            object: ObjectId::new(2),
            scale: Vec2::ONE
        },])
        .is_err());
    instance
        .apply_execution_patch(&ExecutionPatch::RemoveObject(object()))
        .unwrap();
    assert!(instance
        .prepare_effective_property_batch(&[EffectivePropertyWrite::Fill {
            object: object(),
            fill: Some(Color::YELLOW)
        },])
        .is_err());
}

#[test]
fn stale_or_foreign_component_batch_cannot_overwrite_a_new_frame() {
    let mut instance = SceneInstance::new(scene(1, &[]));
    let mut foreign = instance.clone();
    let batch = instance
        .prepare_effective_property_batch(&[EffectivePropertyWrite::Scale {
            object: object(),
            scale: Vec2::new(1.2, 1.2),
        }])
        .unwrap();
    let foreign_phase = foreign.prepare_advance_to(0.0).unwrap();
    assert!(matches!(
        foreign.commit_prepared_frame(foreign_phase, batch.clone()),
        Err(PreparedFrameCommitError::ForeignRuntime { .. })
    ));
    commit(
        &mut instance,
        0.0,
        &[EffectivePropertyWrite::Rotation {
            object: object(),
            rotation: 0.5,
        }],
    );
    let frame = instance.frame().clone();
    let phase = instance.prepare_advance_to(0.0).unwrap();
    assert!(matches!(
        instance.commit_prepared_frame(phase, batch),
        Err(PreparedFrameCommitError::StalePublication { .. })
    ));
    assert_eq!(instance.frame(), &frame);
}

#[test]
fn scoped_write_vocabulary_does_not_unseal_replay() {
    let mut instance = SceneInstance::new(scene(1, &[]));
    instance
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    instance.seal_replay().unwrap();
    assert!(matches!(
        instance.prepare_advance_to(0.0),
        Err(EvaluationError::ReplaySealed)
    ));
    assert!(instance.replay_is_sealed());
}

#[test]
fn effective_world_transform_updates_only_frame_epoch_and_rejects_bad_camera_scale() {
    let id = object();
    let world = noon_core::SemanticWorldTransform3D::new(
        noon_core::SemanticVec3::new(4.0, -2.0, 3.0),
        noon_core::SemanticRotation3D::from_axis_angle(
            noon_core::SemanticVec3::new(0.0, 1.0, 0.0),
            0.75,
        )
        .unwrap(),
        noon_core::SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .unwrap();
    let mut mesh = CompiledObject::new(
        id,
        GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        base_style(),
    );
    mesh.spatial = Some(Box::new(noon_compile::CompiledSpatialState {
        world: noon_core::SemanticWorldTransform3D::IDENTITY,
        camera_projection: None,
        camera_profile: None,
        camera_motions: None,
        material: noon_core::SemanticSpatialMaterial::Unlit,
        point_light: false,
        composition_domain: noon_core::SemanticSpatialCompositionDomain::World,
        draw_kind: noon_compile::CompiledSpatialDrawKind::Planar,
        spatial_anchor_family: None,
        fixed_orientation_center: None,
        cairo_path_appearance: None,
    }));
    let mut instance = SceneInstance::new(CompiledScene::compile_objects(vec![mesh], &[]).unwrap());
    instance.take_frame_changes();
    let initial = instance.publication_context();
    let phase = instance.prepare_advance_to(0.0).unwrap();
    let batch = instance
        .prepare_effective_property_batch(&[EffectivePropertyWrite::WorldTransform {
            object: id,
            world,
        }])
        .unwrap();
    instance.commit_prepared_frame(phase, batch).unwrap();
    assert_eq!(instance.frame().objects[0].world_transform(), Some(world));
    assert_eq!(
        instance.compiled.objects()[0]
            .spatial
            .as_deref()
            .unwrap()
            .world,
        noon_core::SemanticWorldTransform3D::IDENTITY,
    );
    assert_eq!(
        instance.publication_context().scene_revision(),
        initial.scene_revision()
    );
    assert_eq!(
        instance.publication_context().execution_revision(),
        initial.execution_revision()
    );
    assert_eq!(
        instance.publication_context().frame_epoch(),
        initial.frame_epoch().checked_next().unwrap(),
    );
    assert_eq!(instance.take_frame_changes().object_indices(), &[0]);

    let mut camera = CompiledObject::new(
        id,
        GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        base_style(),
    );
    camera.spatial = Some(Box::new(noon_compile::CompiledSpatialState {
        world: noon_core::SemanticWorldTransform3D::IDENTITY,
        camera_projection: Some(noon_core::SemanticProjection3D::Perspective {
            vertical_fov_radians: 1.0,
            near: 0.1,
            far: 100.0,
        }),
        camera_profile: None,
        camera_motions: None,
        material: noon_core::SemanticSpatialMaterial::Unlit,
        point_light: false,
        composition_domain: noon_core::SemanticSpatialCompositionDomain::World,
        draw_kind: noon_compile::CompiledSpatialDrawKind::Planar,
        spatial_anchor_family: None,
        fixed_orientation_center: None,
        cairo_path_appearance: None,
    }));
    let camera_instance =
        SceneInstance::new(CompiledScene::compile_objects(vec![camera], &[]).unwrap());
    let bad_world = noon_core::SemanticWorldTransform3D::new(
        noon_core::SemanticVec3::ZERO,
        noon_core::SemanticRotation3D::IDENTITY,
        noon_core::SemanticVec3::new(2.0, 1.0, 1.0),
    )
    .unwrap();
    let before = camera_instance.frame().clone();
    let publication = camera_instance.publication_context();
    assert!(camera_instance
        .prepare_effective_property_batch(&[EffectivePropertyWrite::WorldTransform {
            object: id,
            world: bad_world
        },])
        .is_err());
    assert_eq!(camera_instance.frame(), &before);
    assert_eq!(camera_instance.publication_context(), publication);
}

#[test]
fn presence_effective_write_is_valid_on_spatial_mesh_rows() {
    let id = object();
    let mut mesh = CompiledObject::new(
        id,
        GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        base_style(),
    );
    mesh.spatial = Some(Box::new(noon_compile::CompiledSpatialState {
        world: noon_core::SemanticWorldTransform3D::IDENTITY,
        camera_projection: None,
        camera_profile: None,
        camera_motions: None,
        material: noon_core::SemanticSpatialMaterial::Unlit,
        point_light: false,
        composition_domain: noon_core::SemanticSpatialCompositionDomain::World,
        draw_kind: noon_compile::CompiledSpatialDrawKind::Mesh,
        spatial_anchor_family: None,
        fixed_orientation_center: None,
        cairo_path_appearance: None,
    }));
    let mut instance = SceneInstance::new(CompiledScene::compile_objects(vec![mesh], &[]).unwrap());
    let phase = instance.prepare_advance_to(0.0).unwrap();
    let batch = instance
        .prepare_effective_property_batch(&[EffectivePropertyWrite::Presence {
            object: id,
            presence: false,
        }])
        .unwrap();
    instance.commit_prepared_frame(phase, batch).unwrap();
    assert!(!instance.frame().is_present(0));
}

#[test]
fn fixed_orientation_family_rows_share_and_locally_refresh_world_bounds_center() {
    let mut store = SemanticStore::new();
    let anchor = store.insert_family();
    let (first, second) = {
        let mut insert_fixed = |x: f64| {
            let mut state = SemanticObjectState::new(StoredGeometry::Rectangle {
                size: Vec2::new(2.0, 2.0),
            });
            state.transform = SemanticTransform {
                translation: SemanticVec3::new(x, 0.0, 0.0),
                scale: SemanticVec3::new(1.0, 1.0, 1.0),
                orientation: SemanticOrientation::Spatial(noon_core::SemanticRotation3D::IDENTITY),
            };
            store.insert_semantic_object(state)
        };
        (insert_fixed(-3.0), insert_fixed(5.0))
    };
    store.add_semantic_family_member(anchor, first).unwrap();
    store.add_semantic_family_member(anchor, second).unwrap();
    let mut declaration = SemanticMutationTransaction::new();
    for member in [first, second] {
        declaration.set_spatial_composition_domain_with_anchor(
            member,
            SemanticSpatialCompositionDomain::FixedOrientation,
            Some(anchor),
        );
    }
    declaration.apply(&mut store).unwrap();

    let mut index = SemanticExecutionIndex::new();
    let lowered = lower_semantic_execution_root(&store, anchor, &mut index).unwrap();
    let first_id = index.execution_object_id(first).unwrap();
    let second_id = index.execution_object_id(second).unwrap();
    let mut instance = SceneInstance::from_semantic_execution(lowered);
    let expected_initial = noon_core::SemanticVec3::new(1.0, 0.0, 0.0);
    for row in &instance.frame().objects {
        assert_eq!(
            row.spatial.as_deref().unwrap().fixed_orientation_center,
            Some(expected_initial)
        );
    }
    instance.take_frame_changes();

    // Move one member only. The derived group center changes for both rows, while
    // the authored/compiled pose and untouched row remain independent.
    let moved_world = noon_core::SemanticWorldTransform3D::new(
        noon_core::SemanticVec3::new(-1.0, 0.0, 0.0),
        noon_core::SemanticRotation3D::IDENTITY,
        noon_core::SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .unwrap();
    commit(
        &mut instance,
        0.0,
        &[EffectivePropertyWrite::WorldTransform {
            object: first_id,
            world: moved_world,
        }],
    );
    let expected_updated = noon_core::SemanticVec3::new(2.0, 0.0, 0.0);
    assert_eq!(
        instance.frame().objects[0]
            .spatial
            .as_deref()
            .unwrap()
            .fixed_orientation_center,
        Some(expected_updated)
    );
    assert_eq!(
        instance.frame().objects[1]
            .spatial
            .as_deref()
            .unwrap()
            .fixed_orientation_center,
        Some(expected_updated)
    );
    assert_eq!(
        instance.frame().objects[1]
            .world_transform()
            .unwrap()
            .translation
            .x,
        5.0
    );
    assert_eq!(instance.frame().objects[0].id, first_id);
    assert_eq!(instance.frame().objects[1].id, second_id);
    assert_eq!(instance.take_frame_changes().object_indices(), &[0, 1]);
}

#[test]
fn cairo_path_family_gradient_uses_transformed_path_controls_and_refreshes_locally() {
    let mut store = SemanticStore::new();
    let anchor = store.insert_family();
    let mut make_path = |path: noon_core::VectorPath, x: f64, y: f64, cairo: bool| {
        let handle = store.insert_geometry_path(path).unwrap();
        let mut state = SemanticObjectState::new(StoredGeometry::Resource(handle));
        if cairo {
            state.set_spatial_material(noon_core::SemanticSpatialMaterial::CairoPath);
            state
                .set_cairo_path_appearance(noon_core::SemanticCairoPathAppearance {
                    sheen_factor: 0.2,
                    gradient_direction: Some(SemanticVec3::new(0.0, 1.0, 0.0)),
                })
                .unwrap();
        }
        state.transform = SemanticTransform {
            translation: SemanticVec3::new(x, y, 0.0),
            scale: SemanticVec3::new(1.0, 1.0, 1.0),
            orientation: SemanticOrientation::Spatial(noon_core::SemanticRotation3D::IDENTITY),
        };
        store.insert_semantic_object(state)
    };
    let owner = make_path(
        noon_core::VectorPath::new()
            .move_to(noon_core::Vec2::new(0.0, 0.0))
            .line_to(noon_core::Vec2::new(1.0, 0.0)),
        0.0,
        0.0,
        true,
    );
    let shaft = make_path(
        noon_core::VectorPath::new()
            .move_to(noon_core::Vec2::new(-2.0, 0.0))
            .line_to(noon_core::Vec2::new(2.0, 0.0)),
        0.0,
        1.0,
        true,
    );
    let tip = make_path(
        noon_core::VectorPath::new()
            .move_to(noon_core::Vec2::new(1.0, 1.0))
            .line_to(noon_core::Vec2::new(2.0, 1.0))
            .line_to(noon_core::Vec2::new(1.5, 2.0))
            .close(),
        0.0,
        0.0,
        false,
    );
    for child in [owner, shaft, tip] {
        store.add_semantic_family_member(anchor, child).unwrap();
    }
    let mut declaration = SemanticMutationTransaction::new();
    declaration.set_spatial_composition_domain_with_anchor(
        owner,
        SemanticSpatialCompositionDomain::World,
        Some(anchor),
    );
    declaration.set_spatial_composition_domain_with_anchor(
        shaft,
        SemanticSpatialCompositionDomain::World,
        Some(anchor),
    );
    declaration.apply(&mut store).unwrap();

    let mut index = SemanticExecutionIndex::new();
    let lowered = lower_semantic_execution_root(&store, anchor, &mut index).unwrap();
    let owner_id = index.execution_object_id(owner).unwrap();
    let shaft_id = index.execution_object_id(shaft).unwrap();
    let mut instance = SceneInstance::from_semantic_execution(lowered);
    let row_for = |id| {
        instance
            .frame()
            .objects
            .iter()
            .position(|row| row.id == id)
            .unwrap()
    };
    let owner_row = row_for(owner_id);
    let shaft_row = row_for(shaft_id);
    let tip_row = row_for(index.execution_object_id(tip).unwrap());
    let bounds = instance.frame().objects[owner_row]
        .spatial
        .as_deref()
        .unwrap()
        .cairo_path_appearance
        .as_deref()
        .unwrap()
        .world_family_bounds
        .unwrap();
    assert_eq!(bounds.min, SemanticVec3::new(-2.0, 0.0, 0.0));
    assert_eq!(bounds.max, SemanticVec3::new(2.0, 2.0, 0.0));
    instance.take_frame_changes();

    let moved = noon_core::SemanticWorldTransform3D::new(
        SemanticVec3::new(0.0, 0.0, 3.0),
        noon_core::SemanticRotation3D::IDENTITY,
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .unwrap();
    commit(
        &mut instance,
        0.0,
        &[EffectivePropertyWrite::WorldTransform {
            object: shaft_id,
            world: moved,
        }],
    );
    let bounds = instance.frame().objects[owner_row]
        .spatial
        .as_deref()
        .unwrap()
        .cairo_path_appearance
        .as_deref()
        .unwrap()
        .world_family_bounds
        .unwrap();
    assert_eq!(bounds.min.z, 0.0);
    assert_eq!(bounds.max.z, 3.0);
    let shaft_bounds = instance.frame().objects[shaft_row]
        .spatial
        .as_deref()
        .unwrap()
        .cairo_path_appearance
        .as_deref()
        .unwrap()
        .world_family_bounds
        .unwrap();
    assert_eq!(shaft_bounds, bounds);
    let mut expected_changed = vec![owner_row, shaft_row];
    expected_changed.sort_unstable();
    assert_eq!(
        instance.take_frame_changes().object_indices(),
        expected_changed
    );
    assert_eq!(
        instance.frame().objects[tip_row]
            .world_transform()
            .unwrap()
            .translation
            .z,
        0.0
    );
}

#[test]
fn prepared_spatial_state_promotes_one_row_and_preserves_2d_payload() {
    let id = object();
    let mut instance = SceneInstance::new(scene(2, &[]));
    instance.take_frame_changes();
    let before = instance.publication_context();
    let mut planar = Transform2D::IDENTITY;
    planar.translation = Vec2::new(2.0, -3.0);
    let spatial = noon_compile::CompiledSpatialState {
        world: noon_core::SemanticWorldTransform3D::IDENTITY,
        camera_projection: None,
        camera_profile: None,
        camera_motions: None,
        material: noon_core::SemanticSpatialMaterial::Unlit,
        point_light: false,
        composition_domain: noon_core::SemanticSpatialCompositionDomain::FixedFrame,
        draw_kind: noon_compile::CompiledSpatialDrawKind::Planar,
        spatial_anchor_family: None,
        fixed_orientation_center: None,
        cairo_path_appearance: None,
    };
    instance
        .apply_execution_patch(&noon_compile::ExecutionPatch::SetSpatialState {
            object: id,
            base_transform: planar,
            spatial: Some(spatial.clone()),
        })
        .unwrap();

    let row = &instance.frame().objects[0];
    assert_eq!(row.transform, planar);
    assert_eq!(row.style, base_style());
    assert_eq!(row.spatial.as_deref(), Some(&spatial));
    assert!(row.geometry().is_some());
    assert_eq!(instance.take_frame_changes().object_indices(), &[0]);
    assert_eq!(
        instance.publication_context().execution_revision(),
        before.execution_revision().checked_next().unwrap()
    );
}
