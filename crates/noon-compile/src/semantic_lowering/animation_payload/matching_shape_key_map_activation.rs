use noon_core::{
    matching_shape_correspondence_with_key_map, MatchingShapeCorrespondence, MatchingShapeKey,
    ObjectId, SemanticNodeId, SemanticStore,
};

use super::{
    prepare_matching_shape_activation_correspondence, EffectiveAnimationProperties,
    PreparedMatchingShapeActivationError, PreparedMatchingShapeActivationProjection,
    PreparedSemanticAnimationTrack,
};
use super::super::SemanticExecutionIndex;

/// Activation-time matching-shape state paired with correspondence after applying an
/// ordered explicit key map.
///
/// Geometry/content/affine capture remains owned entirely by
/// `prepare_matching_shape_activation_correspondence`; this wrapper only repartitions
/// the captured deterministic keys. A later public adapter can therefore add `key_map`
/// without creating a second start-state or geometry model.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedMatchingShapeKeyMapActivationProjection {
    activation: PreparedMatchingShapeActivationProjection,
    correspondence: MatchingShapeCorrespondence,
}

impl PreparedMatchingShapeKeyMapActivationProjection {
    pub const fn activation(&self) -> &PreparedMatchingShapeActivationProjection {
        &self.activation
    }

    pub const fn correspondence(&self) -> &MatchingShapeCorrespondence {
        &self.correspondence
    }
}

/// Capture one matching-shape activation and then apply Manim-compatible ordered
/// key redirection to the keys visible at that exact activation boundary.
pub fn prepare_matching_shape_activation_correspondence_with_key_map<F>(
    store: &SemanticStore,
    index: &SemanticExecutionIndex,
    source_root: SemanticNodeId,
    target_root: SemanticNodeId,
    activation_start: f64,
    prior_tracks: &[PreparedSemanticAnimationTrack],
    key_map: &[(MatchingShapeKey, MatchingShapeKey)],
    effective_properties: F,
) -> Result<PreparedMatchingShapeKeyMapActivationProjection, PreparedMatchingShapeActivationError>
where
    F: FnMut(ObjectId) -> Option<EffectiveAnimationProperties>,
{
    let activation = prepare_matching_shape_activation_correspondence(
        store,
        index,
        source_root,
        target_root,
        activation_start,
        prior_tracks,
        effective_properties,
    )?;
    let source_keys = activation
        .source_members()
        .iter()
        .map(|member| member.key.clone())
        .collect::<Vec<_>>();
    let target_keys = activation
        .target_members()
        .iter()
        .map(|member| member.key.clone())
        .collect::<Vec<_>>();
    let correspondence =
        matching_shape_correspondence_with_key_map(&source_keys, &target_keys, key_map);
    Ok(PreparedMatchingShapeKeyMapActivationProjection {
        activation,
        correspondence,
    })
}

#[cfg(test)]
mod tests {
    use noon_core::{SemanticObjectState, Style, StoredGeometry, Transform2D, Vec2, VectorPath};

    use super::*;

    fn path(store: &mut SemanticStore, points: [Vec2; 3]) -> SemanticNodeId {
        let path = VectorPath::new()
            .move_to(points[0])
            .line_to(points[1])
            .line_to(points[2])
            .close();
        let handle = store.insert_geometry_path(path).unwrap();
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Resource(handle)))
    }

    fn family(store: &mut SemanticStore, members: &[SemanticNodeId]) -> SemanticNodeId {
        let family = store.insert_family();
        for &member in members {
            store.add_member(family, member).unwrap();
        }
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

    #[test]
    fn key_map_repartitions_activation_time_keys_without_recapturing_state() {
        let mut store = SemanticStore::new();
        let a = path(
            &mut store,
            [Vec2::new(-1.0, -1.0), Vec2::new(1.0, -0.5), Vec2::new(-0.25, 1.0)],
        );
        let b = path(
            &mut store,
            [Vec2::new(0.0, -1.0), Vec2::new(1.5, 0.0), Vec2::new(0.0, 1.0)],
        );
        let source = family(&mut store, &[a, b]);
        store.attach_to_scene(source).unwrap();

        let target_a = path(
            &mut store,
            [Vec2::new(-1.0, -1.0), Vec2::new(1.0, -0.5), Vec2::new(-0.25, 1.0)],
        );
        let target_b = path(
            &mut store,
            [Vec2::new(0.0, -1.0), Vec2::new(1.5, 0.0), Vec2::new(0.0, 1.0)],
        );
        let target = family(&mut store, &[target_a, target_b]);

        let mut index = SemanticExecutionIndex::new();
        index.lower_scene(&store).unwrap();
        let baseline = prepare_matching_shape_activation_correspondence(
            &store,
            &index,
            source,
            target,
            0.0,
            &[],
            |_| Some(effective()),
        )
        .unwrap();
        let key_a = baseline.source_members()[0].key.clone();
        let key_b = baseline.source_members()[1].key.clone();

        let mapped = prepare_matching_shape_activation_correspondence_with_key_map(
            &store,
            &index,
            source,
            target,
            0.0,
            &[],
            &[(key_a, key_b)],
            |_| Some(effective()),
        )
        .unwrap();

        assert_eq!(mapped.activation().source_members(), baseline.source_members());
        assert_eq!(mapped.activation().target_members(), baseline.target_members());
        assert_eq!(mapped.correspondence().matched_groups.len(), 2);
        assert_eq!(mapped.correspondence().mapped_groups.len(), 1);
        assert_eq!(mapped.correspondence().mapped_groups[0].source_indices, vec![0]);
        assert_eq!(mapped.correspondence().mapped_groups[0].target_indices, vec![1]);
        assert_eq!(mapped.correspondence().unmatched_source_indices, vec![1]);
        assert_eq!(mapped.correspondence().unmatched_target_indices, vec![0]);
    }
}
