use super::*;

/// The point-owning operand selected by default null-point alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointAlignmentSide {
    Left,
    Right,
}

/// Request-local promotion result for native or frontend metadata reconstruction.
/// The left/original operands retain their semantic identities.
#[derive(Debug)]
pub struct PointAlignment {
    pub side: PointAlignmentSide,
    pub copied: SemanticCopy<CompositeEntryHandle>,
}

pub(crate) fn alignment_side(left: usize, right: usize) -> Option<PointAlignmentSide> {
    match (left == 0, right == 0) {
        (true, false) => Some(PointAlignmentSide::Right),
        (false, true) => Some(PointAlignmentSide::Left),
        _ => None,
    }
}

impl MobjectTarget<'_> {
    /// Observe default own-point alignment without changing either operand.
    pub fn point_alignment_side(
        &self,
        other: MobjectTarget<'_>,
    ) -> Result<Option<PointAlignmentSide>, AuthoringError> {
        self.validate()?;
        other.require_store(self.integration_store())?;
        Ok(alignment_side(
            self.get_num_points()?,
            other.get_num_points()?,
        ))
    }

    /// Default null alignment promotes only the operand with own points.
    /// Both pointful and both point-free operands remain unchanged.
    pub fn align_own_points(
        &self,
        other: MobjectTarget<'_>,
    ) -> Result<Option<PointAlignment>, AuthoringError> {
        self.align_own_points_with_references(other, &[], &[])
    }

    /// Only references belonging to the selected operand are copied.
    pub fn align_own_points_with_references(
        &self,
        other: MobjectTarget<'_>,
        left_references: &[MobjectTarget<'_>],
        right_references: &[MobjectTarget<'_>],
    ) -> Result<Option<PointAlignment>, AuthoringError> {
        let Some(side) = self.point_alignment_side(other)? else {
            return Ok(None);
        };
        let copied = match side {
            PointAlignmentSide::Left => self.promote_own_points_with_references(left_references)?,
            PointAlignmentSide::Right => {
                other.promote_own_points_with_references(right_references)?
            }
        };
        Ok(Some(PointAlignment { side, copied }))
    }
}

#[cfg(test)]
mod default_point_alignment_tests {
    use super::*;
    use crate::{Scene, VectorPath};

    #[test]
    fn authored_alignment_truth_table_preserves_noops_and_promotes_the_pointful_operand() {
        for left_count in [0, 4] {
            for right_count in [0, 4] {
                let mut scene = Scene::new();
                let left = scene
                    .path(
                        VectorPath::from_manim_points_f64(vec![(0.0, 0.0); left_count]),
                        Default::default(),
                    )
                    .unwrap();
                let right = scene
                    .path(
                        VectorPath::from_manim_points_f64(vec![(1.0, 0.0); right_count]),
                        Default::default(),
                    )
                    .unwrap();
                let revision = scene.revision();
                let nodes = scene.integration_store().borrow().len();
                let ids = [left.node_id(), right.node_id()];
                let result = MobjectTarget::from(&left)
                    .align_own_points((&right).into())
                    .unwrap();
                assert_eq!([left.node_id(), right.node_id()], ids);
                match result {
                    None => {
                        assert_eq!(left_count, right_count);
                        assert_eq!(scene.revision(), revision);
                        assert_eq!(scene.integration_store().borrow().len(), nodes);
                        assert_eq!(left.get_num_points().unwrap(), left_count);
                        assert_eq!(right.get_num_points().unwrap(), right_count);
                    }
                    Some(result) => {
                        assert_ne!(left_count, right_count);
                        assert_eq!(
                            result.side,
                            if left_count == 0 {
                                PointAlignmentSide::Right
                            } else {
                                PointAlignmentSide::Left
                            }
                        );
                        assert_eq!(left.get_num_points().unwrap(), 0);
                        assert_eq!(right.get_num_points().unwrap(), 0);
                        assert_eq!(result.copied.root().get_num_points().unwrap(), 4);
                        assert_eq!(result.copied.root().member_count().unwrap(), 0);
                        assert_eq!(scene.revision(), revision.checked_next().unwrap());
                        assert_eq!(scene.integration_store().borrow().len(), nodes + 1);
                    }
                }
            }
        }
    }

    #[test]
    fn live_alignment_uses_one_coherent_publication_and_rejects_foreign_noop_pairs() {
        for left_count in [0, 4] {
            for right_count in [0, 4] {
                let mut scene = Scene::new();
                let left = scene
                    .path(
                        VectorPath::from_manim_points_f64(vec![(0.0, 0.0); left_count]),
                        Default::default(),
                    )
                    .unwrap();
                let right = scene
                    .path(
                        VectorPath::from_manim_points_f64(vec![(1.0, 0.0); right_count]),
                        Default::default(),
                    )
                    .unwrap();
                scene.add(&left).unwrap();
                scene.add(&right).unwrap();
                let mut session = scene.execution_session().unwrap();
                let publication = session.publication_context();
                let revision = scene.revision();
                let result = scene
                    .live(&mut session)
                    .align_own_points((&left).into(), (&right).into(), &[], &[])
                    .unwrap();
                if left_count == right_count {
                    assert!(result.is_none());
                    assert_eq!(session.publication_context(), publication);
                    assert_eq!(scene.revision(), revision);
                } else {
                    assert!(result.is_some());
                    assert_ne!(session.publication_context(), publication);
                    assert_eq!(scene.revision(), revision.checked_next().unwrap());
                    assert_eq!(left.get_num_points().unwrap(), 0);
                    assert_eq!(right.get_num_points().unwrap(), 0);
                }
            }
        }
        let mut scene = Scene::new();
        let left = scene.path(VectorPath::new(), Default::default()).unwrap();
        let foreign = Scene::new()
            .path(VectorPath::new(), Default::default())
            .unwrap();
        let before = scene.revision();
        assert!(MobjectTarget::from(&left)
            .align_own_points((&foreign).into())
            .is_err());
        assert_eq!(scene.revision(), before);
    }
}
