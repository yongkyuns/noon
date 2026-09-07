//! Atomic scene membership and family projection through the shared live runtime.
use std::{cell::RefCell, rc::Rc};

use crate::{
    ContinuationStep, LiveContinuation, LiveProgram, LiveSession, Mobject, MobjectFamily,
    MobjectFamilyMember::{Family, Mobject as Leaf},
    Scene, SemanticNodeId, SemanticStore,
};

pub struct OrdinaryMembership {
    red: Mobject,
    blue: Mobject,
    green: Mobject,
    family: MobjectFamily,
    store: Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    stage: u8,
}

impl LiveContinuation for OrdinaryMembership {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let expected = match self.stage {
            0 => {
                // A bad final argument must not partially admit the valid first one.
                let foreign = Scene::new().square(1.0)?;
                if live.add_many(&[Leaf(&self.red), Leaf(&foreign)]).is_ok()
                    || live.contains(&self.red).map_err(|e| e.to_string())?
                {
                    return Err("invalid membership batch partially committed".into());
                }
                live.add_many(&[Leaf(&self.red), Leaf(&self.blue)])
                    .map_err(|e| e.to_string())?;
                vec![self.red.node_id(), self.blue.node_id()]
            }
            1 => {
                live.replace(Leaf(&self.red), Leaf(&self.green))
                    .map_err(|e| e.to_string())?;
                vec![self.green.node_id(), self.blue.node_id()]
            }
            2 => {
                live.remove(&self.blue).map_err(|e| e.to_string())?;
                vec![self.green.node_id()]
            }
            3 => {
                live.add_many(&[Family(&self.family)])
                    .map_err(|e| e.to_string())?;
                vec![self.green.node_id(), self.family.node_id()]
            }
            4 => {
                // Removing a descendant promotes its surviving sibling at the family slot.
                live.remove(&self.red).map_err(|e| e.to_string())?;
                vec![self.green.node_id(), self.blue.node_id()]
            }
            5 => {
                // Re-adding an existing root moves it to the end without changing identity.
                live.add(&self.green).map_err(|e| e.to_string())?;
                vec![self.blue.node_id(), self.green.node_id()]
            }
            6 => {
                live.clear().map_err(|e| e.to_string())?;
                vec![]
            }
            7 => return Ok(ContinuationStep::Finished),
            _ => return Err("membership continuation resumed after completion".into()),
        };
        let actual = self.store.borrow().node(self.root).unwrap().members();
        if actual != expected {
            return Err(format!(
                "membership stage {}: {actual:?}, expected {expected:?}",
                self.stage
            ));
        }
        self.stage += 1;
        live.wait_segment(0.5)
            .map(ContinuationStep::Await)
            .map_err(|e| e.to_string())
    }
}

pub fn program() -> Result<LiveProgram<OrdinaryMembership>, String> {
    let scene = Scene::new();
    let mut red = scene.square(2.0)?;
    red.set_fill(1.0, 0.0, 0.0, 1.0)?;
    red.set_translation(-0.5, 0.0)?;
    let mut blue = scene.square(2.0)?;
    blue.set_fill(0.0, 0.0, 1.0, 1.0)?;
    blue.set_translation(0.5, 0.0)?;
    let mut green = scene.square(2.0)?;
    green.set_fill(0.0, 1.0, 0.0, 1.0)?;
    let family = scene.family(&[&red, &blue])?;
    let continuation = OrdinaryMembership {
        red,
        blue,
        green,
        family,
        store: Rc::clone(scene.store()),
        root: scene.root(),
        stage: 0,
    };
    scene
        .into_live_program(continuation)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, Rect, RustHostCallbackTable, Vec2};

    #[test]
    fn shared_membership_publishes_each_ordered_projection_and_clear() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        for (stage, count) in [2, 2, 1, 3, 2, 2, 0].into_iter().enumerate() {
            assert!(matches!(
                program.resume().unwrap(),
                LiveProgramStatus::Awaiting(_)
            ));
            let visible =
                program.query_viewport(Rect::new(Vec2::new(-4.0, -3.0), Vec2::new(4.0, 3.0)));
            assert_eq!(visible.object_indices().len(), count, "stage {stage}");
            match program
                .drive_to(&mut callbacks, (stage + 1) as f64 * 0.5)
                .unwrap()
            {
                LiveProgramStatus::PublicationPending(expected) => {
                    let context = program.take_renderer_publication().context();
                    assert_eq!(context, expected);
                    program.admit_publication(context).unwrap();
                }
                LiveProgramStatus::ReadyToResume => {}
                status => panic!("membership completion: {status:?}"),
            }
        }
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }
}
