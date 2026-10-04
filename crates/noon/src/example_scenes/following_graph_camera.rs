//! Direct Rust counterpart of ManimCE's FollowingGraphCamera example.
//!
//! The moving dot and camera frame remain ordinary semantic objects. Shared
//! MoveAlongPath drives the dot and one host updater keeps the camera on it.
use crate::{
    AnimationOptions, Color, ContinuationStep, LiveContinuation, LiveProgram, LiveSession,
    ManimAxesOptions, ManimGeometryOptions, Mobject, RateFunction, RustHostCallbackTable, Scene,
};

use noon_core::{HostCallbackId, SemanticMutationTransaction};

const FOLLOW_CAMERA: HostCallbackId = HostCallbackId::new(1);
const FOLLOW_START: f64 = 1.0;
const FOLLOW_END: f64 = 2.0;

pub struct FollowingGraphCamera {
    frame: Mobject,
    moving_dot: Mobject,
    path: Mobject,
    zoomed_frame: Mobject,
    restored_frame: Mobject,
    stage: u8,
}

impl LiveContinuation for FollowingGraphCamera {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {
                self.stage = 1;
                live.declare_and_activate_transform_to(
                    &self.frame,
                    &self.zoomed_frame,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Smooth),
                )
                .map(ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            1 => {
                self.stage = 2;
                live.declare_and_activate_move_along_path(
                    &self.moving_dot,
                    &self.path,
                    AnimationOptions::new()
                        .run_time(FOLLOW_END - FOLLOW_START)
                        .rate_func(RateFunction::Linear),
                )
                .map(ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            2 => {
                let mut transaction = SemanticMutationTransaction::new();
                transaction.remove_updater(self.frame.node_id(), FOLLOW_CAMERA, FOLLOW_END);
                live.apply(transaction).map_err(|error| error.to_string())?;
                self.stage = 3;
                live.declare_and_activate_transform_to(
                    &self.frame,
                    &self.restored_frame,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Smooth),
                )
                .map(ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            3 => {
                self.stage = 4;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("FollowingGraphCamera resumed after completion".into()),
        }
    }
}

/// Build the exact camera motion over shared Axes, graph and dot semantics.
/// Returns the ordinary Rust callback table required by native live execution.
pub fn program() -> Result<(LiveProgram<FollowingGraphCamera>, RustHostCallbackTable), String> {
    let mut scene = Scene::new();
    let frame = scene.camera_frame().map_err(|error| error.to_string())?;
    let axes_options =
        ManimAxesOptions::from_ranges(Some(&[-1.0, 10.0]), Some(&[-1.0, 10.0]), None, None)
            .map_err(|error| error.to_string())?;
    let axes = scene
        .axes(&axes_options)
        .map_err(|error| error.to_string())?;
    let mut graph = axes
        .plot(|x| x.sin(), Some(&[0.0, 3.0 * std::f64::consts::PI]), false)
        .map_err(|error| error.to_string())?;
    graph
        .set_color(
            f64::from(Color::BLUE.red),
            f64::from(Color::BLUE.green),
            f64::from(Color::BLUE.blue),
            1.0,
        )
        .map_err(|error| error.to_string())?;
    let path_query = graph.path_query().map_err(|error| error.to_string())?;
    let start = path_query
        .point_from_proportion(0.0)
        .map_err(|error| error.to_string())?;
    let end = path_query
        .point_from_proportion(1.0)
        .map_err(|error| error.to_string())?;

    let mut moving_dot = scene
        .geometry(ManimGeometryOptions::dot(start.0, start.1, 0.08).map_err(|e| e.to_string())?)
        .map_err(|error| error.to_string())?;
    moving_dot
        .set_color(
            f64::from(Color::ORANGE.red),
            f64::from(Color::ORANGE.green),
            f64::from(Color::ORANGE.blue),
            1.0,
        )
        .map_err(|error| error.to_string())?;
    let first_dot = scene
        .geometry(ManimGeometryOptions::dot(start.0, start.1, 0.08).map_err(|e| e.to_string())?)
        .map_err(|error| error.to_string())?;
    let last_dot = scene
        .geometry(ManimGeometryOptions::dot(end.0, end.1, 0.08).map_err(|e| e.to_string())?)
        .map_err(|error| error.to_string())?;
    scene
        .add_many(&[
            axes.family().into(),
            (&graph).into(),
            (&first_dot).into(),
            (&last_dot).into(),
            (&moving_dot).into(),
        ])
        .map_err(|error| error.to_string())?;

    let mut zoomed_frame = frame.target_editor().map_err(|error| error.to_string())?;
    zoomed_frame
        .set_translation(start.0, start.1)
        .and_then(|_| zoomed_frame.set_scale(0.5, 0.5))
        .map_err(|error| error.to_string())?;
    let restored_frame = frame.target_editor().map_err(|error| error.to_string())?;

    let mut callbacks = RustHostCallbackTable::new();
    let moving_id = moving_dot.node_id();
    callbacks
        .insert(FOLLOW_CAMERA, move |context| {
            let dot = context
                .read_object(moving_id)
                .map_err(std::io::Error::other)?;
            let mut transform = context.target_state().transform;
            // move_to aligns the effective bounds centers, including the
            // camera's invisible rectangle, rather than transform origins.
            let center = |bounds: noon_core::Rect| {
                (
                    (f64::from(bounds.min.x) + f64::from(bounds.max.x)) * 0.5,
                    (f64::from(bounds.min.y) + f64::from(bounds.max.y)) * 0.5,
                )
            };
            let dot_center = center(
                dot.bounds
                    .ok_or_else(|| std::io::Error::other("dot has no effective bounds"))?,
            );
            let camera_center = center(
                context
                    .target_state()
                    .bounds
                    .ok_or_else(|| std::io::Error::other("camera has no effective bounds"))?,
            );
            transform.translation = noon_core::Vec2::new(
                (f64::from(transform.translation.x) + dot_center.0 - camera_center.0) as f32,
                (f64::from(transform.translation.y) + dot_center.1 - camera_center.1) as f32,
            );
            context
                .set_target_transform(transform)
                .map_err(std::io::Error::other)
        })
        .map_err(|error| error.to_string())?;
    {
        let store = scene.integration_store();
        let mut store = store.borrow_mut();
        callbacks
            .add_updater(
                &mut store,
                frame.node_id(),
                FOLLOW_CAMERA,
                FOLLOW_START,
                None,
            )
            .map_err(|error| error.to_string())?;
    }

    let program = scene
        .into_live_program(FollowingGraphCamera {
            frame,
            moving_dot,
            path: graph,
            zoomed_frame,
            restored_frame,
            stage: 0,
        })
        .map_err(|error| error.to_string())?;
    Ok((program, callbacks))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LiveProgramStatus;

    fn admit_pending(program: &mut LiveProgram<FollowingGraphCamera>, status: LiveProgramStatus) {
        if matches!(status, LiveProgramStatus::PublicationPending(_)) {
            let publication = program.take_renderer_publication().context();
            program.admit_publication(publication).unwrap();
        }
    }

    #[test]
    fn following_camera_moves_with_the_dot_then_restores_on_the_shared_runtime() {
        let (mut program, mut callbacks) = program().unwrap();
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        let at_zoom_endpoint = program.drive_to(&mut callbacks, FOLLOW_START).unwrap();
        admit_pending(&mut program, at_zoom_endpoint);
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));

        program.drive_to(&mut callbacks, 1.5).unwrap();
        let following = program.session().camera().unwrap();
        assert!(following.center.x > -1.0 && following.center.x < 10.0);
        assert!((following.height - 4.0).abs() < 1.0e-4);
        let dot = program.session().frame().objects.last().unwrap();
        assert!((following.center.x - dot.transform.translation.x).abs() < 1.0e-4);
        assert!((following.center.y - dot.transform.translation.y).abs() < 1.0e-4);

        let at_path_endpoint = program.drive_to(&mut callbacks, FOLLOW_END).unwrap();
        admit_pending(&mut program, at_path_endpoint);
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        let restored = program.drive_to(&mut callbacks, 3.0).unwrap();
        admit_pending(&mut program, restored);
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        assert_eq!(
            program.session().camera().unwrap(),
            noon_core::Camera2DState::default()
        );
        assert_eq!(program.session().frame().time, 3.0);
    }
}
