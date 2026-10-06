use super::*;
use crate::WorldAffineEdit;
use noon_core::{AnimationOptions, RateFunction, SemanticProjection3D, SemanticRotation3D};
use noon_geometry::{SurfaceSample, UvSurfacePlan};

fn camera() -> SemanticCamera3D {
    SemanticCamera3D::new(
        SemanticVec3::new(0.0, 0.0, 5.0),
        SemanticRotation3D::IDENTITY,
        SemanticProjection3D::Perspective {
            vertical_fov_radians: 1.0,
            near: 0.1,
            far: 100.0,
        },
    )
    .unwrap()
}

fn cube() -> MeshOptions {
    MeshOptions::new(noon_geometry::cube_mesh(1.0).unwrap())
}

fn offset_mesh() -> MeshOptions {
    let positions = vec![
        SemanticVec3::new(10.0, 0.0, 0.0),
        SemanticVec3::new(12.0, 0.0, 0.0),
        SemanticVec3::new(10.0, 2.0, 2.0),
        SemanticVec3::new(12.0, 2.0, 2.0),
    ];
    MeshOptions::new(MeshResource::new(positions, None, vec![0, 1, 2, 1, 3, 2]).unwrap())
}

fn flat_grid() -> SurfaceGrid {
    let plan = UvSurfacePlan::new([0.0, 1.0], [0.0, 1.0], [2, 2]).unwrap();
    let samples = plan
        .coordinates()
        .map(|(u, v)| SurfaceSample::position(SemanticVec3::new(u, v, 0.0)))
        .collect::<Vec<_>>();
    plan.finish_samples(samples).unwrap()
}

#[test]
fn cairo_surface_and_cairo_cap_publish_as_one_family_and_checkerboard_only_cells() {
    let grid = UvSurfacePlan::new([0.0, 1.0], [0.0, std::f64::consts::TAU], [1, 1])
        .unwrap()
        .sample_cairo(|u, v| SemanticVec3::new(u * v.cos(), u * v.sin(), 0.0))
        .unwrap();
    let cap = SpatialPathOptions::circle(
        1.0,
        SemanticWorldTransform3D::from_axial_direction(SemanticVec3::new(0.0, 0.0, 1.0), -1.0)
            .unwrap(),
        Color::RED,
        true,
    )
    .unwrap();
    let mut scene = Scene::new();
    let family = scene
        .surface_cairo_family_with_paths(
            grid,
            SurfaceOptions {
                material: SemanticSpatialMaterial::CairoSurface,
                ..SurfaceOptions::default()
            },
            vec![cap],
        )
        .unwrap();
    let store = std::rc::Rc::clone(scene.integration_store());
    let leaves = store
        .borrow()
        .ordered_leaf_nodes(family.family().node_id())
        .unwrap();
    assert_eq!(leaves.len(), 2);
    let (cell, cap, cap_geometry) = {
        let borrowed = store.borrow();
        let cell = leaves
            .iter()
            .copied()
            .find(|node| {
                borrowed
                    .semantic_object_state_checked(*node)
                    .unwrap()
                    .surface_uv_cell()
                    .is_some()
            })
            .unwrap();
        let cap = leaves.iter().copied().find(|node| *node != cell).unwrap();
        let cap_state = borrowed.semantic_object_state_checked(cap).unwrap();
        assert_eq!(cap_state.surface_uv_cell(), None);
        assert_eq!(
            cap_state.spatial_material(),
            SemanticSpatialMaterial::CairoPath
        );
        assert_eq!(
            cap_state.cairo_path_appearance(),
            Some(SemanticCairoPathAppearance::default())
        );
        assert_eq!(
            cap_state.spatial_composition_domain(),
            noon_core::SemanticSpatialCompositionDomain::World
        );
        let StoredGeometry::Resource(handle) = cap_state.content.geometry().unwrap() else {
            panic!("cap is a retained path resource");
        };
        let GeometryResource::VectorPath(path) = borrowed.geometry_resources().get(handle).unwrap()
        else {
            panic!("cap resource is a vector path");
        };
        (cell, cap, std::sync::Arc::clone(path))
    };
    let cap_style_before = store
        .borrow()
        .semantic_object_state_checked(cap)
        .unwrap()
        .style
        .clone();

    family
        .set_fill_by_checkerboard([Color::GREEN, Color::YELLOW], 0.25)
        .unwrap();
    let borrowed = store.borrow();
    assert_eq!(
        borrowed.semantic_object_state_checked(cap).unwrap().style,
        cap_style_before
    );
    assert_eq!(
        borrowed
            .semantic_object_state_checked(cell)
            .unwrap()
            .style
            .fill_opacity,
        0.25
    );
    let cap_state = borrowed.semantic_object_state_checked(cap).unwrap();
    let StoredGeometry::Resource(handle) = cap_state.content.geometry().unwrap() else {
        unreachable!()
    };
    let GeometryResource::VectorPath(after) = borrowed.geometry_resources().get(handle).unwrap()
    else {
        unreachable!()
    };
    assert!(std::sync::Arc::ptr_eq(&cap_geometry, after));
}

#[test]
fn malformed_surface_cell_rolls_back_mixed_family_resources_and_nodes() {
    let store = std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
    let before_resources = store.borrow().geometry_resources().len();
    let before_nodes = store.borrow().len();
    assert!(matches!(
        SpatialPathOptions::circle(
            1.0,
            SemanticWorldTransform3D::IDENTITY,
            Color::rgba(1.0, 0.0, 0.0, 0.5),
            false,
        ),
        Err(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::SpatialPathOpacity
        ))
    ));
    let invalid_cell = MeshOptions::new(
        MeshResource::new(
            vec![
                SemanticVec3::ZERO,
                SemanticVec3::new(1.0, 0.0, 0.0),
                SemanticVec3::new(0.0, 1.0, 0.0),
            ],
            None,
            vec![0, 1, 2],
        )
        .unwrap(),
    )
    .with_surface_uv_cell([0, 0]);
    let cap =
        SpatialPathOptions::circle(1.0, SemanticWorldTransform3D::IDENTITY, Color::RED, false)
            .unwrap();
    assert!(
        MobjectFamily::from_meshes_and_paths(store.clone(), vec![invalid_cell], vec![cap]).is_err()
    );
    assert_eq!(store.borrow().geometry_resources().len(), before_resources);
    assert_eq!(store.borrow().len(), before_nodes);

    let valid_mesh = MeshOptions::new(noon_geometry::cube_mesh(1.0).unwrap());
    let invalid_cap = SpatialPathOptions {
        path: noon_core::VectorPath::new().move_to(noon_core::Vec2::new(f32::NAN, 0.0)),
        transform: SemanticWorldTransform3D::IDENTITY,
        style: SemanticStyle::default(),
        material: SemanticSpatialMaterial::Unlit,
        cairo_appearance: None,
    };
    assert!(MobjectFamily::from_meshes_and_paths(
        store.clone(),
        vec![valid_mesh],
        vec![invalid_cap],
    )
    .is_err());
    assert_eq!(store.borrow().geometry_resources().len(), before_resources);
    assert_eq!(store.borrow().len(), before_nodes);
}

#[test]
fn mixed_live_family_rolls_back_valid_body_when_cap_path_preparation_fails() {
    let scene = Scene::new();
    let store = std::rc::Rc::clone(scene.integration_store());
    let mut execution = scene.execution_session().unwrap();
    let before_resources = store.borrow().geometry_resources().len();
    let before_nodes = store.borrow().len();
    let invalid_cap = SpatialPathOptions {
        path: noon_core::VectorPath::new().move_to(noon_core::Vec2::new(f32::NAN, 0.0)),
        transform: SemanticWorldTransform3D::IDENTITY,
        style: SemanticStyle::default(),
        material: SemanticSpatialMaterial::Unlit,
        cairo_appearance: None,
    };

    let mut live = crate::LiveSession::new(&store, scene.root(), &mut execution);
    assert!(live
        .create_mesh_family_with_paths(vec![cube()], vec![invalid_cap])
        .is_err());
    assert_eq!(store.borrow().geometry_resources().len(), before_resources);
    assert_eq!(store.borrow().len(), before_nodes);
}

#[test]
fn cairo_surface_family_publishes_appearance_metadata_with_distinct_material() {
    let cairo_grid = UvSurfacePlan::new([0.0, 1.0], [0.0, 1.0], [1, 1])
        .unwrap()
        .sample_cairo(|u, v| SemanticVec3::new(u, v, u * u + v * v))
        .unwrap();
    let expected = cairo_grid.appearances()[0];
    let mut scene = Scene::new();
    let family = scene
        .surface_cairo_family(
            cairo_grid,
            SurfaceOptions {
                material: SemanticSpatialMaterial::CairoSurface,
                ..SurfaceOptions::default()
            },
        )
        .unwrap();
    let store = std::rc::Rc::clone(scene.integration_store());
    let leaves = store
        .borrow()
        .ordered_leaf_nodes(family.family().node_id())
        .unwrap();
    assert_eq!(leaves.len(), 1);
    let borrowed = store.borrow();
    let state = borrowed.semantic_object_state_checked(leaves[0]).unwrap();
    assert_eq!(
        state.spatial_material(),
        SemanticSpatialMaterial::CairoSurface
    );
    let StoredGeometry::Resource(handle) = state.content.geometry().unwrap() else {
        panic!("Cairo Surface cell should retain a mesh resource");
    };
    let Some(noon_core::GeometryResource::Mesh(mesh)) = borrowed.geometry_resources().get(handle)
    else {
        panic!("Cairo Surface cell should retain a mesh");
    };
    assert_eq!(mesh.cairo_appearance(), Some(&expected));
}

#[test]
fn cairo_spherical_surface_family_accepts_poles_and_recolors_without_replacing_meshes() {
    let plan = UvSurfacePlan::new(
        [0.0, std::f64::consts::TAU],
        [0.0, std::f64::consts::PI],
        [8, 6],
    )
    .unwrap();
    let cairo_grid = plan
        .sample_cairo(|u, v| {
            let (sin_v, cos_v) = v.sin_cos();
            let (sin_u, cos_u) = u.sin_cos();
            SemanticVec3::new(cos_u * sin_v, sin_u * sin_v, -cos_v)
        })
        .unwrap();
    assert!(cairo_grid.grid().normals().contains(&SemanticVec3::ZERO));

    let mut scene = Scene::new();
    let surface = scene
        .surface_cairo_family(
            cairo_grid,
            SurfaceOptions {
                material: SemanticSpatialMaterial::CairoSurface,
                ..SurfaceOptions::default()
            },
        )
        .unwrap();
    let store = std::rc::Rc::clone(scene.integration_store());
    let leaves = store
        .borrow()
        .ordered_leaf_nodes(surface.family().node_id())
        .unwrap();
    assert_eq!(leaves.len(), 8 * 6);

    let retained_handles = {
        let borrowed = store.borrow();
        leaves
            .iter()
            .map(|leaf| {
                let state = borrowed.semantic_object_state_checked(*leaf).unwrap();
                assert_eq!(
                    state.spatial_material(),
                    SemanticSpatialMaterial::CairoSurface
                );
                let StoredGeometry::Resource(handle) = state.content.geometry().unwrap() else {
                    panic!("Cairo pole cell should retain a mesh resource");
                };
                let Some(noon_core::GeometryResource::Mesh(mesh)) =
                    borrowed.geometry_resources().get(handle)
                else {
                    panic!("Cairo pole cell should retain a mesh");
                };
                assert!(mesh
                    .cairo_appearance()
                    .is_some_and(|appearance| appearance.is_finite()));
                handle
            })
            .collect::<Vec<_>>()
    };

    let revision = scene.revision();
    scene
        .set_surface_checkerboard(&surface, [Color::RED, Color::GREEN], 0.4)
        .unwrap();
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    let borrowed = store.borrow();
    for (leaf, retained_handle) in leaves.iter().zip(retained_handles) {
        let state = borrowed.semantic_object_state_checked(*leaf).unwrap();
        assert_eq!(
            state.spatial_material(),
            SemanticSpatialMaterial::CairoSurface
        );
        assert_eq!(state.style.fill_opacity, 0.4);
        let [u, v] = state.surface_uv_cell().unwrap();
        assert_eq!(
            state.style.fill,
            Some(SemanticPaint::Solid(if (u % 2 + v % 2) % 2 == 0 {
                Color::RED
            } else {
                Color::GREEN
            }))
        );
        assert!(matches!(
            state.content.geometry(),
            Some(StoredGeometry::Resource(handle)) if handle == retained_handle
        ));
    }
}

#[test]
fn non_cairo_surface_family_still_rejects_zero_vertex_normals() {
    let positions = vec![
        SemanticVec3::ZERO,
        SemanticVec3::new(1.0, 0.0, 0.0),
        SemanticVec3::new(1.0, 1.0, 0.0),
        SemanticVec3::new(0.0, 1.0, 0.0),
    ];
    let mesh = MeshResource::new(
        positions,
        Some(vec![SemanticVec3::ZERO; 4]),
        vec![0, 1, 3, 1, 2, 3],
    )
    .unwrap();
    let mut scene = Scene::new();
    let object = scene
        .mesh(MeshOptions::new(mesh).with_surface_uv_cell([0, 0]))
        .unwrap();
    let family = scene.family(&[(&object).into()]).unwrap();

    assert!(matches!(
        SurfaceFamily::from_family(family),
        Err(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::SurfaceCellRole
        ))
    ));
}

#[test]
fn surface_family_retains_uv_roles_and_applies_defaults_and_atomic_checkerboard() {
    let mut scene = Scene::new();
    let surface = scene
        .surface_family(flat_grid(), SurfaceOptions::default())
        .unwrap();
    let store = std::rc::Rc::clone(scene.integration_store());
    let leaves = store
        .borrow()
        .ordered_leaf_nodes(surface.family().node_id())
        .unwrap();
    let colors = leaves
        .iter()
        .enumerate()
        .map(|(index, leaf)| {
            let borrowed = store.borrow();
            let state = borrowed.semantic_object_state_checked(*leaf).unwrap();
            assert_eq!(state.surface_uv_cell(), Some([index / 2, index % 2]));
            assert_eq!(state.style.stroke_width, 0.005);
            assert_eq!(
                state.style.stroke_width_mode,
                noon_core::StrokeWidthMode::ScreenSpace
            );
            assert_eq!(state.spatial_material(), SemanticSpatialMaterial::PointLit);
            match state.style.fill.as_ref().unwrap() {
                SemanticPaint::Solid(color) => *color,
                _ => panic!("solid cell fill"),
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        colors,
        [Color::BLUE_D, Color::BLUE_E, Color::BLUE_E, Color::BLUE_D]
    );

    scene
        .set_surface_checkerboard(&surface, [Color::RED, Color::GREEN], 0.4)
        .unwrap();
    let changed = leaves
        .iter()
        .map(|leaf| {
            store
                .borrow()
                .semantic_object_state_checked(*leaf)
                .unwrap()
                .clone()
        })
        .collect::<Vec<_>>();
    assert_eq!(changed[0].style.fill_opacity, 0.4);
    assert_eq!(changed[1].style.fill_opacity, 0.4);
    assert_eq!(
        changed[0].style.fill,
        Some(SemanticPaint::Solid(Color::RED))
    );
    assert_eq!(
        changed[1].style.fill,
        Some(SemanticPaint::Solid(Color::GREEN))
    );
    let before = changed;
    let revision = scene.revision();
    assert!(scene
        .set_surface_checkerboard(&surface, [Color::BLUE, Color::RED], f64::NAN)
        .is_err());
    assert_eq!(scene.revision(), revision);
    for (leaf, state) in leaves.iter().zip(before) {
        assert_eq!(
            store.borrow().semantic_object_state_checked(*leaf).unwrap(),
            &state
        );
    }
}

#[test]
fn surface_family_unlit_is_explicit_and_invalid_members_fail_atomically() {
    let mut scene = Scene::new();
    let surface = scene
        .surface_family(
            flat_grid(),
            SurfaceOptions {
                material: SemanticSpatialMaterial::Unlit,
                ..SurfaceOptions::default()
            },
        )
        .unwrap();
    let leaves = scene
        .integration_store()
        .borrow()
        .ordered_leaf_nodes(surface.family().node_id())
        .unwrap();
    assert!(leaves.iter().all(|leaf| {
        scene
            .integration_store()
            .borrow()
            .semantic_object_state_checked(*leaf)
            .unwrap()
            .spatial_material()
            == SemanticSpatialMaterial::Unlit
    }));
    assert!(SurfaceFamily::from_family(surface.family().clone()).is_ok());

    // The wrapper carries no shadow membership list: later non-cell members
    // remain part of the same semantic family without receiving checkerboard.
    let ordinary = scene.circle(0.25).unwrap();
    surface.family().add((&ordinary).into()).unwrap();
    let ordinary_before = ordinary.state().unwrap();
    scene
        .set_surface_checkerboard(&surface, [Color::RED, Color::GREEN], 0.5)
        .unwrap();
    assert_eq!(ordinary.state().unwrap(), ordinary_before);
    assert!(leaves.iter().all(|leaf| {
        scene
            .integration_store()
            .borrow()
            .semantic_object_state_checked(*leaf)
            .unwrap()
            .style
            .fill_opacity
            == 0.5
    }));

    let impostor = scene.mesh(cube().with_surface_uv_cell([0, 0])).unwrap();
    let impostor_family = scene.family(&[(&impostor).into()]).unwrap();
    assert!(matches!(
        SurfaceFamily::from_family(impostor_family),
        Err(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::SurfaceCellRole
        ))
    ));
}

#[test]
fn translucent_prism_face_family_is_atomic_reusable_and_rust_transformable() {
    let mut scene = Scene::new();
    let revision = scene.revision();
    let resource_stats = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .stats();
    assert!(scene
        .prism_face_family(
            SemanticVec3::new(3.0, 2.0, 1.0),
            Color::BLUE,
            f64::NAN,
            false,
        )
        .is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats(),
        resource_stats
    );

    let cube = scene
        .cube_face_family(2.0, Color::BLUE, 0.75, false)
        .unwrap();
    assert_eq!(
        store_leaf_count(scene.integration_store(), cube.node_id()),
        6
    );

    let tint = Color::rgba(0.2, 0.4, 0.8, 0.6);
    let family = scene
        .prism_face_family(SemanticVec3::new(3.0, 2.0, 1.0), tint, 0.75, false)
        .unwrap();
    let store = std::rc::Rc::clone(scene.integration_store());
    let leaves = store.borrow().ordered_leaf_nodes(family.node_id()).unwrap();
    assert_eq!(leaves.len(), 6);
    let resource_handles = leaves
        .iter()
        .map(|leaf| {
            let borrowed = store.borrow();
            let state = borrowed.semantic_object_state_checked(*leaf).unwrap();
            assert_eq!(state.style.fill, Some(SemanticPaint::Solid(tint)));
            assert_eq!(state.style.fill_opacity, 0.75);
            assert_eq!(state.style.stroke, None);
            assert_eq!(state.spatial_material(), SemanticSpatialMaterial::Unlit);
            let handle = state.content.geometry().unwrap().resource_handle().unwrap();
            let Some(GeometryResource::Mesh(mesh)) = borrowed.geometry_resources().get(handle)
            else {
                panic!("prism face mesh resource")
            };
            assert_eq!(mesh.positions().len(), 4);
            assert_eq!(mesh.normals().unwrap().len(), 4);
            assert_eq!(mesh.indices(), [0, 1, 3, 1, 2, 3]);
            assert!(mesh.cairo_appearance().is_none());
            handle
        })
        .collect::<Vec<_>>();

    let copy = family.copy_family().unwrap();
    let copy_family = copy.root().clone();
    let copied_leaves = store
        .borrow()
        .ordered_leaf_nodes(copy_family.node_id())
        .unwrap();
    assert_eq!(copied_leaves.len(), 6);
    let copied_handles = copied_leaves
        .iter()
        .map(|leaf| {
            store
                .borrow()
                .semantic_object_state_checked(*leaf)
                .unwrap()
                .content
                .geometry()
                .unwrap()
                .resource_handle()
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(copied_handles, resource_handles);
    let resources_after_copy = store.borrow().geometry_resources().stats();

    let mut moved = copy_family.clone();
    moved
        .world_affine(WorldAffineEdit::Shift(SemanticVec3::new(1.0, -2.0, 0.5)))
        .unwrap();
    for leaf in copied_leaves {
        let borrowed = store.borrow();
        let state = borrowed.semantic_object_state_checked(leaf).unwrap();
        let world = state.transform.world_transform().unwrap();
        assert_eq!(world.translation, SemanticVec3::new(1.0, -2.0, 0.5));
        assert_eq!(state.style.fill_opacity, 0.75);
    }
    for leaf in leaves {
        let borrowed = store.borrow();
        let state = borrowed.semantic_object_state_checked(leaf).unwrap();
        assert_eq!(
            state.transform.world_transform().unwrap().translation,
            SemanticVec3::ZERO
        );
    }
    assert_eq!(
        store.borrow().geometry_resources().stats(),
        resources_after_copy
    );
}

#[test]
fn cairo_shaded_prism_uses_six_retained_face_appearances_and_distinct_material() {
    let mut scene = Scene::new();
    let family = scene
        .prism_face_family(SemanticVec3::new(3.0, 2.0, 1.0), Color::BLUE, 0.75, true)
        .unwrap();
    let store = std::rc::Rc::clone(scene.integration_store());
    let leaves = store.borrow().ordered_leaf_nodes(family.node_id()).unwrap();
    assert_eq!(leaves.len(), 6);
    let borrowed = store.borrow();
    for leaf in leaves {
        let state = borrowed.semantic_object_state_checked(leaf).unwrap();
        assert_eq!(
            state.spatial_material(),
            SemanticSpatialMaterial::CairoSurface
        );
        let StoredGeometry::Resource(handle) = state.content.geometry().unwrap() else {
            panic!("Cairo shaded prism face should retain a mesh");
        };
        let Some(GeometryResource::Mesh(mesh)) = borrowed.geometry_resources().get(handle) else {
            panic!("Cairo shaded prism face should retain a mesh resource");
        };
        assert!(mesh.cairo_appearance().is_some());
        assert!(
            mesh.has_usable_normals(),
            "native outward normals remain retained"
        );
    }
}

fn store_leaf_count(
    store: &std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
    family: noon_core::SemanticNodeId,
) -> usize {
    store.borrow().ordered_leaf_nodes(family).unwrap().len()
}

fn assert_vec3_near(actual: SemanticVec3, expected: SemanticVec3) {
    assert!(
        (actual.x - expected.x).abs() < 1e-12,
        "{actual:?} != {expected:?}"
    );
    assert!(
        (actual.y - expected.y).abs() < 1e-12,
        "{actual:?} != {expected:?}"
    );
    assert!(
        (actual.z - expected.z).abs() < 1e-12,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn world_center_uses_transformed_local_bounds_for_offset_meshes() {
    let mut scene = Scene::new();
    let object = scene.mesh(offset_mesh()).unwrap();
    let world = SemanticWorldTransform3D::new(
        SemanticVec3::new(1.0, 2.0, 3.0),
        SemanticRotation3D::from_axis_angle(
            SemanticVec3::new(0.0, 0.0, 1.0),
            std::f64::consts::FRAC_PI_2,
        )
        .unwrap(),
        SemanticVec3::new(2.0, 1.0, 1.0),
    )
    .unwrap();
    scene.set_world_transform(&object, world).unwrap();
    scene.add(&object).unwrap();
    assert_vec3_near(
        object.world_center().unwrap(),
        SemanticVec3::new(0.0, 24.0, 4.0),
    );

    // Effective track sampling must win over the unchanged authored pose.
    let mut endpoint = world;
    endpoint.translation = SemanticVec3::new(-3.0, 5.0, 7.0);
    let animation = scene
        .declare_world_transform(
            &object,
            endpoint,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    let root = transaction.create_animation_composition(
        noon_core::SemanticAnimationCompositionKind::Parallel,
        [animation.node_id()],
        AnimationOptions::new(),
    );
    let root = transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap()
        .resolve(root)
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    session
        .activate_animation_segment(
            &scene.integration_store().borrow(),
            root,
            AnimationOptions::new(),
        )
        .unwrap();
    session.advance_to(1.0).unwrap();
    scene.install_execution(session);
    assert_vec3_near(
        scene.effective_world_center(&object).unwrap(),
        SemanticVec3::new(-2.0, 25.5, 6.0),
    );
    assert_eq!(object.world_transform().unwrap(), world);
}

#[test]
fn world_center_handles_native_cylinder_camera_light_and_detached_live_mesh() {
    let mut scene = Scene::new();
    let camera = scene.camera_3d(camera()).unwrap();
    assert_eq!(
        camera.world_center().unwrap(),
        SemanticVec3::new(0.0, 0.0, 5.0)
    );
    let light = scene
        .point_light_3d(SemanticVec3::new(1.0, -2.0, 3.0), Color::WHITE, 0.5)
        .unwrap();
    assert_eq!(
        light.world_center().unwrap(),
        SemanticVec3::new(1.0, -2.0, 3.0)
    );

    let cylinder = scene
        .mesh(MeshOptions::new(
            noon_geometry::cylinder_mesh(1.0, 4.0, 16).unwrap(),
        ))
        .unwrap();
    assert_eq!(
        cylinder.world_center().unwrap(),
        SemanticVec3::new(0.0, 0.0, 2.0)
    );

    scene.install_execution(scene.execution_session().unwrap());
    let detached = scene.mesh(offset_mesh()).unwrap();
    assert_eq!(
        scene.effective_world_center(&detached).unwrap(),
        SemanticVec3::new(11.0, 1.0, 1.0)
    );
    scene
        .world_affine(
            (&detached).into(),
            WorldAffineEdit::Shift(SemanticVec3::new(2.0, 0.0, -1.0)),
        )
        .unwrap();
    assert_vec3_near(
        scene.effective_world_center(&detached).unwrap(),
        SemanticVec3::new(13.0, 1.0, 0.0),
    );
    scene
        .world_affine(
            (&detached).into(),
            WorldAffineEdit::Rotate {
                axis: SemanticVec3::new(0.0, 0.0, 1.0),
                radians: std::f64::consts::FRAC_PI_2,
                about: None,
            },
        )
        .unwrap();
    assert_vec3_near(
        scene.effective_world_center(&detached).unwrap(),
        SemanticVec3::new(13.0, 1.0, 0.0),
    );
}

#[test]
fn world_center_rejects_foreign_and_stale_handles() {
    let mut first = Scene::new();
    let second = Scene::new();
    let object = first.mesh(offset_mesh()).unwrap();
    assert!(matches!(
        second.effective_world_center(&object),
        Err(AuthoringError::ForeignStore)
    ));
    let stale = object.clone();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.remove_node(object.node_id());
    transaction
        .apply(&mut first.integration_store().borrow_mut())
        .unwrap();
    assert!(stale.world_center().is_err());

    let mut running = Scene::new();
    let detached = running.mesh(offset_mesh()).unwrap();
    running.install_execution(running.execution_session().unwrap());
    let mut external = SemanticMutationTransaction::new();
    external.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 0.5 },
    )));
    external
        .apply(&mut running.integration_store().borrow_mut())
        .unwrap();
    assert!(running.effective_world_center(&detached).is_err());
}

#[test]
fn family_world_center_uses_authored_and_effective_leaf_bounds() {
    let mut scene = Scene::new();
    let first = scene.mesh(offset_mesh()).unwrap();
    let mut second_transform = SemanticWorldTransform3D::IDENTITY;
    second_transform.translation = SemanticVec3::new(20.0, 0.0, 0.0);
    let second = scene
        .mesh(
            MeshOptions::new(noon_geometry::cube_mesh(1.0).unwrap())
                .with_transform(second_transform),
        )
        .unwrap();
    let detached_member = scene.mesh(offset_mesh()).unwrap();
    let store = std::rc::Rc::clone(scene.integration_store());
    let family = MobjectFamily::create(store, &[(&first).into(), (&second).into()]).unwrap();
    let detached_family = MobjectFamily::create(
        std::rc::Rc::clone(scene.integration_store()),
        &[(&detached_member).into()],
    )
    .unwrap();
    assert_eq!(
        family.world_center().unwrap(),
        SemanticVec3::new(15.25, 0.75, 0.75)
    );
    assert_eq!(
        detached_family.world_center().unwrap(),
        SemanticVec3::new(11.0, 1.0, 1.0)
    );
    scene.add_many(&[(&family).into()]).unwrap();

    let mut target = first.world_transform().unwrap();
    target.translation.x += 3.0;
    let declaration = scene
        .declare_world_transform(
            &first,
            target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    let root = transaction.create_animation_composition(
        noon_core::SemanticAnimationCompositionKind::Parallel,
        [declaration.node_id()],
        AnimationOptions::new(),
    );
    let root = transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap()
        .resolve(root)
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    session
        .activate_animation_segment(
            &scene.integration_store().borrow(),
            root,
            AnimationOptions::new(),
        )
        .unwrap();
    session.advance_to(1.0).unwrap();
    scene.install_execution(session);
    assert_eq!(
        scene.effective_world_family_center(&family).unwrap(),
        SemanticVec3::new(16.0, 0.75, 0.75)
    );
    assert_eq!(
        scene
            .effective_world_family_center(&detached_family)
            .unwrap(),
        SemanticVec3::new(11.0, 1.0, 1.0)
    );

    let mut external = SemanticMutationTransaction::new();
    external.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 0.5 },
    )));
    external
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    assert!(scene.effective_world_family_center(&family).is_err());
}

#[test]
fn mesh_creation_motion_and_residency_use_ordinary_scene_publication() {
    let mut scene = Scene::new();
    scene.camera_3d(camera()).unwrap();
    let object = scene.mesh(cube()).unwrap();
    scene.add(&object).unwrap();
    let state = object.state().unwrap();
    let handle = state.content.geometry().unwrap().resource_handle().unwrap();
    let initial_resource = match scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .get(handle)
        .unwrap()
    {
        GeometryResource::Mesh(mesh) => Arc::clone(mesh),
        _ => panic!("mesh resource"),
    };
    let mut target = SemanticWorldTransform3D::IDENTITY;
    target.translation = SemanticVec3::new(2.0, -1.0, 3.0);
    target.rotation =
        SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 1.0, 0.0), 0.8).unwrap();
    let animation = scene
        .declare_world_transform(
            &object,
            target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    let root = transaction.create_animation_composition(
        noon_core::SemanticAnimationCompositionKind::Parallel,
        [animation.node_id()],
        AnimationOptions::new(),
    );
    let root = transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap()
        .resolve(root)
        .unwrap();
    let animation = DeclaredAnimation::new(Rc::clone(scene.integration_store()), root);
    let revision = scene.revision();
    let activate = || {
        let mut session = scene.execution_session().unwrap();
        session
            .activate_animation_segment(
                &scene.integration_store().borrow(),
                animation.node_id(),
                AnimationOptions::new(),
            )
            .unwrap();
        session
    };
    let mut forward = activate();
    let mut seek = activate();
    for time in [0.0, 0.5, 1.5, 2.0] {
        forward.advance_to(time).unwrap();
        seek.seek(time).unwrap();
        assert_eq!(forward.frame(), seek.frame());
        assert_eq!(scene.revision(), revision);
        assert_eq!(object.state().unwrap(), state);
    }
    assert_eq!(
        forward
            .frame()
            .objects
            .iter()
            .find(|row| row.camera_projection().is_none())
            .unwrap()
            .world_transform()
            .unwrap(),
        target
    );
    let resource = scene.integration_store().borrow();
    let GeometryResource::Mesh(retained) = resource.geometry_resources().get(handle).unwrap()
    else {
        panic!("mesh resource")
    };
    assert!(Arc::ptr_eq(&initial_resource, retained));
}

#[test]
fn invalid_face_batch_rolls_back_all_new_resources_and_nodes() {
    let mut scene = Scene::new();
    scene.camera_3d(camera()).unwrap();
    let existing = scene.mesh(cube()).unwrap();
    let before = existing.state().unwrap();
    let revision = scene.revision();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .stats();
    let mut invalid = cube();
    invalid.transform.translation.x = f64::NAN;
    assert!(scene.mesh_family(vec![cube(), invalid]).is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(existing.state().unwrap(), before);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats(),
        resources
    );
}

#[test]
fn live_mesh_creation_and_local_world_edit_preserve_existing_slots() {
    let mut scene = Scene::new();
    scene.camera_3d(camera()).unwrap();
    let object = scene.mesh(cube()).unwrap();
    scene.add(&object).unwrap();
    scene.install_execution(scene.execution_session().unwrap());
    let original_id = scene
        .owned_execution()
        .execution_object_id(object.node_id())
        .unwrap();
    let initial_pose =
        SemanticWorldTransform3D::from_axial_direction(SemanticVec3::new(1.0, 2.0, 3.0), -1.0)
            .unwrap();
    let detached = scene
        .mesh(
            MeshOptions::new(noon_geometry::cylinder_mesh(0.3, 2.0, 16).unwrap())
                .with_transform(initial_pose),
        )
        .unwrap();
    assert_eq!(scene.owned_execution().frame().objects.len(), 2);
    scene.add(&detached).unwrap();
    assert_eq!(scene.owned_execution().frame().objects.len(), 3);
    assert_eq!(detached.world_transform().unwrap(), initial_pose);
    let state = detached.state().unwrap();
    let handle = state.content.geometry().unwrap().resource_handle().unwrap();
    let retained = match scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .get(handle)
        .unwrap()
    {
        GeometryResource::Mesh(mesh) => Arc::clone(mesh),
        _ => panic!("cylinder mesh"),
    };
    let resources_before = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .stats();
    let mut target =
        SemanticWorldTransform3D::from_axial_direction(SemanticVec3::new(-1.0, 2.0, -3.0), -1.0)
            .unwrap();
    target.translation.z = -2.0;
    scene.set_world_transform(&detached, target).unwrap();
    assert_eq!(detached.world_transform().unwrap(), target);
    let borrowed = scene.integration_store().borrow();
    assert_eq!(borrowed.geometry_resources().stats(), resources_before);
    let GeometryResource::Mesh(after) = borrowed.geometry_resources().get(handle).unwrap() else {
        panic!("cylinder mesh")
    };
    assert!(Arc::ptr_eq(&retained, after));
    assert_eq!(
        scene
            .owned_execution()
            .execution_object_id(object.node_id())
            .unwrap(),
        original_id
    );
    assert_eq!(
        object.world_transform().unwrap(),
        SemanticWorldTransform3D::IDENTITY
    );
}

#[test]
fn camera_initialization_and_light_validation_are_atomic() {
    let mut scene = Scene::new();
    let revision = scene.revision();
    let mut invalid = camera();
    invalid.position.x = f64::NAN;
    assert!(scene.camera_3d(invalid).is_err());
    assert_eq!(scene.revision(), revision);
    let camera = scene.camera_3d(camera()).unwrap();
    assert!(scene.camera_3d(super::tests::camera()).is_err());
    let revision = scene.revision();
    assert!(scene
        .point_light_3d(SemanticVec3::ZERO, Color::WHITE, f64::NAN)
        .is_err());
    assert_eq!(scene.revision(), revision);
    let light = scene
        .point_light_3d(SemanticVec3::new(4.0, -3.0, 6.0), Color::WHITE, 0.5)
        .unwrap();
    scene.add(&light).unwrap();
    let execution = scene.execution_session().unwrap();
    assert_eq!(camera.state().unwrap().role(), SemanticObjectRole::Camera3D);
    assert_eq!(
        light.state().unwrap().role(),
        SemanticObjectRole::PointLight3D
    );
    assert_eq!(
        execution
            .frame()
            .objects
            .iter()
            .filter(|row| row
                .spatial
                .as_ref()
                .is_some_and(|spatial| spatial.point_light))
            .count(),
        1
    );
}

#[test]
fn unsupported_mesh_fade_is_rejected_before_activation_but_light_opacity_remains_live() {
    use crate::{FadeEndpoint, FadeTranslation};
    use noon_core::SemanticFadeDirection;

    for initial_opacity in [1.0, 0.5] {
        let mut scene = Scene::new();
        let mut options = cube();
        options.style.fill_opacity = initial_opacity;
        let mesh = scene.mesh(options).unwrap();
        scene.add(&mesh).unwrap();
        let state_before = mesh.state().unwrap();
        let revision_before = scene.revision();
        let handle = state_before
            .content
            .geometry()
            .unwrap()
            .resource_handle()
            .unwrap();
        let resources_before = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats();
        let mut execution = scene.execution_session().unwrap();

        let activation = {
            let mut live = scene.live(&mut execution);
            live.declare_and_activate_fade_with_endpoint(
                &mesh,
                SemanticFadeDirection::Out,
                FadeEndpoint::new(0.8, FadeTranslation::Shift(SemanticVec3::ZERO)),
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
        };
        assert!(activation.is_err(), "mesh fade must fail at activation");
        assert_eq!(scene.revision(), revision_before);
        assert_eq!(mesh.state().unwrap(), state_before);
        assert_eq!(
            execution_world_transform(&execution, mesh.node_id()),
            SemanticWorldTransform3D::IDENTITY
        );
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .stats(),
            resources_before
        );
        assert!(scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .get(handle)
            .is_some());
    }

    let mut scene = Scene::new();
    let light = scene
        .point_light_3d(SemanticVec3::new(1.0, 2.0, 3.0), Color::WHITE, 0.5)
        .unwrap();
    scene.add(&light).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    let track = transaction.create_object_property_track(
        light.node_id(),
        noon_core::SemanticObjectTrackProperty::Opacity,
        noon_core::SemanticObjectTrackValues::Scalar { from: 0.5, to: 1.0 },
        noon_core::TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        noon_core::CompositionTimeMap::identity(),
    );
    let applied = transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let track = applied.resolve(track).unwrap();
    let animation = scene
        .declare_animation(
            noon_core::SemanticAnimationIntent::Composition {
                kind: noon_core::SemanticAnimationCompositionKind::Parallel,
                children: vec![track],
            },
            AnimationOptions::new(),
        )
        .unwrap();
    let mut execution = scene
        .execution_session_with_animation_root(&animation)
        .unwrap();
    execution.seek(1.0).unwrap();
    let light_row = execution
        .frame()
        .objects
        .iter()
        .find(|row| row.spatial.as_ref().is_some_and(|state| state.point_light))
        .unwrap();
    assert!(light_row.spatial.as_ref().unwrap().point_light);
    assert_eq!(light_row.style.opacity, 0.75);
}

#[test]
fn live_world_compositions_capture_effective_pose_and_complete_authored_endpoint() {
    let mut scene = Scene::new();
    let camera = scene.camera_3d(camera()).unwrap();
    let mesh = scene.mesh(cube()).unwrap();
    scene.add(&mesh).unwrap();
    let authored_mesh = mesh.state().unwrap();
    let mut execution = scene.execution_session().unwrap();
    let first = SemanticWorldTransform3D::new(
        SemanticVec3::new(2.0, 0.0, 0.0),
        SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 1.0, 0.0), 0.6).unwrap(),
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .unwrap();
    let second = SemanticWorldTransform3D::new(
        SemanticVec3::new(4.0, 1.0, -2.0),
        SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 1.0, 0.0), 1.2).unwrap(),
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .unwrap();
    let options = AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear);
    let sequential = crate::AnimationCompositionRequest::Composition {
        kind: noon_core::SemanticAnimationCompositionKind::Sequence,
        children: vec![
            crate::AnimationCompositionRequest::WorldTransform {
                target: &mesh,
                transform: first,
                options,
            },
            crate::AnimationCompositionRequest::WorldTransform {
                target: &mesh,
                transform: second,
                options,
            },
        ],
        options: AnimationOptions::new(),
    };
    let segment;
    {
        let mut live =
            crate::LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
        segment = live
            .declare_and_activate_composition(&sequential, AnimationOptions::new())
            .unwrap();
        live.advance_segment_to(segment, 0.5).unwrap();
    }
    assert_world_near(
        execution_world_transform(&execution, mesh.node_id()),
        SemanticWorldTransform3D::IDENTITY
            .interpolate(first, 0.5)
            .unwrap(),
    );
    {
        let mut live =
            crate::LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
        live.advance_segment_to(segment, 1.5).unwrap();
    }
    assert_world_near(
        execution_world_transform(&execution, mesh.node_id()),
        first.interpolate(second, 0.5).unwrap(),
    );
    assert_eq!(mesh.state().unwrap(), authored_mesh);
    {
        let mut live =
            crate::LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
    }
    assert_eq!(mesh.world_transform().unwrap(), second);

    let camera_target = SemanticWorldTransform3D::new(
        SemanticVec3::new(0.5, -0.25, 5.0),
        SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 0.0, 1.0), 0.2).unwrap(),
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .unwrap();
    let mesh_target = SemanticWorldTransform3D::new(
        SemanticVec3::new(-1.0, 0.25, 1.5),
        SemanticRotation3D::from_axis_angle(SemanticVec3::new(1.0, 0.0, 0.0), 0.3).unwrap(),
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .unwrap();
    let camera_start = camera.world_transform().unwrap();
    let parallel = crate::AnimationCompositionRequest::Composition {
        kind: noon_core::SemanticAnimationCompositionKind::Parallel,
        children: vec![
            crate::AnimationCompositionRequest::WorldTransform {
                target: &camera,
                transform: camera_target,
                options,
            },
            crate::AnimationCompositionRequest::WorldTransform {
                target: &mesh,
                transform: mesh_target,
                options,
            },
        ],
        options: AnimationOptions::new(),
    };
    let revision = scene.revision();
    let bad_camera = SemanticWorldTransform3D::new(
        camera_target.translation,
        camera_target.rotation,
        SemanticVec3::new(2.0, 1.0, 1.0),
    )
    .unwrap();
    let frame_before_invalid = execution.frame().clone();
    {
        let mut live =
            crate::LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
        let invalid = crate::AnimationCompositionRequest::WorldTransform {
            target: &camera,
            transform: bad_camera,
            options,
        };
        assert!(live
            .declare_and_activate_composition(&invalid, AnimationOptions::new())
            .is_err());
        assert_eq!(scene.revision(), revision);
    }
    assert_eq!(*execution.frame(), frame_before_invalid);
    let segment;
    {
        let mut live =
            crate::LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
        segment = live
            .declare_and_activate_composition(&parallel, AnimationOptions::new())
            .unwrap();
        live.advance_segment_to(segment, segment.start_time() + 0.5)
            .unwrap();
    }
    assert_world_near(
        execution_world_transform(&execution, camera.node_id()),
        camera_start.interpolate(camera_target, 0.5).unwrap(),
    );
    assert_world_near(
        execution_world_transform(&execution, mesh.node_id()),
        second.interpolate(mesh_target, 0.5).unwrap(),
    );
    {
        let mut live =
            crate::LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
    }
    assert_eq!(camera.world_transform().unwrap(), camera_target);
    assert_eq!(mesh.world_transform().unwrap(), mesh_target);
}

fn execution_world_transform(
    execution: &crate::ExecutionSession,
    object: noon_core::SemanticNodeId,
) -> SemanticWorldTransform3D {
    let id = execution.execution_object_id(object).unwrap();
    execution
        .frame()
        .objects
        .iter()
        .find(|row| row.id == id)
        .unwrap()
        .world_transform()
        .unwrap()
}

fn assert_world_near(actual: SemanticWorldTransform3D, expected: SemanticWorldTransform3D) {
    for (actual, expected) in [
        (actual.translation.x, expected.translation.x),
        (actual.translation.y, expected.translation.y),
        (actual.translation.z, expected.translation.z),
        (actual.scale.x, expected.scale.x),
        (actual.scale.y, expected.scale.y),
        (actual.scale.z, expected.scale.z),
    ] {
        assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
    }
    for (actual, expected) in actual
        .rotation
        .components()
        .into_iter()
        .zip(expected.rotation.components())
    {
        assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
    }
}
