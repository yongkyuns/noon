//! Ordered retained-path and analytic callback results in one coherent frame.
//!
//! Paired with `ordinary_effective_mixed_content_updaters.py`. The two waits
//! force observations of changed content and then equal content independently
//! of presentation frequency. Callbacks remain explicit host-dynamic work.

use crate::{
    ContinuationStep, GeometryRef, LiveContinuation, LiveProgram, LiveSession, Mobject,
    RustHostCallbackTable, Scene, Vec2, VectorPath,
};
use noon_core::HostCallbackId;

pub const DURATION: f64 = 0.25;

pub struct MixedContentContinuation {
    objects: [Mobject; 4],
    stage: u8,
}

impl LiveContinuation for MixedContentContinuation {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        if self.stage > 0 {
            let time = f64::from(self.stage) * 0.125;
            for (object, expected) in self.objects.iter().zip([
                (0.25 + time, -1.5, 0.2, 0.2),
                (-2.0, 0.0, 1.6, 0.6),
                (0.0, 0.0, 0.6, 1.2),
                (2.0, 0.0, 0.8, 0.8),
            ]) {
                let layout = live.effective_layout(object).map_err(|e| e.to_string())?;
                for (actual, expected) in [
                    (layout.center.0, expected.0),
                    (layout.center.1, expected.1),
                    (layout.width, expected.2),
                    (layout.height, expected.3),
                ] {
                    if (actual - expected).abs() > 1e-6 {
                        return Err(format!(
                            "mixed callback continuation layout {actual} differs from {expected}"
                        ));
                    }
                }
            }
        }
        if self.stage == 2 {
            return Ok(ContinuationStep::Finished);
        }
        self.stage += 1;
        live.wait_segment(0.125)
            .map(ContinuationStep::Await)
            .map_err(|e| e.to_string())
    }
}

fn polygon(points: &[(f32, f32)]) -> VectorPath {
    let mut path = VectorPath::new().move_to(Vec2::new(points[0].0, points[0].1));
    for &(x, y) in &points[1..] {
        path = path.line_to(Vec2::new(x, y));
    }
    path.close()
}

/// Normal direct Rust continuation and callback table, shared by native/WASM.
pub fn program() -> Result<(LiveProgram<MixedContentContinuation>, RustHostCallbackTable), String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut objects = Vec::with_capacity(4);
        for (x, y, color) in [
            (0.25, -1.5, (1.0, 1.0, 0.0)),
            (-2.0, 0.0, (1.0, 0.0, 0.0)),
            (0.0, 0.0, (0.0, 0.0, 1.0)),
            (2.0, 0.0, (0.0, 1.0, 0.0)),
        ] {
            let mut object = scene.circle(0.1)?;
            object.set_translation(x, y)?;
            object.set_fill(color.0, color.1, color.2, 1.0)?;
            object.set_stroke_width(0.0)?;
            scene.add(&object)?;
            objects.push(object);
        }
        let witness = objects[0].node_id();
        let mut callbacks = RustHostCallbackTable::new();
        callbacks.insert(HostCallbackId::new(1), |context| {
            let mut transform = context.target_state().transform;
            transform.translation = Vec2::new(0.25 + context.time() as f32, -1.5);
            context.set_target_transform(transform)
        })?;
        for index in 1..4 {
            callbacks.insert(HostCallbackId::new(index as u64 + 1), move |context| {
                let observed = context.read_object(witness)?;
                let center = observed
                    .bounds
                    .expect("preceding property callback retains bounds")
                    .center();
                assert!((f64::from(center.x) - (0.25 + context.time())).abs() < 1e-6);
                assert!((center.y + 1.5).abs() < 1e-6);
                let grown = context.time() >= 0.125;
                match index {
                    1 => {
                        let w = if grown { 0.8 } else { 0.4 };
                        context.set_target_path(polygon(&[
                            (-w, -0.3),
                            (w, -0.3),
                            (w, 0.3),
                            (-w, 0.3),
                        ]))
                    }
                    2 => {
                        let h = if grown { 0.6 } else { 0.3 };
                        context.set_target_path(polygon(&[
                            (-0.3, 0.0),
                            (0.0, h),
                            (0.3, 0.0),
                            (0.0, -h),
                        ]))
                    }
                    _ => context.set_target_geometry(GeometryRef::Circle {
                        radius: if grown { 0.4 } else { 0.2 },
                    }),
                }
            })?;
        }
        for (index, object) in objects.iter().enumerate() {
            callbacks.add_updater(
                &mut scene.integration_store().borrow_mut(),
                object.node_id(),
                HostCallbackId::new(index as u64 + 1),
                0.0,
                None,
            )?;
        }
        let objects = objects
            .try_into()
            .map_err(|_| "four mixed-content targets")?;
        Ok((
            scene.into_live_program(MixedContentContinuation { objects, stage: 0 })?,
            callbacks,
        ))
    };
    build().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LiveProgramStatus;
    use noon_core::GeometryResourceLookup;

    #[test]
    fn ordered_mixed_callback_content_survives_continuations_and_reuses_equal_paths() {
        let (mut program, mut callbacks) = program().unwrap();
        let identity = program.session().runtime_identity();
        let mut previous_paths = None;
        for end in [0.125, DURATION] {
            assert!(matches!(
                program.resume().unwrap(),
                LiveProgramStatus::Awaiting(_)
            ));
            program.drive_to(&mut callbacks, end - 0.0625).unwrap();
            let before = program.session().publication_context();
            let LiveProgramStatus::PublicationPending(expected) =
                program.drive_to(&mut callbacks, end).unwrap()
            else {
                panic!("completed wait needs its coherent publication");
            };
            let session = program.session();
            assert_eq!(session.runtime_identity(), identity);
            assert_eq!(session.frame().objects.len(), 4);
            assert_eq!(
                session.publication_context().scene_revision(),
                before.scene_revision()
            );
            assert_eq!(
                session.publication_context().execution_revision(),
                before.execution_revision()
            );
            let paths = [1, 2].map(|index| {
                let Some(GeometryRef::External(id)) = session.frame().objects[index].geometry()
                else {
                    panic!("callback result must retain a path");
                };
                session.geometry_resources().current_handle(*id).unwrap()
            });
            assert_ne!(paths[0], paths[1]);
            if let Some(previous) = previous_paths {
                assert_eq!(
                    paths, previous,
                    "equal paths keep their resource identities"
                );
            }
            previous_paths = Some(paths);
            let publication = program.take_renderer_publication().context();
            assert_eq!(publication, expected);
            program.admit_publication(publication).unwrap();
        }
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        assert_eq!(program.session().frame().time, DURATION);
    }
}
