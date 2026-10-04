//! Shared authored world-space affine edits for spatial semantic objects.

use crate::{AuthoringError, MobjectTarget, UnsupportedAuthoringOperation};
use noon_core::{
    GeometryResource, SemanticMutationTransaction, SemanticNodeId, SemanticObjectRole,
    SemanticObjectState, SemanticOrientation, SemanticStore, SemanticTransform, SemanticVec3,
    SemanticWorldTransform3D, StoredGeometry,
};
use std::{cell::RefCell, rc::Rc};

/// One high-precision world-space affine family operation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WorldAffineEdit {
    Shift(SemanticVec3),
    Scale {
        factor: f64,
        about: Option<SemanticVec3>,
    },
    Rotate {
        axis: SemanticVec3,
        radians: f64,
        about: Option<SemanticVec3>,
    },
}

struct LeafPose {
    node: SemanticNodeId,
    state: SemanticObjectState,
    world: SemanticWorldTransform3D,
}

/// Prepare an authored affine edit using each leaf's authored world pose.
pub(crate) fn prepare_world_affine(
    store_rc: &Rc<RefCell<SemanticStore>>,
    target: MobjectTarget<'_>,
    edit: WorldAffineEdit,
) -> Result<SemanticMutationTransaction, AuthoringError> {
    prepare_world_affine_with(store_rc, target, edit, |store, node| {
        store
            .semantic_object_state_checked(node)
            .map_err(AuthoringError::from)?
            .transform
            .world_transform()
            .ok_or(AuthoringError::NonFiniteObjectState)
    })
}

/// Prepare one transaction from a coherent set of authored/effective world poses.
/// The supplied closure is called once per unique affected semantic leaf.
pub(crate) fn prepare_world_affine_with<F>(
    store_rc: &Rc<RefCell<SemanticStore>>,
    target: MobjectTarget<'_>,
    edit: WorldAffineEdit,
    mut world_for: F,
) -> Result<SemanticMutationTransaction, AuthoringError>
where
    F: FnMut(&SemanticStore, SemanticNodeId) -> Result<SemanticWorldTransform3D, AuthoringError>,
{
    validate_edit(edit)?;
    let root = target.require_store(store_rc)?;
    let store = store_rc.borrow();
    let leaves = store
        .ordered_leaf_nodes(root)
        .map_err(AuthoringError::from)?;
    if leaves.is_empty() {
        return Err(AuthoringError::FamilyPairing(
            noon_core::SemanticFamilyPairingError::Empty,
        ));
    }

    let mut poses = Vec::with_capacity(leaves.len());
    for node in leaves {
        let state = store
            .semantic_object_state_checked(node)
            .map_err(AuthoringError::from)?
            .clone();
        if !state.spatial_declaration_is_valid() {
            return Err(AuthoringError::NonFiniteObjectState);
        }
        validate_spatial_leaf(&store, &state)?;
        let world = world_for(&store, node)?;
        if !valid_world(world) {
            return Err(AuthoringError::NonFiniteObjectState);
        }
        poses.push(LeafPose { node, state, world });
    }

    let pivot = match edit {
        WorldAffineEdit::Shift(_) => None,
        WorldAffineEdit::Scale { about, .. } | WorldAffineEdit::Rotate { about, .. } => {
            Some(match about {
                Some(point) => point,
                None => union_world_bounds_center(&store, &poses)?,
            })
        }
    };

    // Build the complete candidate set before constructing any transaction edits.
    // A bad final leaf therefore cannot expose a partial write set.
    let mut candidates = Vec::with_capacity(poses.len());
    for pose in poses {
        let world = apply_edit(pose.world, edit, pivot)?;
        if matches!(
            pose.state.role(),
            SemanticObjectRole::Camera3D | SemanticObjectRole::PointLight3D
        ) && world.scale != SemanticVec3::new(1.0, 1.0, 1.0)
        {
            return Err(AuthoringError::NonFiniteObjectState);
        }
        let mut candidate_state = pose.state;
        candidate_state.set_transform(semantic_transform(world));
        if !candidate_state.spatial_declaration_is_valid() {
            return Err(AuthoringError::NonFiniteObjectState);
        }
        candidates.push((pose.node, world));
    }

    let mut transaction = SemanticMutationTransaction::new();
    for (node, world) in candidates {
        transaction.set_object_transform(node, semantic_transform(world));
    }
    Ok(transaction)
}

fn validate_edit(edit: WorldAffineEdit) -> Result<(), AuthoringError> {
    match edit {
        WorldAffineEdit::Shift(delta) if delta.is_finite() => Ok(()),
        WorldAffineEdit::Scale { factor, about }
            if factor.is_finite() && about.is_none_or(SemanticVec3::is_finite) =>
        {
            Ok(())
        }
        WorldAffineEdit::Rotate {
            axis,
            radians,
            about,
        } if axis.is_finite()
            && radians.is_finite()
            && about.is_none_or(SemanticVec3::is_finite)
            && noon_core::SemanticRotation3D::from_axis_angle(axis, radians).is_some() =>
        {
            Ok(())
        }
        _ => Err(AuthoringError::NonFiniteObjectState),
    }
}

fn valid_world(world: SemanticWorldTransform3D) -> bool {
    world.translation.is_finite() && world.scale.is_finite() && world.rotation.is_valid()
}

fn validate_spatial_leaf(
    store: &SemanticStore,
    state: &SemanticObjectState,
) -> Result<(), AuthoringError> {
    match state.role() {
        SemanticObjectRole::Camera3D | SemanticObjectRole::PointLight3D => Ok(()),
        SemanticObjectRole::Ordinary => match state.content {
            noon_core::SemanticObjectContent::Image(_) => Err(unsupported_spatial_path()),
            noon_core::SemanticObjectContent::Geometry(StoredGeometry::Resource(handle)) => store
                .geometry_resources()
                .get(handle)
                .map(|_| ())
                .ok_or(AuthoringError::MissingGeometryResource(handle)),
            noon_core::SemanticObjectContent::Geometry(_) => Ok(()),
            noon_core::SemanticObjectContent::Text(handle) => store
                .text_resources()
                .get(handle)
                .map(|_| ())
                .ok_or(AuthoringError::MissingTextResource(handle)),
        },
        _ => Err(unsupported_spatial_path()),
    }
}

fn unsupported_spatial_path() -> AuthoringError {
    AuthoringError::Unsupported(UnsupportedAuthoringOperation::WorldAffineContent)
}

fn union_world_bounds_center(
    store: &SemanticStore,
    poses: &[LeafPose],
) -> Result<SemanticVec3, AuthoringError> {
    let mut min = SemanticVec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut max = SemanticVec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for pose in poses {
        let corners = match pose.state.role() {
            SemanticObjectRole::Camera3D | SemanticObjectRole::PointLight3D => {
                [pose.world.translation; 8]
            }
            SemanticObjectRole::Ordinary => {
                let (local_min, local_max) = match pose.state.content.geometry() {
                    Some(StoredGeometry::Resource(handle))
                        if matches!(
                            store.geometry_resources().get(handle),
                            Some(GeometryResource::Mesh(_))
                        ) =>
                    {
                        let Some(GeometryResource::Mesh(mesh)) =
                            store.geometry_resources().get(handle)
                        else {
                            unreachable!()
                        };
                        let bounds = mesh.bounds();
                        (bounds.min, bounds.max)
                    }
                    _ => {
                        let bounds = crate::semantic_mobject::boundary_for_content(
                            store,
                            pose.state.content,
                            noon_core::SemanticTransform2_5D {
                                translation: SemanticVec3::ZERO,
                                scale: SemanticVec3::new(1.0, 1.0, 1.0),
                                rotation_z: 0.0,
                            },
                        )?
                        .ok_or(AuthoringError::NonFiniteGeometry)?;
                        (
                            SemanticVec3::new(bounds.min_x, bounds.min_y, 0.0),
                            SemanticVec3::new(bounds.max_x, bounds.max_y, 0.0),
                        )
                    }
                };
                let mut corners = [SemanticVec3::ZERO; 8];
                for (index, corner) in corners.iter_mut().enumerate() {
                    let local = SemanticVec3::new(
                        if index & 1 == 0 {
                            local_min.x
                        } else {
                            local_max.x
                        },
                        if index & 2 == 0 {
                            local_min.y
                        } else {
                            local_max.y
                        },
                        if index & 4 == 0 {
                            local_min.z
                        } else {
                            local_max.z
                        },
                    );
                    *corner = pose
                        .world
                        .transform_point(local)
                        .ok_or(AuthoringError::NonFiniteObjectState)?;
                }
                corners
            }
            _ => return Err(unsupported_spatial_path()),
        };
        for point in corners {
            if !point.is_finite() {
                return Err(AuthoringError::NonFiniteObjectState);
            }
            min.x = min.x.min(point.x);
            min.y = min.y.min(point.y);
            min.z = min.z.min(point.z);
            max.x = max.x.max(point.x);
            max.y = max.y.max(point.y);
            max.z = max.z.max(point.z);
        }
    }
    let center = SemanticVec3::new(
        midpoint(min.x, max.x),
        midpoint(min.y, max.y),
        midpoint(min.z, max.z),
    );
    center
        .is_finite()
        .then_some(center)
        .ok_or(AuthoringError::NonFiniteObjectState)
}

/// Compute the center of one object's world-space axis-aligned bounds using
/// the same local bounds and eight-corner transform used by default pivots.
pub(crate) fn world_bounds_center(
    store: &SemanticStore,
    node: SemanticNodeId,
    state: &SemanticObjectState,
    world: SemanticWorldTransform3D,
) -> Result<SemanticVec3, AuthoringError> {
    if !state.spatial_declaration_is_valid() || !valid_world(world) {
        return Err(AuthoringError::NonFiniteObjectState);
    }
    validate_spatial_leaf(store, state)?;
    union_world_bounds_center(
        store,
        &[LeafPose {
            node,
            state: state.clone(),
            world,
        }],
    )
}

fn midpoint(a: f64, b: f64) -> f64 {
    a * 0.5 + b * 0.5
}

fn apply_edit(
    mut world: SemanticWorldTransform3D,
    edit: WorldAffineEdit,
    pivot: Option<SemanticVec3>,
) -> Result<SemanticWorldTransform3D, AuthoringError> {
    match edit {
        WorldAffineEdit::Shift(delta) => {
            world.translation = add(world.translation, delta);
        }
        WorldAffineEdit::Scale { factor, .. } => {
            let about = pivot.ok_or(AuthoringError::NonFiniteObjectState)?;
            world.translation = add(about, scale(subtract(world.translation, about), factor));
            world.scale = scale(world.scale, factor);
        }
        WorldAffineEdit::Rotate { axis, radians, .. } => {
            let about = pivot.ok_or(AuthoringError::NonFiniteObjectState)?;
            let rotation = noon_core::SemanticRotation3D::from_axis_angle(axis, radians)
                .ok_or(AuthoringError::NonFiniteObjectState)?;
            world.rotation = rotation
                .compose(world.rotation)
                .ok_or(AuthoringError::NonFiniteObjectState)?;
            let offset = rotation
                .rotate_vector(subtract(world.translation, about))
                .ok_or(AuthoringError::NonFiniteObjectState)?;
            world.translation = add(about, offset);
        }
    }
    valid_world(world)
        .then_some(world)
        .ok_or(AuthoringError::NonFiniteObjectState)
}

fn semantic_transform(world: SemanticWorldTransform3D) -> SemanticTransform {
    SemanticTransform {
        translation: world.translation,
        scale: world.scale,
        orientation: SemanticOrientation::Spatial(world.rotation),
    }
}

fn add(a: SemanticVec3, b: SemanticVec3) -> SemanticVec3 {
    SemanticVec3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}

fn subtract(a: SemanticVec3, b: SemanticVec3) -> SemanticVec3 {
    SemanticVec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

fn scale(value: SemanticVec3, factor: f64) -> SemanticVec3 {
    SemanticVec3::new(value.x * factor, value.y * factor, value.z * factor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Mobject, MobjectFamily};
    use noon_core::{MeshResource, SemanticProjection3D, SemanticRotation3D};

    fn store() -> Rc<RefCell<SemanticStore>> {
        Rc::new(RefCell::new(SemanticStore::new()))
    }

    fn box_mesh(min: SemanticVec3, max: SemanticVec3) -> MeshResource {
        let positions = (0..8)
            .map(|index| {
                SemanticVec3::new(
                    if index & 1 == 0 { min.x } else { max.x },
                    if index & 2 == 0 { min.y } else { max.y },
                    if index & 4 == 0 { min.z } else { max.z },
                )
            })
            .collect();
        MeshResource::new(positions, None, vec![0, 1, 3]).unwrap()
    }

    fn mesh_object(
        store: &Rc<RefCell<SemanticStore>>,
        min: SemanticVec3,
        max: SemanticVec3,
        world: SemanticWorldTransform3D,
    ) -> Mobject {
        let handle = store.borrow_mut().insert_geometry_mesh(box_mesh(min, max));
        let mut state = SemanticObjectState::new(StoredGeometry::Resource(handle));
        state.transform = semantic_transform(world);
        let id = store.borrow_mut().insert_semantic_object(state);
        Mobject::from_node(Rc::clone(store), id).unwrap()
    }

    fn world(translation: SemanticVec3, scale: SemanticVec3) -> SemanticWorldTransform3D {
        SemanticWorldTransform3D::new(translation, SemanticRotation3D::IDENTITY, scale).unwrap()
    }

    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1.0e-9, "{a} != {b}");
    }

    #[test]
    fn rotation_pre_multiplies_and_uses_transformed_mesh_bounds_center() {
        let store = store();
        let object = mesh_object(
            &store,
            SemanticVec3::new(0.0, 0.0, 0.0),
            SemanticVec3::new(2.0, 1.0, 2.0),
            world(SemanticVec3::ZERO, SemanticVec3::new(1.0, 1.0, 1.0)),
        );
        let before = store.borrow().scene_revision();
        let transaction = prepare_world_affine(
            &store,
            (&object).into(),
            WorldAffineEdit::Rotate {
                axis: SemanticVec3::new(0.0, 0.0, 1.0),
                radians: std::f64::consts::FRAC_PI_2,
                about: None,
            },
        )
        .unwrap();
        assert_eq!(
            store.borrow().scene_revision(),
            before,
            "preparation is inert"
        );
        transaction.apply(&mut store.borrow_mut()).unwrap();
        let state = store
            .borrow()
            .semantic_object_state_checked(object.node_id())
            .unwrap()
            .clone();
        near(state.transform.translation.x, 1.5);
        near(state.transform.translation.y, -0.5);
        near(state.transform.translation.z, 0.0);
        assert_eq!(state.transform.scale, SemanticVec3::new(1.0, 1.0, 1.0));
        let actual = state
            .transform
            .world_transform()
            .unwrap()
            .rotation
            .rotate_vector(SemanticVec3::new(1.0, 0.0, 0.0))
            .unwrap();
        near(actual.x, 0.0);
        near(actual.y, 1.0);
        near(actual.z, 0.0);
    }

    #[test]
    fn uniform_scale_uses_world_aabb_center_and_keeps_orientation() {
        let store = store();
        let rotation =
            SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 0.0, 1.0), 0.25).unwrap();
        let object = mesh_object(
            &store,
            SemanticVec3::ZERO,
            SemanticVec3::new(2.0, 1.0, 2.0),
            SemanticWorldTransform3D::new(
                SemanticVec3::new(5.0, 2.0, 1.0),
                rotation,
                SemanticVec3::new(2.0, 1.0, 3.0),
            )
            .unwrap(),
        );
        let transaction = prepare_world_affine(
            &store,
            (&object).into(),
            WorldAffineEdit::Scale {
                factor: 2.0,
                about: None,
            },
        )
        .unwrap();
        transaction.apply(&mut store.borrow_mut()).unwrap();
        let transform = store
            .borrow()
            .semantic_object_state_checked(object.node_id())
            .unwrap()
            .transform
            .world_transform()
            .unwrap();
        assert_eq!(transform.rotation, rotation);
        assert_eq!(transform.scale, SemanticVec3::new(4.0, 2.0, 6.0));
        // World AABB center is not the object's translation or mesh centroid.
        let center_x = 5.0 + (4.0 * 0.25_f64.cos() - 0.25_f64.sin()) * 0.5;
        let center_y = 2.0 + (4.0 * 0.25_f64.sin() + 0.25_f64.cos()) * 0.5;
        near(transform.translation.x, 10.0 - center_x);
        near(transform.translation.y, 4.0 - center_y);
        near(transform.translation.z, -2.0);
    }

    #[test]
    fn aliased_family_leaves_are_edited_once_and_bad_late_leaf_is_atomic() {
        let store = store();
        let mesh = mesh_object(
            &store,
            SemanticVec3::ZERO,
            SemanticVec3::new(1.0, 1.0, 1.0),
            world(
                SemanticVec3::new(3.0, 0.0, 0.0),
                SemanticVec3::new(1.0, 1.0, 1.0),
            ),
        );
        let left = MobjectFamily::create(Rc::clone(&store), &[(&mesh).into()]).unwrap();
        let right = MobjectFamily::create(Rc::clone(&store), &[(&mesh).into()]).unwrap();
        let aliases =
            MobjectFamily::create(Rc::clone(&store), &[(&left).into(), (&right).into()]).unwrap();
        let tx = prepare_world_affine(
            &store,
            (&aliases).into(),
            WorldAffineEdit::Shift(SemanticVec3::new(2.0, 0.0, 0.0)),
        )
        .unwrap();
        tx.apply(&mut store.borrow_mut()).unwrap();
        let moved = store
            .borrow()
            .semantic_object_state_checked(mesh.node_id())
            .unwrap()
            .transform
            .translation;
        assert_eq!(moved.x, 5.0);

        // Planar paths are now supported in the spatial lane. Raster images
        // remain an unsupported late leaf and must reject the entire edit.
        let image_id = store
            .borrow_mut()
            .with_raster_image_rgba8(1, 1, vec![255u8; 4], |store, handle| {
                let mut image =
                    SemanticObjectState::new(noon_core::SemanticImageContent::new(handle));
                image.transform.translation.x = 10.0;
                Ok::<_, noon_core::RasterImageResourceError>(store.insert_semantic_object(image))
            })
            .unwrap();
        let image = Mobject::from_node(Rc::clone(&store), image_id).unwrap();
        let mixed =
            MobjectFamily::create(Rc::clone(&store), &[(&mesh).into(), (&image).into()]).unwrap();
        let before_revision = store.borrow().scene_revision();
        let before_mesh = store
            .borrow()
            .semantic_object_state_checked(mesh.node_id())
            .unwrap()
            .transform;
        assert!(matches!(
            prepare_world_affine(
                &store,
                (&mixed).into(),
                WorldAffineEdit::Shift(SemanticVec3::new(1.0, 0.0, 0.0))
            ),
            Err(AuthoringError::Unsupported(_))
        ));
        assert_eq!(store.borrow().scene_revision(), before_revision);
        assert_eq!(
            store
                .borrow()
                .semantic_object_state_checked(mesh.node_id())
                .unwrap()
                .transform,
            before_mesh
        );
    }

    #[test]
    fn rejects_foreign_stale_and_non_unit_camera_scale_targets() {
        let first = store();
        let second = store();
        let foreign = mesh_object(
            &second,
            SemanticVec3::ZERO,
            SemanticVec3::new(1.0, 1.0, 1.0),
            world(SemanticVec3::ZERO, SemanticVec3::new(1.0, 1.0, 1.0)),
        );
        assert!(matches!(
            prepare_world_affine(
                &first,
                (&foreign).into(),
                WorldAffineEdit::Shift(SemanticVec3::ZERO)
            ),
            Err(AuthoringError::ForeignStore)
        ));

        let stale = mesh_object(
            &first,
            SemanticVec3::ZERO,
            SemanticVec3::new(1.0, 1.0, 1.0),
            world(SemanticVec3::ZERO, SemanticVec3::new(1.0, 1.0, 1.0)),
        );
        let mut remove = SemanticMutationTransaction::new();
        remove.remove_node(stale.node_id());
        remove.apply(&mut first.borrow_mut()).unwrap();
        assert!(prepare_world_affine(
            &first,
            (&stale).into(),
            WorldAffineEdit::Shift(SemanticVec3::ZERO)
        )
        .is_err());

        let mut camera_state = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.0 });
        camera_state.set_role(SemanticObjectRole::Camera3D);
        camera_state
            .set_camera_projection(Some(SemanticProjection3D::Perspective {
                vertical_fov_radians: 1.0,
                near: 0.1,
                far: 10.0,
            }))
            .unwrap();
        let camera = SemanticWorldTransform3D::new(
            SemanticVec3::ZERO,
            SemanticRotation3D::IDENTITY,
            SemanticVec3::new(1.0, 1.0, 1.0),
        )
        .unwrap();
        camera_state.transform = semantic_transform(camera);
        let camera_id = first.borrow_mut().insert_semantic_object(camera_state);
        let camera = Mobject::from_node(Rc::clone(&first), camera_id).unwrap();
        assert!(matches!(
            prepare_world_affine(
                &first,
                (&camera).into(),
                WorldAffineEdit::Scale {
                    factor: 0.0,
                    about: Some(SemanticVec3::ZERO),
                }
            ),
            Err(AuthoringError::NonFiniteObjectState)
        ));

        let mesh = mesh_object(
            &first,
            SemanticVec3::ZERO,
            SemanticVec3::new(1.0, 1.0, 1.0),
            world(SemanticVec3::ZERO, SemanticVec3::new(1.0, 1.0, 1.0)),
        );
        prepare_world_affine(
            &first,
            (&mesh).into(),
            WorldAffineEdit::Scale {
                factor: 0.0,
                about: Some(SemanticVec3::ZERO),
            },
        )
        .unwrap()
        .apply(&mut first.borrow_mut())
        .unwrap();
        assert_eq!(
            first
                .borrow()
                .semantic_object_state_checked(mesh.node_id())
                .unwrap()
                .transform
                .scale,
            SemanticVec3::ZERO
        );
    }

    #[test]
    fn effective_pose_provider_is_called_once_per_unique_leaf() {
        let store = store();
        let object = mesh_object(
            &store,
            SemanticVec3::ZERO,
            SemanticVec3::new(1.0, 1.0, 1.0),
            world(SemanticVec3::ZERO, SemanticVec3::new(1.0, 1.0, 1.0)),
        );
        let mut calls = 0;
        let transaction = prepare_world_affine_with(
            &store,
            (&object).into(),
            WorldAffineEdit::Shift(SemanticVec3::new(1.0, 0.0, 0.0)),
            |_store, _node| {
                calls += 1;
                Ok(world(
                    SemanticVec3::new(4.0, 0.0, 0.0),
                    SemanticVec3::new(1.0, 1.0, 1.0),
                ))
            },
        )
        .unwrap();
        assert_eq!(calls, 1);
        transaction.apply(&mut store.borrow_mut()).unwrap();
        let translation = store
            .borrow()
            .semantic_object_state_checked(object.node_id())
            .unwrap()
            .transform
            .translation;
        assert_eq!(translation.x, 5.0);
    }
}
