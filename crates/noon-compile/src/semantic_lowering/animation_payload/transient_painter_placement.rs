use noon_core::ObjectId;

use super::family_transform_channels::PreparedDerivedFamilyTransformOccurrence;
use super::matching_shape_leftover_fades::PreparedMatchingShapeTargetTransientBase;

/// Compiler-owned painter placement for one identity-free transient presentation.
///
/// Placement is deliberately distinct from semantic/execution identity. Existing
/// family-Transform padding copies stay immediately after the stable source row they
/// visually derive from. Detached authored presentations have no valid source row and
/// instead join the end of their authored z layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreparedTransientPainterPlacement {
    AfterStable {
        anchor_execution_object_id: ObjectId,
    },
    LayerEnd,
}

impl PreparedDerivedFamilyTransformOccurrence {
    /// Painter placement for a repeated-source family occurrence.
    ///
    /// The stable source row is only a painter anchor and activation-state donor; it
    /// never becomes the derived occurrence's semantic identity.
    pub const fn painter_placement(&self) -> PreparedTransientPainterPlacement {
        PreparedTransientPainterPlacement::AfterStable {
            anchor_execution_object_id: self.anchor_execution_object_id,
        }
    }
}

impl PreparedMatchingShapeTargetTransientBase {
    /// Painter placement for a detached authored target leftover.
    ///
    /// No stable source is invented. The runtime/renderer follow-up must merge this
    /// transient at the end of `self.z_index`'s painter layer while preserving the
    /// occurrence order supplied by matching-shape staging.
    pub const fn painter_placement(&self) -> PreparedTransientPainterPlacement {
        PreparedTransientPainterPlacement::LayerEnd
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, ObjectContentRef, SemanticNodeId, Style, Transform2D};

    use super::*;
    use crate::semantic_lowering::animation_payload::EffectiveAnimationProperties;

    #[test]
    fn repeated_source_occurrence_stays_after_its_stable_anchor() {
        let anchor = ObjectId::new(17);
        let occurrence = PreparedDerivedFamilyTransformOccurrence {
            occurrence_index: 3,
            anchor_execution_object_id: anchor,
            source: SemanticNodeId::new(1, 0),
            target_state: SemanticNodeId::new(2, 0),
            effective_source: EffectiveAnimationProperties {
                z_index: 0.0,
                transform: Transform2D::IDENTITY,
                style: Style::default(),
                appearance: 1.0,
                reveal: 1.0,
            },
            tracks: Vec::new(),
        };
        assert_eq!(
            occurrence.painter_placement(),
            PreparedTransientPainterPlacement::AfterStable {
                anchor_execution_object_id: anchor,
            }
        );
    }

    #[test]
    fn detached_target_leftover_uses_layer_end_without_fake_anchor() {
        let base = PreparedMatchingShapeTargetTransientBase {
            node: SemanticNodeId::new(4, 0),
            z_index: 12.0,
            content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
            transform: Transform2D::IDENTITY,
            style: Style::default(),
            appearance: 1.0,
            reveal: 1.0,
            morph: 0.0,
        };
        assert_eq!(
            base.painter_placement(),
            PreparedTransientPainterPlacement::LayerEnd
        );
    }
}
