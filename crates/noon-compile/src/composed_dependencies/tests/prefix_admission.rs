use super::*;

fn spatial() -> crate::CompiledSpatialState {
    crate::CompiledSpatialState {
        world: noon_core::SemanticWorldTransform3D::IDENTITY,
        camera_projection: None,
        camera_profile: None,
        camera_motions: None,
        material: noon_core::SemanticSpatialMaterial::Unlit,
        point_light: false,
        composition_domain: noon_core::SemanticSpatialCompositionDomain::World,
        draw_kind: crate::CompiledSpatialDrawKind::Planar,
        spatial_anchor_family: None,
        fixed_orientation_center: None,
        cairo_path_appearance: None,
    }
}
fn spatial_leaf(value: u64, z: f64) -> CompiledObject {
    let mut row = leaf(value, z);
    row.spatial = Some(Box::new(spatial()));
    row
}
fn panel(value: u64, z: f64) -> CompiledObject {
    let mut row = leaf(value, z);
    row.effects = Some(backdrop(31, 2.0));
    row
}
fn unsupported(error: CompilePatchError, owner: u64, input: u64) {
    assert_eq!(
        error,
        CompilePatchError::InvalidComposedSource(CompiledComposedSourceError {
            owner: id(owner),
            kind: CompiledComposedSourceErrorKind::UnsupportedBackdropInput(id(input))
        })
    );
}
#[test]
fn prefix_admission_rejects_earlier_spatial_draws_but_preserves_later_draws_and_atomic_failure() {
    assert!(
        matches!(CompiledScene::compile_objects(vec![spatial_leaf(1,0.0),panel(2,1.0)],&[]),Err(CompileError::InvalidComposedSource(CompiledComposedSourceError {kind:CompiledComposedSourceErrorKind::UnsupportedBackdropInput(input),..})) if input==id(1))
    );
    let mut scene =
        CompiledScene::compile_objects(vec![panel(2, 1.0), spatial_leaf(1, 2.0)], &[]).unwrap();
    let before = scene.clone();
    let bad = ExecutionPatch::SetZIndex {
        object: id(1),
        value: 0.0,
    };
    unsupported(scene.apply_execution_patch(&bad).unwrap_err(), 2, 1);
    assert_eq!(scene, before);
    let mut earlier = leaf(3, 0.0);
    earlier.draw_enabled = false;
    scene
        .apply_execution_patch(&ExecutionPatch::CreateObject(earlier))
        .unwrap();
    scene
        .apply_execution_patch(&ExecutionPatch::SetSpatialState {
            object: id(3),
            base_transform: Transform2D::IDENTITY,
            spatial: Some(spatial()),
        })
        .unwrap();
    let before = scene.clone();
    unsupported(
        scene
            .apply_execution_patch(&ExecutionPatch::SetDrawEnabled {
                object: id(3),
                enabled: true,
            })
            .unwrap_err(),
        2,
        3,
    );
    assert_eq!(scene, before);
    let before = scene.clone();
    let tx = ExecutionMutationTransaction::from_mutations(vec![
        ExecutionPatch::SetZIndex {
            object: id(1),
            value: 0.0,
        },
        ExecutionPatch::SetStyle {
            object: id(2),
            style: Style {
                opacity: 0.4,
                ..Style::default()
            },
            fill_paint: None,
        },
    ]);
    assert!(scene.preflight_execution_transaction(&tx).is_err());
    assert_eq!(scene, before);
}
#[test]
fn prefix_capability_checks_observe_the_complete_final_transaction_in_either_order() {
    for reverse in [false, true] {
        let mut scene =
            CompiledScene::compile_objects(vec![panel(2, 1.0), spatial_leaf(1, 2.0)], &[]).unwrap();
        let mut patches = vec![
            ExecutionPatch::SetZIndex {
                object: id(1),
                value: 0.0,
            },
            ExecutionPatch::SetEffectStack {
                object: id(2),
                expected: vec![noon_core::SemanticNodeId::new(31, 5)].into_boxed_slice(),
                effects: None,
            },
        ];
        // Use the fixture's exact semantic attachment generation.
        if let ExecutionPatch::SetEffectStack { expected, .. } = &mut patches[1] {
            *expected = scene.objects[0]
                .effects
                .as_ref()
                .unwrap()
                .attachments()
                .to_vec()
                .into_boxed_slice();
        }
        if reverse {
            patches.reverse();
        }
        commit(&mut scene, patches);
        let before = scene.clone();
        let patch = ExecutionPatch::SetEffectStack {
            object: id(2),
            expected: Box::default(),
            effects: Some(backdrop(31, 2.0)),
        };
        unsupported(scene.apply_execution_patch(&patch).unwrap_err(), 2, 1);
        assert_eq!(scene, before);
    }
}
#[test]
fn prefix_admission_tracks_the_first_physical_group_anchor() {
    let mut glass = scope(90, &[1, 2], None);
    glass.effects = Some(backdrop(31, 2.0));
    let mut scene = CompiledScene::compile_objects(
        vec![leaf(1, 1.0), leaf(2, 2.0), spatial_leaf(3, 3.0), glass],
        &[],
    )
    .unwrap();
    let before = scene.clone();
    unsupported(
        scene
            .apply_execution_patch(&ExecutionPatch::SetZIndex {
                object: id(3),
                value: 0.0,
            })
            .unwrap_err(),
        90,
        3,
    );
    assert_eq!(scene, before);
    // Changing only an isolated scope's metadata rank cannot change its input
    // anchor. Capability admission is based on the first physical member.
    scene
        .apply_execution_patch(&ExecutionPatch::SetZIndex {
            object: id(90),
            value: -100.0,
        })
        .unwrap();
    let bad = ExecutionPatch::SetZIndex {
        object: id(3),
        value: 0.0,
    };
    unsupported(scene.apply_execution_patch(&bad).unwrap_err(), 90, 3);
}
#[test]
fn backdrop_enrollment_and_parameter_validation_stay_local_with_ten_thousand_later_spatial_rows() {
    let rows = (0..20_000)
        .map(|i| {
            if i >= 10_000 {
                spatial_leaf(i, i as f64)
            } else {
                leaf(i, i as f64)
            }
        })
        .collect();
    let mut scene = CompiledScene::compile_objects(rows, &[]).unwrap();
    let patch = ExecutionPatch::SetEffectStack {
        object: id(9999),
        expected: Box::default(),
        effects: Some(backdrop(31, 2.0)),
    };
    let stats = scene
        .preflight_composed_mutations(std::slice::from_ref(&patch))
        .unwrap();
    assert_eq!(stats.prefix_rows_examined, 1);
    assert_eq!(stats.members_examined, 0);
    assert_eq!(stats.painter_rows_staged, 0);
    scene.apply_execution_patch(&patch).unwrap();
    let parameter = ExecutionPatch::SetEffects {
        object: id(9999),
        effects: backdrop(31, 2.5),
    };
    assert_eq!(
        scene
            .preflight_composed_mutations(std::slice::from_ref(&parameter))
            .unwrap(),
        ComposedValidationStats::default()
    );
    scene.apply_execution_patch(&parameter).unwrap();
    let retained = scene.composed_dependencies.clone();
    scene
        .apply_execution_patch(&ExecutionPatch::SetTransform {
            object: id(0),
            transform: Transform2D {
                translation: noon_core::Vec2::new(1.0, 0.0),
                ..Transform2D::IDENTITY
            },
        })
        .unwrap();
    assert_eq!(scene.composed_dependencies, retained);
}

#[test]
fn prefix_capability_metadata_survives_replay_retirement_and_compaction() {
    let mut scene = CompiledScene::compile_objects(
        vec![leaf(7, 0.0), panel(2, 1.0), spatial_leaf(3, 2.0)],
        &[],
    )
    .unwrap();
    let initial = scene.clone();
    let patch = ExecutionPatch::SetDrawEnabled {
        object: id(3),
        enabled: false,
    };
    let mut revision = scene.prepare_replay_revision(&patch).unwrap();
    scene.apply_execution_patch(&patch).unwrap();
    let disabled = scene.clone();
    scene.exchange_replay_revision(&mut revision);
    assert_eq!(scene, initial);
    unsupported(
        scene
            .apply_execution_patch(&ExecutionPatch::SetZIndex {
                object: id(3),
                value: -1.0,
            })
            .unwrap_err(),
        2,
        3,
    );
    scene.exchange_replay_revision(&mut revision);
    assert_eq!(scene, disabled);
    scene
        .apply_execution_patch(&ExecutionPatch::SetZIndex {
            object: id(3),
            value: -1.0,
        })
        .unwrap();
    scene
        .apply_execution_patch(&ExecutionPatch::RemoveObject(id(7)))
        .unwrap();
    scene.compact_retired_object_slots().unwrap();
    let before = scene.clone();
    unsupported(
        scene
            .apply_execution_patch(&ExecutionPatch::SetDrawEnabled {
                object: id(3),
                enabled: true,
            })
            .unwrap_err(),
        2,
        3,
    );
    assert_eq!(scene, before);
}
