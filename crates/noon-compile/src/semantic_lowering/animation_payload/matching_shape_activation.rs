use noon_core::{
    matching_shape_correspondence, vector_path_matching_shape_key, GeometryRef,
    MatchingShapeCorrespondence, MatchingShapeKey, MatchingShapeKeyError, ObjectId, SemanticNodeId,
    SemanticObjectContent, SemanticStore, Transform2D,
};

use super::super::{
    compiled_scene::lower_semantic_geometry_value,
    projection::{lower_semantic_transform_value, SemanticExecutionIndex},
};
use super::affine::EffectiveAnimationProperties;
use super::prepared_composition::PreparedSemanticAnimationTrack;
use super::scheduled_captures::{completed_content_before, completed_effective_properties_before};

/// One source leaf as it exists at a matching-shape animation's activation boundary.
///
/// Content and affine/style values are compiler-derived endpoint state. They are not
/// written back into the Semantic Scene and carry no renderer identity.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedMatchingShapeSourceMember {
    pub node: SemanticNodeId,
    pub execution_object_id: ObjectId,
    pub content: SemanticObjectContent,
    pub effective: EffectiveAnimationProperties,
    pub key: MatchingShapeKey,
}

/// One authored target leaf used by activation-time matching.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedMatchingShapeTargetMember {
    pub node: SemanticNodeId,
    pub content: SemanticObjectContent,
    pub transform: Transform2D,
    pub key: MatchingShapeKey,
}

/// Deterministic activation-time matching projection below any frontend adapter.
///
/// `correspondence` indexes the two member arrays. Equal-key duplicates therefore
/// remain grouped, while unmatched leaves stay explicit for the later fade/mismatch
/// lowering slice.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedMatchingShapeActivationProjection {
    source_members: Vec<PreparedMatchingShapeSourceMember>,
    target_members: Vec<PreparedMatchingShapeTargetMember>,
    correspondence: MatchingShapeCorrespondence,
}

impl PreparedMatchingShapeActivationProjection {
    pub fn source_members(&self) -> &[PreparedMatchingShapeSourceMember] {
        &self.source_members
    }

    pub fn target_members(&self) -> &[PreparedMatchingShapeTargetMember] {
        &self.target_members
    }

    pub const fn correspondence(&self) -> &MatchingShapeCorrespondence {
        &self.correspondence
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreparedMatchingShapeActivationError {
    InvalidActivationStart(f64),
    FamilyLeaves {
        root: SemanticNodeId,
    },
    EmptyFamily {
        root: SemanticNodeId,
    },
    ObjectState {
        node: SemanticNodeId,
    },
    MissingExecutionSource {
        node: SemanticNodeId,
    },
    MissingEffectiveSource {
        node: SemanticNodeId,
        execution_object_id: ObjectId,
    },
    SequentialStateUnavailable {
        node: SemanticNodeId,
        execution_object_id: ObjectId,
    },
    InvalidTargetTransform {
        node: SemanticNodeId,
    },
    UnsupportedGeometry {
        node: SemanticNodeId,
    },
    ShapeKey {
        node: SemanticNodeId,
        error: MatchingShapeKeyError,
    },
}

impl std::fmt::Display for PreparedMatchingShapeActivationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidActivationStart(value) => {
                write!(formatter, "matching-shape activation start must be finite, got {value}")
            }
            Self::FamilyLeaves { root } => write!(
                formatter,
                "matching-shape family {}:{} cannot derive ordered leaves",
                root.slot(),
                root.generation()
            ),
            Self::EmptyFamily { root } => write!(
                formatter,
                "matching-shape family {}:{} has no ordinary leaves",
                root.slot(),
                root.generation()
            ),
            Self::ObjectState { node } => write!(
                formatter,
                "matching-shape leaf {}:{} has no valid semantic object state",
                node.slot(),
                node.generation()
            ),
            Self::MissingExecutionSource { node } => write!(
                formatter,
                "matching-shape source {}:{} has no stable execution identity",
                node.slot(),
                node.generation()
            ),
            Self::MissingEffectiveSource {
                node,
                execution_object_id,
            } => write!(
                formatter,
                "matching-shape source {}:{} / object {} has no activation-effective state",
                node.slot(),
                node.generation(),
                execution_object_id.get()
            ),
            Self::SequentialStateUnavailable {
                node,
                execution_object_id,
            } => write!(
                formatter,
                "matching-shape source {}:{} / object {} has an overlapping or opaque prior content/affine driver",
                node.slot(),
                node.generation(),
                execution_object_id.get()
            ),
            Self::InvalidTargetTransform { node } => write!(
                formatter,
                "matching-shape target {}:{} has an invalid 2D transform",
                node.slot(),
                node.generation()
            ),
            Self::UnsupportedGeometry { node } => write!(
                formatter,
                "matching-shape leaf {}:{} is not a resolved vector path",
                node.slot(),
                node.generation()
            ),
            Self::ShapeKey { node, error } => write!(
                formatter,
                "matching-shape key failed for leaf {}:{}: {error}",
                node.slot(),
                node.generation()
            ),
        }
    }
}

impl std::error::Error for PreparedMatchingShapeActivationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ShapeKey { error, .. } => Some(error),
            _ => None,
        }
    }
}

/// Build deterministic matching-shape correspondence from the state visible when
/// one scheduled family animation actually begins.
///
/// Source content and affine values consume completed earlier prepared tracks. A
/// prior content morph therefore contributes its semantic completion endpoint rather
/// than its request-construction geometry or a sampled renderer resource. Targets
/// remain authored detached state. Unsupported/overlapping states fail closed.
pub fn prepare_matching_shape_activation_correspondence<F>(
    store: &SemanticStore,
    index: &SemanticExecutionIndex,
    source_root: SemanticNodeId,
    target_root: SemanticNodeId,
    activation_start: f64,
    prior_tracks: &[PreparedSemanticAnimationTrack],
    mut effective_properties: F,
) -> Result<PreparedMatchingShapeActivationProjection, PreparedMatchingShapeActivationError>
where
    F: FnMut(ObjectId) -> Option<EffectiveAnimationProperties>,
{
    if !activation_start.is_finite() {
        return Err(PreparedMatchingShapeActivationError::InvalidActivationStart(activation_start));
    }

    let source_nodes = ordered_nonempty_leaves(store, source_root)?;
    let target_nodes = ordered_nonempty_leaves(store, target_root)?;

    let mut source_members = Vec::with_capacity(source_nodes.len());
    let mut source_keys = Vec::with_capacity(source_nodes.len());
    for node in source_nodes {
        let state = store
            .semantic_object_state_checked(node)
            .map_err(|_| PreparedMatchingShapeActivationError::ObjectState { node })?;
        let execution_object_id = index
            .execution_object_id(node)
            .ok_or(PreparedMatchingShapeActivationError::MissingExecutionSource { node })?;
        let base = effective_properties(execution_object_id).ok_or(
            PreparedMatchingShapeActivationError::MissingEffectiveSource {
                node,
                execution_object_id,
            },
        )?;
        let effective = completed_effective_properties_before(
            base,
            execution_object_id,
            activation_start,
            prior_tracks,
        )
        .ok_or(
            PreparedMatchingShapeActivationError::SequentialStateUnavailable {
                node,
                execution_object_id,
            },
        )?;
        let content = completed_content_before(
            state.content,
            execution_object_id,
            activation_start,
            prior_tracks,
        )
        .ok_or(
            PreparedMatchingShapeActivationError::SequentialStateUnavailable {
                node,
                execution_object_id,
            },
        )?;
        let key = matching_key(store, node, content, effective.transform)?;
        source_keys.push(key.clone());
        source_members.push(PreparedMatchingShapeSourceMember {
            node,
            execution_object_id,
            content,
            effective,
            key,
        });
    }

    let mut target_members = Vec::with_capacity(target_nodes.len());
    let mut target_keys = Vec::with_capacity(target_nodes.len());
    for node in target_nodes {
        let state = store
            .semantic_object_state_checked(node)
            .map_err(|_| PreparedMatchingShapeActivationError::ObjectState { node })?;
        let transform = lower_semantic_transform_value(state)
            .map_err(|_| PreparedMatchingShapeActivationError::InvalidTargetTransform { node })?;
        let key = matching_key(store, node, state.content, transform)?;
        target_keys.push(key.clone());
        target_members.push(PreparedMatchingShapeTargetMember {
            node,
            content: state.content,
            transform,
            key,
        });
    }

    Ok(PreparedMatchingShapeActivationProjection {
        source_members,
        target_members,
        correspondence: matching_shape_correspondence(&source_keys, &target_keys),
    })
}

fn ordered_nonempty_leaves(
    store: &SemanticStore,
    root: SemanticNodeId,
) -> Result<Vec<SemanticNodeId>, PreparedMatchingShapeActivationError> {
    let leaves = store
        .ordered_leaf_nodes(root)
        .map_err(|_| PreparedMatchingShapeActivationError::FamilyLeaves { root })?;
    if leaves.is_empty() {
        return Err(PreparedMatchingShapeActivationError::EmptyFamily { root });
    }
    Ok(leaves)
}

fn matching_key(
    store: &SemanticStore,
    node: SemanticNodeId,
    content: SemanticObjectContent,
    transform: Transform2D,
) -> Result<MatchingShapeKey, PreparedMatchingShapeActivationError> {
    let geometry = content
        .geometry()
        .ok_or(PreparedMatchingShapeActivationError::UnsupportedGeometry { node })?;
    let geometry = lower_semantic_geometry_value(geometry, Some(store))
        .map_err(|_| PreparedMatchingShapeActivationError::UnsupportedGeometry { node })?;
    let GeometryRef::VectorPath(path) = geometry else {
        return Err(PreparedMatchingShapeActivationError::UnsupportedGeometry { node });
    };
    vector_path_matching_shape_key(&path, transform)
        .map_err(|error| PreparedMatchingShapeActivationError::ShapeKey { node, error })
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{
        CompositionTimeMap, Property, RateFunction, SemanticTransactionNodeRef, Style, TrackTiming,
        TrackValues, Vec2, VectorPath,
    };

    use super::super::affine::SemanticAnimationCompletion;

    fn triangle() -> VectorPath {
        VectorPath::new()
            .move_to(Vec2::new(-1.0, -1.0))
            .line_to(Vec2::new(1.0, -0.5))
            .line_to(Vec2::new(-0.25, 1.0))
            .close()
    }

    fn kite() -> VectorPath {
        VectorPath::new()
            .move_to(Vec2::new(0.0, -1.0))
            .line_to(Vec2::new(1.5, 0.0))
            .line_to(Vec2::new(0.0, 1.0))
            .line_to(Vec2::new(-0.5, 0.0))
            .close()
    }

    fn path_object(store: &mut SemanticStore, path: VectorPath) -> SemanticNodeId {
        path_object_with_rotation(store, path, 0.0)
    }

    fn path_object_with_rotation(
        store: &mut SemanticStore,
        path: VectorPath,
        rotation_z: f64,
    ) -> SemanticNodeId {
        let handle = store.insert_geometry_path(path).unwrap();
        let mut state =
            noon_core::SemanticObjectState::new(noon_core::StoredGeometry::Resource(handle));
        state.transform.rotation_z = rotation_z;
        store.insert_semantic_object(state)
    }

    fn family(store: &mut SemanticStore, member: SemanticNodeId) -> SemanticNodeId {
        let family = store.insert_family();
        store.add_member(family, member).unwrap();
        family
    }

    fn effective() -> EffectiveAnimationProperties {
        EffectiveAnimationProperties {
            z_index: 0.0,
            transform: Transform2D::IDENTITY,
            style: Style::default(),
            appearance: 1.0,
            reveal: 1.0,
        }
    }

    fn indexed_source(
        source_path: VectorPath,
        target_path: VectorPath,
    ) -> (
        SemanticStore,
        SemanticExecutionIndex,
        SemanticNodeId,
        SemanticNodeId,
        SemanticNodeId,
        SemanticNodeId,
    ) {
        let mut store = SemanticStore::new();
        let source_leaf = path_object(&mut store, source_path);
        let target_leaf = path_object(&mut store, target_path);
        let source = family(&mut store, source_leaf);
        let target = family(&mut store, target_leaf);
        store.attach_to_scene(source).unwrap();
        let mut index = SemanticExecutionIndex::new();
        index.lower_scene(&store).unwrap();
        (store, index, source, target, source_leaf, target_leaf)
    }

    #[test]
    fn completed_prior_morph_supplies_activation_content_key() {
        let source_path = triangle();
        let target_path = kite();
        let (store, index, source, target, source_leaf, target_leaf) =
            indexed_source(source_path.clone(), target_path.clone());
        let object = index.execution_object_id(source_leaf).unwrap();
        let target_content = store
            .semantic_object_state_checked(target_leaf)
            .unwrap()
            .content;
        let track = PreparedSemanticAnimationTrack {
            animation: SemanticTransactionNodeRef::Existing(source_leaf),
            target: SemanticTransactionNodeRef::Existing(source_leaf),
            execution_object_id: object,
            property: Property::Morph,
            completion: SemanticAnimationCompletion::ContentMorph {
                content: target_content,
            },
            values: TrackValues::PreparedMorph {
                from: 0.0,
                to: 1.0,
                geometry: GeometryRef::path(source_path.with_morph_target(target_path)),
                render_transform: None,
            },
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        };

        let projection = prepare_matching_shape_activation_correspondence(
            &store,
            &index,
            source,
            target,
            1.0,
            std::slice::from_ref(&track),
            |_| Some(effective()),
        )
        .unwrap();
        assert_eq!(projection.correspondence().matched_groups.len(), 1);
        assert!(projection
            .correspondence()
            .unmatched_source_indices
            .is_empty());
        assert!(projection
            .correspondence()
            .unmatched_target_indices
            .is_empty());
        assert_eq!(projection.source_members()[0].content, target_content);

        assert!(matches!(
            prepare_matching_shape_activation_correspondence(
                &store,
                &index,
                source,
                target,
                0.5,
                &[track],
                |_| Some(effective()),
            ),
            Err(PreparedMatchingShapeActivationError::SequentialStateUnavailable { .. })
        ));
    }

    #[test]
    fn completed_prior_rotation_supplies_activation_affine_key() {
        let mut store = SemanticStore::new();
        let source_leaf = path_object(&mut store, triangle());
        let target_leaf =
            path_object_with_rotation(&mut store, triangle(), std::f64::consts::FRAC_PI_2);
        let source = family(&mut store, source_leaf);
        let target = family(&mut store, target_leaf);
        store.attach_to_scene(source).unwrap();
        let mut index = SemanticExecutionIndex::new();
        index.lower_scene(&store).unwrap();
        let object = index.execution_object_id(source_leaf).unwrap();
        let track = PreparedSemanticAnimationTrack {
            animation: SemanticTransactionNodeRef::Existing(source_leaf),
            target: SemanticTransactionNodeRef::Existing(source_leaf),
            execution_object_id: object,
            property: Property::Rotation,
            completion: SemanticAnimationCompletion::Release,
            values: TrackValues::Scalar {
                from: 0.0,
                to: std::f32::consts::FRAC_PI_2,
            },
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        };

        let without_prior = prepare_matching_shape_activation_correspondence(
            &store,
            &index,
            source,
            target,
            1.0,
            &[],
            |_| Some(effective()),
        )
        .unwrap();
        assert!(without_prior.correspondence().matched_groups.is_empty());

        let with_prior = prepare_matching_shape_activation_correspondence(
            &store,
            &index,
            source,
            target,
            1.0,
            &[track],
            |_| Some(effective()),
        )
        .unwrap();
        assert_eq!(with_prior.correspondence().matched_groups.len(), 1);
        assert_eq!(
            with_prior.source_members()[0].effective.transform.rotation,
            std::f32::consts::FRAC_PI_2
        );
    }
}
