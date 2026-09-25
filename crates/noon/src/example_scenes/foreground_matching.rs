//! Native/direct-WASM counterpart of the pinned foreground matching overlap fixture.
use std::{cell::RefCell, rc::Rc};

use crate::{
    AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram, LiveSession,
    ManimGeometryOptions, Mobject, MobjectFamily, RateFunction, Scene, SemanticNodeId, Vec2,
    VectorPath,
};
use noon_core::SemanticStore;

pub struct ForegroundMatching {
    source: MobjectFamily,
    target: MobjectFamily,
    left: Mobject,
    right: Mobject,
    later: Mobject,
    store: Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    stage: u8,
}

impl ForegroundMatching {
    fn check_completion(&self, include_later: bool) -> Result<(), String> {
        let store = self.store.borrow();
        let root = store.node(self.root).ok_or("scene root was retired")?;
        let mut expected = vec![self.target.node_id()];
        if include_later {
            expected.push(self.later.node_id());
        }
        expected.extend([self.left.node_id(), self.right.node_id()]);
        if root.members() != expected
            || root.foreground_members() != [self.left.node_id(), self.right.node_id()]
        {
            return Err("matching completion changed foreground membership".into());
        }
        Ok(())
    }
}

impl LiveContinuation for ForegroundMatching {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let segment = match self.stage {
            0 => live.declare_and_activate_matching_family_transform_to(
                &self.source,
                &self.target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            ),
            1 => {
                self.check_completion(false)?;
                live.wait_segment(0.2)
            }
            2 => {
                self.check_completion(false)?;
                live.add(&self.later).map_err(|error| error.to_string())?;
                self.check_completion(true)?;
                live.wait_segment(0.2)
            }
            3 => {
                self.check_completion(true)?;
                self.stage += 1;
                return Ok(ContinuationStep::Finished);
            }
            _ => return Err("foreground matching resumed after completion".into()),
        }
        .map_err(|error| error.to_string())?;
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

fn paint(mut object: Mobject, x: f64, rgb: [f64; 3], layer: f64) -> Result<Mobject, String> {
    object
        .set_translation(x, 0.0)
        .map_err(|error| error.to_string())?;
    object
        .set_fill(rgb[0], rgb[1], rgb[2], 1.0)
        .map_err(|error| error.to_string())?;
    object
        .set_stroke_opacity(0.0)
        .map_err(|error| error.to_string())?;
    object
        .set_z_index(layer)
        .map_err(|error| error.to_string())?;
    Ok(object)
}

fn triangle(scene: &mut Scene, x: f64, color: [f64; 3]) -> Result<Mobject, String> {
    let path = VectorPath::new()
        .move_to(Vec2::new(-0.5, -0.8))
        .line_to(Vec2::new(0.5, -0.8))
        .line_to(Vec2::new(0.0, 0.8))
        .close();
    let options = ManimGeometryOptions::path(path).map_err(|error| error.to_string())?;
    paint(
        scene.geometry(options).map_err(|error| error.to_string())?,
        x,
        color,
        0.0,
    )
}

/// Three oracle variants use the same scene: an ordinary source, a foreground
/// source with the right overlay on layer 2, and an ordinary source with that layer.
pub fn program(
    source_is_foreground: bool,
    target_layer: f64,
) -> Result<LiveProgram<ForegroundMatching>, String> {
    let mut scene = Scene::new();
    let source_first = triangle(&mut scene, -2.4, [0.0, 0.0, 1.0])?;
    let source_second = triangle(&mut scene, -0.8, [0.0, 0.0, 1.0])?;
    let source = scene
        .family(&[(&source_first).into(), (&source_second).into()])
        .map_err(|error| error.to_string())?;
    let target_first = triangle(&mut scene, -2.4, [0.0, 1.0, 0.0])?;
    let target_second = triangle(&mut scene, -0.8, [0.0, 1.0, 0.0])?;
    let target_third = triangle(&mut scene, 0.8, [0.0, 1.0, 0.0])?;
    let diamond_path = VectorPath::new()
        .move_to(Vec2::new(0.0, -0.8))
        .line_to(Vec2::new(0.5, 0.0))
        .line_to(Vec2::new(0.0, 0.8))
        .line_to(Vec2::new(-0.5, 0.0))
        .close();
    let diamond_options =
        ManimGeometryOptions::path(diamond_path).map_err(|error| error.to_string())?;
    let diamond = paint(
        scene
            .geometry(diamond_options)
            .map_err(|error| error.to_string())?,
        2.4,
        [0.0, 1.0, 0.0],
        target_layer,
    )?;
    let target = scene
        .family(&[
            (&target_first).into(),
            (&target_second).into(),
            (&target_third).into(),
            (&diamond).into(),
        ])
        .map_err(|error| error.to_string())?;
    let left = paint(
        scene
            .rectangle(5.4, 0.3)
            .map_err(|error| error.to_string())?,
        -0.8,
        [1.0; 3],
        0.0,
    )?;
    let right = paint(
        scene
            .rectangle(1.2, 0.3)
            .map_err(|error| error.to_string())?,
        2.4,
        [1.0; 3],
        target_layer,
    )?;
    let later = paint(
        scene
            .rectangle(6.2, 1.0)
            .map_err(|error| error.to_string())?,
        0.0,
        [1.0, 0.0, 0.0],
        0.0,
    )?;
    scene
        .add_many(&[(&source).into()])
        .map_err(|error| error.to_string())?;
    if source_is_foreground {
        scene.add_foreground_many(&[(&left).into(), (&source).into(), (&right).into()])
    } else {
        scene.add_foreground_many(&[(&left).into(), (&right).into()])
    }
    .map_err(|error| error.to_string())?;
    let continuation = ForegroundMatching {
        source,
        target,
        left,
        right,
        later,
        store: Rc::clone(scene.integration_store()),
        root: scene.root(),
        stage: 0,
    };
    scene
        .into_live_program(continuation)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    #[test]
    fn pinned_foreground_variants_complete_on_native_typed_path() {
        for (foreground, layer) in [(false, 0.0), (true, 2.0), (false, 2.0)] {
            let mut program = program(foreground, layer).unwrap();
            let mut callbacks = RustHostCallbackTable::new();
            for time in [2.0, 2.0 + 0.2, 2.0 + 0.2 + 0.2] {
                assert!(matches!(
                    program.resume().unwrap(),
                    LiveProgramStatus::Awaiting(_)
                ));
                match program.drive_to(&mut callbacks, time).unwrap() {
                    LiveProgramStatus::PublicationPending(expected) => {
                        let context = program.take_renderer_publication().context();
                        assert_eq!(context, expected);
                        program.admit_publication(context).unwrap();
                    }
                    LiveProgramStatus::ReadyToResume => {}
                    status => panic!("foreground matching completion: {status:?}"),
                }
            }
            assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        }
    }
}
