//! Public child edits and alias-preserving copies, paired with
//! `web/python/examples/manim_parity_object_members.py` on the shared renderer.
use crate::{
    ContinuationStep, LiveContinuation, LiveProgram, LiveSession, LiveSessionError,
    ManimGeometryOptions, Mobject, Scene,
};

fn split_ids(root: &Mobject) -> Result<Vec<noon_core::SemanticNodeId>, crate::AuthoringError> {
    root.split_members().map(|members| {
        members
            .iter()
            .map(|member| member.as_target().node_id())
            .collect()
    })
}

fn assert_depth(
    live: &LiveSession<'_>,
    root: &Mobject,
    own: f64,
    minimum: f64,
    maximum: f64,
) -> Result<(), LiveSessionError> {
    let snapshot = live.point_reduction_tree(root.into())?;
    assert!(!snapshot.points.is_empty());
    assert!(snapshot.points.iter().all(|point| point.z == own));
    let coordinates = snapshot
        .flattened_points_3d()
        .into_iter()
        .flat_map(|point| [point.x, point.y, point.z])
        .collect::<Vec<_>>();
    assert_eq!(
        crate::reduce_point_extremum(&coordinates, 3, 2, -1.)?,
        minimum
    );
    assert_eq!(
        crate::reduce_point_extremum(&coordinates, 3, 2, 1.)?,
        maximum
    );
    Ok(())
}

pub struct ObjectMembershipQueries {
    root: Mobject,
    a: Mobject,
    b: Mobject,
    c: Mobject,
    stage: u8,
}

impl LiveContinuation for ObjectMembershipQueries {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        if self.stage == 2 {
            return Ok(ContinuationStep::Finished);
        }
        let step = (|| -> Result<_, LiveSessionError> {
            if self.stage == 0 {
                assert_depth(live, &self.root, 1.125, -3.5, 4.75)?;
            }
            if self.stage == 1 {
                live.replace_members((&self.root).into(), &[(&self.a).into(), (&self.b).into()])?;
                live.shift_3d(&self.root, noon_core::SemanticVec3::new(0., 0., 0.5))?;
                assert_depth(live, &self.root, 1.625, -3., 2.75)?;
                assert_depth(live, &self.c, 4.75, 4.75, 4.75)?;
                // The Python metadata alias points to the same copied child.
                let copied = live.copy_mobject_with_references(&self.root, &[(&self.b).into()])?;
                let copied_root = copied.root();
                let copied_alias = copied.mobject(&self.b)?;
                assert_ne!(copied_root.node_id(), self.root.node_id());
                assert_ne!(copied_alias.node_id(), self.b.node_id());
                assert_eq!(
                    copied_root.member_at(1)?.unwrap().as_target().node_id(),
                    copied_alias.node_id()
                );
                assert_ne!(
                    copied_root.member_at(0)?.unwrap().as_target().node_id(),
                    self.a.node_id()
                );
                live.shift(copied_root, 0.0, -1.0)?;
                assert_eq!(
                    split_ids(&self.root)?,
                    [self.root.node_id(), self.a.node_id(), self.b.node_id()]
                );
                live.add(copied_root)?;
                assert_depth(live, copied_root, 1.625, -3., 2.75)?;
            }
            self.stage += 1;
            live.wait_segment(0.25).map(ContinuationStep::Await)
        })();
        step.map_err(|error| error.to_string())
    }
}

/// The same typed continuation feeds native export/window and direct Rust/WASM.
pub fn program() -> Result<LiveProgram<ObjectMembershipQueries>, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        #[cfg(feature = "native-text")]
        {
            let mut text = scene.text(crate::Text::new("I"))?;
            let mut circle = scene.geometry(ManimGeometryOptions::circle(0.2)?)?;
            text.shift_3d(crate::SemanticVec3::new(-3., 0., 1.125))?;
            circle.shift_3d(crate::SemanticVec3::new(2., 0., -3.5))?;
            let family = scene.family(&[(&text).into(), (&circle).into()])?;
            let text_anchor = crate::LayoutAnchor::from(&text);
            let family_anchor = crate::LayoutAnchor::from(&family);
            for direction in [-1., 1.] {
                assert_eq!(
                    text_anchor
                        .critical_point_3d(crate::SemanticVec3::new(0., 0., direction))?
                        .z,
                    1.125
                );
                assert_eq!(
                    family_anchor
                        .critical_point_3d(crate::SemanticVec3::new(0., 0., direction))?
                        .z,
                    if direction < 0. { -3.5 } else { 1.125 }
                );
            }
        }
        let mut root = scene.geometry(ManimGeometryOptions::circle(0.4)?)?;
        let mut a = scene.geometry(ManimGeometryOptions::square(0.3)?)?;
        let mut b = scene.geometry(ManimGeometryOptions::circle(0.3)?)?;
        let mut c = scene.geometry(ManimGeometryOptions::square(0.4)?)?;
        a.shift(-1.0, 0.0)?;
        b.shift(1.0, 0.0)?;
        c.shift(0.0, 1.0)?;
        root.shift_3d(noon_core::SemanticVec3::new(0., 0., 1.125))?;
        a.shift_3d(noon_core::SemanticVec3::new(0., 0., 2.25))?;
        b.shift_3d(noon_core::SemanticVec3::new(0., 0., -3.5))?;
        c.shift_3d(noon_core::SemanticVec3::new(0., 0., 4.75))?;
        let original_root = root.node_id();
        root.add_many(&[(&a).into(), (&b).into(), (&a).into()])?;
        assert_eq!(split_ids(&root)?, [original_root, b.node_id(), a.node_id()]);
        let captured = root.split_iter()?;
        root.add((&c).into())?;
        let captured_ids = captured
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .map(|member| member.as_target().node_id())
            .collect::<Vec<_>>();
        assert_eq!(captured_ids, [original_root, b.node_id(), a.node_id()]);
        root.remove((&a).into())?;
        let captured = root.split_iter()?;
        root.prepend_many(&[(&c).into(), (&b).into(), (&c).into()])?;
        assert_eq!(captured.collect::<Result<Vec<_>, _>>()?.len(), 1);
        assert_eq!(split_ids(&root)?, [original_root, c.node_id(), b.node_id()]);
        scene.add(&root)?;
        Ok(scene.into_live_program(ObjectMembershipQueries {
            root,
            a,
            b,
            c,
            stage: 0,
        })?)
    };
    build().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    #[test]
    fn shared_public_object_membership_program_publishes_both_segments() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        for end in [0.25, 0.5] {
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
                other => panic!("membership completion: {other:?}"),
            }
        }
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }
}
