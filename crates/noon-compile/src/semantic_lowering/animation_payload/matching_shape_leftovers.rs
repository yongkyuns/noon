use noon_core::{
    vector_path_matching_shape_bounds, GeometryRef, MatchingShapeBounds, MatchingShapeKeyError,
    SemanticNodeId, SemanticObjectContent, SemanticStore, Transform2D, Vec2,
};

use super::matching_shape_activation::PreparedMatchingShapeActivationProjection;

/// Activation-time spatial relationship between unmatched source and target groups.
///
/// Manim's default TransformMatchingShapes leftovers are one FadeOut over the source
/// rest and one FadeIn over the target rest. Their `target_position` values therefore
/// use the two VGroup centers, not arbitrary source/target member pairs. Empty groups
/// use the origin, matching the empty-Mobject center convention.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedMatchingShapeLeftoverLayout {
    pub source_bounds: Option<MatchingShapeBounds>,
    pub target_bounds: Option<MatchingShapeBounds>,
    pub source_center: Vec2,
    pub target_center: Vec2,
    /// Translation applied to every source leftover's faded endpoint.
    pub source_to_target: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreparedMatchingShapeLeftoverLayoutError {
    InvalidSourceIndex(usize),
    InvalidTargetIndex(usize),
    UnsupportedGeometry {
        node: SemanticNodeId,
    },
    Bounds {
        node: SemanticNodeId,
        error: MatchingShapeKeyError,
    },
}

impl std::fmt::Display for PreparedMatchingShapeLeftoverLayoutError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSourceIndex(index) => {
                write!(
                    formatter,
                    "matching-shape source leftover index {index} is out of bounds"
                )
            }
            Self::InvalidTargetIndex(index) => {
                write!(
                    formatter,
                    "matching-shape target leftover index {index} is out of bounds"
                )
            }
            Self::UnsupportedGeometry { node } => write!(
                formatter,
                "matching-shape leftover {}:{} is not a resolved vector path",
                node.slot(),
                node.generation()
            ),
            Self::Bounds { node, error } => write!(
                formatter,
                "matching-shape leftover bounds failed for {}:{}: {error}",
                node.slot(),
                node.generation()
            ),
        }
    }
}

impl std::error::Error for PreparedMatchingShapeLeftoverLayoutError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bounds { error, .. } => Some(error),
            _ => None,
        }
    }
}

/// Derive the two unmatched-group bounds visible at this exact matching activation.
///
/// Source members use #1579's activation-effective content and transform; target
/// members use their authored detached content and transform. The returned shared
/// delta is the one displacement Manim applies to every source-rest member during
/// FadeOut. Target-rest FadeIn uses the inverse delta.
pub fn prepare_matching_shape_leftover_layout(
    store: &SemanticStore,
    projection: &PreparedMatchingShapeActivationProjection,
) -> Result<PreparedMatchingShapeLeftoverLayout, PreparedMatchingShapeLeftoverLayoutError> {
    let correspondence = projection.correspondence();
    let mut source_bounds = None;
    for &index in &correspondence.unmatched_source_indices {
        let member = projection
            .source_members()
            .get(index)
            .ok_or(PreparedMatchingShapeLeftoverLayoutError::InvalidSourceIndex(index))?;
        include_bounds(
            &mut source_bounds,
            member_bounds(
                store,
                member.node,
                member.content,
                member.effective.transform,
            )?,
        );
    }

    let mut target_bounds = None;
    for &index in &correspondence.unmatched_target_indices {
        let member = projection
            .target_members()
            .get(index)
            .ok_or(PreparedMatchingShapeLeftoverLayoutError::InvalidTargetIndex(index))?;
        include_bounds(
            &mut target_bounds,
            member_bounds(store, member.node, member.content, member.transform)?,
        );
    }

    Ok(leftover_layout_from_bounds(source_bounds, target_bounds))
}

fn member_bounds(
    store: &SemanticStore,
    node: SemanticNodeId,
    content: SemanticObjectContent,
    transform: Transform2D,
) -> Result<MatchingShapeBounds, PreparedMatchingShapeLeftoverLayoutError> {
    let geometry = content
        .geometry()
        .ok_or(PreparedMatchingShapeLeftoverLayoutError::UnsupportedGeometry { node })?;
    let geometry =
        super::super::compiled_scene::lower_semantic_geometry_value(geometry, Some(store))
            .map_err(|_| PreparedMatchingShapeLeftoverLayoutError::UnsupportedGeometry { node })?;
    let GeometryRef::VectorPath(path) = geometry else {
        return Err(PreparedMatchingShapeLeftoverLayoutError::UnsupportedGeometry { node });
    };
    vector_path_matching_shape_bounds(&path, transform)
        .map_err(|error| PreparedMatchingShapeLeftoverLayoutError::Bounds { node, error })
}

fn include_bounds(total: &mut Option<MatchingShapeBounds>, next: MatchingShapeBounds) {
    *total = Some(total.map_or(next, |current| current.union(next)));
}

fn leftover_layout_from_bounds(
    source_bounds: Option<MatchingShapeBounds>,
    target_bounds: Option<MatchingShapeBounds>,
) -> PreparedMatchingShapeLeftoverLayout {
    let source_center = source_bounds.map_or(Vec2::ZERO, MatchingShapeBounds::center);
    let target_center = target_bounds.map_or(Vec2::ZERO, MatchingShapeBounds::center);
    PreparedMatchingShapeLeftoverLayout {
        source_bounds,
        target_bounds,
        source_center,
        target_center,
        source_to_target: target_center - source_center,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_delta_uses_union_centers_not_member_pairing() {
        let source = MatchingShapeBounds::new(Vec2::new(-6.0, -2.0), Vec2::new(-2.0, 2.0));
        let target = MatchingShapeBounds::new(Vec2::new(5.0, -1.0), Vec2::new(9.0, 3.0));
        let layout = leftover_layout_from_bounds(Some(source), Some(target));
        assert_eq!(layout.source_center, Vec2::new(-4.0, 0.0));
        assert_eq!(layout.target_center, Vec2::new(7.0, 1.0));
        assert_eq!(layout.source_to_target, Vec2::new(11.0, 1.0));
    }

    #[test]
    fn empty_leftover_group_uses_origin_center() {
        let target = MatchingShapeBounds::new(Vec2::new(3.0, 1.0), Vec2::new(5.0, 5.0));
        let layout = leftover_layout_from_bounds(None, Some(target));
        assert_eq!(layout.source_center, Vec2::ZERO);
        assert_eq!(layout.target_center, Vec2::new(4.0, 3.0));
        assert_eq!(layout.source_to_target, Vec2::new(4.0, 3.0));
    }
}
