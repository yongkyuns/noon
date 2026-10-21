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

pub struct ObjectMembershipQueries {
    root: Mobject,
    a: Mobject,
    b: Mobject,
    stage: u8,
}

impl LiveContinuation for ObjectMembershipQueries {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        if self.stage == 2 {
            return Ok(ContinuationStep::Finished);
        }
        let step = (|| -> Result<_, LiveSessionError> {
            if self.stage == 1 {
                live.replace_members((&self.root).into(), &[(&self.a).into(), (&self.b).into()])?;
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
        let root = scene.geometry(ManimGeometryOptions::circle(0.2)?)?;
        let mut a = scene.geometry(ManimGeometryOptions::square(0.1)?)?;
        let mut b = scene.geometry(ManimGeometryOptions::circle(0.1)?)?;
        let mut c = scene.geometry(ManimGeometryOptions::square(0.15)?)?;
        a.shift(-1.0, 0.0)?;
        b.shift(1.0, 0.0)?;
        c.shift(0.0, 1.0)?;
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
