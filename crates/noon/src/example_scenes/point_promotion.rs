//! One typed live program for own-point promotion and null-point alignment.
use crate::{
    ContinuationStep, LiveContinuation, LiveProgram, LiveSession, LiveSessionError,
    ManimGeometryOptions, Mobject, PointAlignmentSide, Scene,
};

pub struct PointPromotion {
    left: Mobject,
    right: Mobject,
    left_child: Mobject,
    // The native counterpart of the copied Python metadata alias.
    left_metadata: Option<(Mobject, Mobject)>,
    stage: u8,
}

impl LiveContinuation for PointPromotion {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        if self.stage == 3 {
            return Ok(ContinuationStep::Finished);
        }
        let step = (|| -> Result<_, LiveSessionError> {
            match self.stage {
                0 => {}
                1 => {
                    let before = live.point_reduction_tree((&self.left).into())?.points;
                    let captured = self.left.split_iter()?;
                    let references = [(&self.left_child).into()];
                    let copied = live.promote_own_points((&self.left).into(), &references)?;
                    let root = copied.mobject(&self.left)?;
                    let alias = copied.mobject(&self.left_child)?;
                    assert_eq!(self.left.get_num_points()?, 0);
                    assert_eq!(self.left.member_count()?, 2);
                    assert_eq!(
                        self.left.member_at(0)?.unwrap().as_target().node_id(),
                        self.left_child.node_id()
                    );
                    assert_eq!(
                        self.left.member_at(1)?.unwrap().as_target().node_id(),
                        root.node_id()
                    );
                    assert_eq!(root.get_num_points()?, 32);
                    assert_eq!(live.point_reduction_tree((&root).into())?.points, before);
                    assert_eq!(root.member_count()?, 0);
                    assert_ne!(root.node_id(), self.left.node_id());
                    assert_ne!(alias.node_id(), self.left_child.node_id());
                    assert_eq!(alias.get_num_points()?, 16);
                    let old = captured
                        .collect::<Result<Vec<_>, _>>()?
                        .iter()
                        .map(|entry| entry.as_target().node_id())
                        .collect::<Vec<_>>();
                    assert_eq!(old, [self.left.node_id(), self.left_child.node_id()]);
                    self.left_metadata = Some((root, alias));
                }
                2 => {
                    let aligned = live
                        .align_own_points((&self.left).into(), (&self.right).into(), &[], &[])?
                        .expect("only the right operand still has own points");
                    assert_eq!(aligned.side, PointAlignmentSide::Right);
                    let root = aligned.copied.mobject(&self.right)?;
                    assert_eq!(root.get_num_points()?, 32);
                    assert_eq!(root.member_count()?, 0);
                    assert_eq!(self.left.get_num_points()?, 0);
                    assert_eq!(self.right.get_num_points()?, 0);
                    assert_eq!(self.right.member_count()?, 2);
                    assert_eq!(
                        self.right.member_at(1)?.unwrap().as_target().node_id(),
                        root.node_id()
                    );
                    assert!(live
                        .align_own_points((&self.left).into(), (&self.right).into(), &[], &[])?
                        .is_none());
                    let (left_copy, left_alias) = self
                        .left_metadata
                        .as_ref()
                        .expect("retained native metadata");
                    assert_eq!(left_copy.get_num_points()?, 32);
                    assert_eq!(left_copy.member_count()?, 0);
                    assert_eq!(left_alias.get_num_points()?, 16);
                }
                _ => unreachable!("three held membership phases"),
            }
            self.stage += 1;
            live.wait_segment(0.25).map(ContinuationStep::Await)
        })();
        step.map_err(|error| error.to_string())
    }
}

pub fn program() -> Result<LiveProgram<PointPromotion>, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        detached_profiles(&mut scene)?;
        let mut left = scene.geometry(ManimGeometryOptions::circle(0.7)?)?;
        let mut right = scene.geometry(ManimGeometryOptions::circle(0.7)?)?;
        let mut left_child = scene.geometry(ManimGeometryOptions::square(0.8)?)?;
        let mut right_child = scene.geometry(ManimGeometryOptions::square(0.8)?)?;
        for (shape, x, color) in [
            (&mut left, -1.2, (1.0, 0.0, 0.0)),
            (&mut right, 1.2, (1.0, 0.0, 0.0)),
            (&mut left_child, -1.2, (0.0, 0.0, 1.0)),
            (&mut right_child, 1.2, (0.0, 1.0, 0.0)),
        ] {
            shape.shift(x, 0.0)?;
            shape.set_fill(color.0, color.1, color.2, 1.0)?;
            shape.set_stroke_width(0.0)?;
        }
        assert_eq!(left.get_num_points()?, 32);
        assert_eq!(left_child.get_num_points()?, 16);
        left.shift_3d(crate::SemanticVec3::new(0., 0., 1.125))?;
        right.shift_3d(crate::SemanticVec3::new(0., 0., -3.5))?;
        left.add((&left_child).into())?;
        right.add((&right_child).into())?;
        scene.add_many(&[(&left).into(), (&right).into()])?;
        Ok(scene.into_live_program(PointPromotion {
            left,
            right,
            left_child,
            left_metadata: None,
            stage: 0,
        })?)
    };
    build().map_err(|error| error.to_string())
}

fn detached_profiles(scene: &mut Scene) -> Result<(), crate::AuthoringError> {
    let points = (0..8)
        .map(|i| crate::SemanticVec3::new(i as f64, (i % 3) as f64, i as f64 * 0.5 - 1.125))
        .collect::<Vec<_>>();
    for family in [false, true] {
        for draw_enabled in [false, true] {
            for count in [0, 1, 4, 5, 8] {
                let mut child =
                    scene.geometry(ManimGeometryOptions::path(crate::VectorPath::new())?)?;
                child.set_points_3d(&points[..4])?;
                let root = if family {
                    let root = scene.family_with_presentation(
                        &[(&child).into()],
                        0.,
                        crate::Color::WHITE,
                        draw_enabled,
                    )?;
                    root.set_points_3d(&points[..count])?;
                    crate::CompositeEntryHandle::Family(root)
                } else {
                    let mut options = ManimGeometryOptions::path(crate::VectorPath::new())?;
                    options.set_draw_enabled(draw_enabled);
                    let mut root = scene.geometry(options)?;
                    root.set_points_3d(&points[..count])?;
                    root.add((&child).into())?;
                    crate::CompositeEntryHandle::Mobject(root)
                };
                let old_members = root.member_view()?;
                let old_iterator = root.split_iter()?;
                let copied = root
                    .as_target()
                    .promote_own_points_with_references(&[(&child).into()])?;
                let clone = copied.root();
                assert_eq!(root.get_num_points()?, 0);
                assert_eq!(root.member_count()?, 2);
                assert_eq!(
                    root.member_at(0)?.unwrap().as_target().node_id(),
                    child.node_id()
                );
                assert_eq!(old_members.len()?, 1);
                let old = old_iterator.collect::<Result<Vec<_>, _>>()?;
                assert_eq!(old.len(), if count == 0 { 1 } else { 2 });
                assert_eq!(clone.member_count()?, 0);
                assert_eq!(clone.as_target().all_points_3d()?, points[..count]);
                let alias = copied.mobject(&child)?;
                assert_ne!(alias.node_id(), child.node_id());
                assert_eq!(alias.path_query()?.points_3d()?, points[..4]);
                let mut peer =
                    scene.geometry(ManimGeometryOptions::path(crate::VectorPath::new())?)?;
                peer.set_points_3d(&points[..4])?;
                root.as_target()
                    .align_own_points((&peer).into())?
                    .expect("pointful peer");
                assert_eq!(peer.get_num_points()?, 0);
                assert!(root.as_target().align_own_points((&peer).into())?.is_none());
                let before = root.as_target().all_points_3d()?;
                let second = root.as_target().promote_own_points()?;
                assert_eq!(second.root().get_num_points()?, 0);
                assert_eq!(second.root().member_count()?, 0);
                assert_eq!(root.as_target().all_points_3d()?, before);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    #[test]
    fn shared_point_promotion_program_publishes_all_three_holds() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        for end in [0.25, 0.5, 0.75] {
            assert!(matches!(
                program.resume().unwrap(),
                LiveProgramStatus::Awaiting(_)
            ));
            match program.drive_to(&mut callbacks, end).unwrap() {
                LiveProgramStatus::PublicationPending(expected) => {
                    let actual = program.take_renderer_publication().context();
                    assert_eq!(actual, expected);
                    program.admit_publication(actual).unwrap();
                }
                LiveProgramStatus::ReadyToResume => {}
                other => panic!("point promotion completion: {other:?}"),
            }
        }
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }
}
