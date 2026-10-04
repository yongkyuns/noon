//! Native counterpart of ManimCE's MovingZoomedSceneAround example.
//!
//! The two pop-out phases pair a shared Transform restore with a bounded Rust
//! host callback. The callback keeps the transparent helper rectangle fitted
//! to the moving zoom display through the same effective-layout operation used
//! by Python `UpdateFromFunc`.

use crate::{
    AnimationCompositionRequest as Request, AnimationOptions, Color, ContinuationStep,
    FadeEndpoint, FadeTranslation, ImageMobjectOptions, LayoutAnchor, LayoutDimension,
    LiveContinuation, LiveLayoutTarget, LiveProgram, LiveSession, ManimGeometryOptions,
    ManimNextToArgs, Mobject, RateFunction, RustHostCallbackTable, Scene,
    SemanticAnimationCompositionKind as Kind, SemanticFadeDirection, SemanticVec3, Text,
    TransformToRequest, ZoomedSceneOptions, ZoomedView,
};
use noon_core::HostCallbackId;

const UNFOLD_RECT: HostCallbackId = HostCallbackId::new(1);
const STEP_DURATION: f64 = 1.0;

fn smooth(duration: f64) -> AnimationOptions {
    AnimationOptions::new()
        .run_time(duration)
        .rate_func(RateFunction::Smooth)
}

fn linear(duration: f64) -> AnimationOptions {
    AnimationOptions::new()
        .run_time(duration)
        .rate_func(RateFunction::Linear)
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

pub struct MovingZoomedSceneAround {
    frame: Mobject,
    display: Mobject,
    zoomed_view: ZoomedView,
    display_helper: Mobject,
    frame_text: Mobject,
    zoomed_camera_text: Mobject,
    stage: u8,
}

impl LiveContinuation for MovingZoomedSceneAround {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {
                self.stage = 1;
                let options = smooth(STEP_DURATION);
                let requests = [
                    Request::Create {
                        target: &self.frame,
                        options,
                    },
                    Request::Fade {
                        target: &self.frame_text,
                        direction: SemanticFadeDirection::In,
                        endpoint: FadeEndpoint::new(
                            1.0,
                            FadeTranslation::Shift(SemanticVec3::new(0.0, 1.0, 0.0)),
                        ),
                        options,
                    },
                ];
                live.declare_and_activate_animation_composition(
                    Kind::Parallel,
                    &requests,
                    AnimationOptions::new().rate_func(RateFunction::Linear),
                    AnimationOptions::new(),
                )
                .map(ContinuationStep::Await)
                .map_err(message)
            }
            1 | 9 => {
                // Manim saves the current display, stretches it over the camera
                // frame, then runs Restore back to that saved state. Capture
                // from the exact effective publication before stretching.
                if self.stage == 1 {
                    live.activate_zooming(&self.zoomed_view).map_err(message)?;
                }
                let saved_display = live.target_editor(&self.display).map_err(message)?;
                live.replace_layout(
                    &LayoutAnchor::from(&self.display),
                    &LayoutAnchor::from(&self.frame),
                    LayoutDimension::Width,
                    true,
                )
                .map_err(message)?;
                let transform_options = if self.stage == 9 {
                    smooth(STEP_DURATION).reverse_rate_function(true)
                } else {
                    smooth(STEP_DURATION)
                };
                let requests = [
                    Request::TransformTo(TransformToRequest::new(
                        &self.display,
                        &saved_display,
                        transform_options,
                    )),
                    Request::CallbackInterval {
                        target: &self.display_helper,
                        callback: UNFOLD_RECT,
                        options: linear(STEP_DURATION),
                    },
                ];
                self.stage += 1;
                live.declare_and_activate_animation_composition(
                    Kind::Parallel,
                    &requests,
                    AnimationOptions::new().rate_func(RateFunction::Linear),
                    AnimationOptions::new(),
                )
                .map(ContinuationStep::Await)
                .map_err(message)
            }
            2 => {
                let caption_anchor = LayoutAnchor::from(&self.zoomed_camera_text);
                live.next_layout_to_aligned(
                    &caption_anchor,
                    LiveLayoutTarget::Mobject(&self.display),
                    &caption_anchor,
                    ManimNextToArgs {
                        direction: (0.0, -1.0),
                        buff: 0.25,
                        aligned_edge: (0.0, 0.0),
                        mask: (1.0, 1.0),
                    },
                )
                .map_err(message)?;
                self.stage = 3;
                live.declare_and_activate_fade_with_endpoint(
                    &self.zoomed_camera_text,
                    SemanticFadeDirection::In,
                    FadeEndpoint::new(
                        1.0,
                        FadeTranslation::Shift(SemanticVec3::new(0.0, 1.0, 0.0)),
                    ),
                    smooth(STEP_DURATION),
                )
                .map(ContinuationStep::Await)
                .map_err(message)
            }
            3 => {
                let frame_target = live.target_editor(&self.frame).map_err(message)?;
                live.manim_scale(&frame_target, 0.5, 1.5).map_err(message)?;
                let display_target = live.target_editor(&self.display).map_err(message)?;
                live.manim_scale(&display_target, 0.5, 1.5)
                    .map_err(message)?;
                let options = smooth(STEP_DURATION);
                let requests = [
                    Request::TransformTo(TransformToRequest::new(
                        &self.frame,
                        &frame_target,
                        options,
                    )),
                    Request::TransformTo(TransformToRequest::new(
                        &self.display,
                        &display_target,
                        options,
                    )),
                    Request::Fade {
                        target: &self.zoomed_camera_text,
                        direction: SemanticFadeDirection::Out,
                        endpoint: FadeEndpoint::default(),
                        options,
                    },
                    Request::Fade {
                        target: &self.frame_text,
                        direction: SemanticFadeDirection::Out,
                        endpoint: FadeEndpoint::default(),
                        options,
                    },
                ];
                self.stage = 4;
                live.declare_and_activate_animation_composition(
                    Kind::Parallel,
                    &requests,
                    AnimationOptions::new().rate_func(RateFunction::Linear),
                    AnimationOptions::new(),
                )
                .map(ContinuationStep::Await)
                .map_err(message)
            }
            4 | 6 | 8 | 11 => {
                self.stage += 1;
                live.wait_segment(STEP_DURATION)
                    .map(ContinuationStep::Await)
                    .map_err(message)
            }
            5 => {
                let target = live.target_editor(&self.display).map_err(message)?;
                live.manim_scale(&target, 2.0, 2.0).map_err(message)?;
                self.stage = 6;
                live.declare_and_activate_transform_to(
                    &self.display,
                    &target,
                    smooth(STEP_DURATION),
                )
                .map(ContinuationStep::Await)
                .map_err(message)
            }
            7 => {
                let target = live.target_editor(&self.frame).map_err(message)?;
                live.shift(&target, 0.0, -2.5).map_err(message)?;
                self.stage = 8;
                live.declare_and_activate_transform_to(&self.frame, &target, smooth(STEP_DURATION))
                    .map(ContinuationStep::Await)
                    .map_err(message)
            }
            10 => {
                let options = smooth(STEP_DURATION);
                let requests = [
                    Request::Uncreate {
                        target: &self.display,
                        options,
                    },
                    Request::Fade {
                        target: &self.frame,
                        direction: SemanticFadeDirection::Out,
                        endpoint: FadeEndpoint::default(),
                        options,
                    },
                ];
                self.stage = 11;
                live.declare_and_activate_animation_composition(
                    Kind::Parallel,
                    &requests,
                    AnimationOptions::new().rate_func(RateFunction::Linear),
                    AnimationOptions::new(),
                )
                .map(ContinuationStep::Await)
                .map_err(message)
            }
            12 => {
                self.stage = 13;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("MovingZoomedSceneAround resumed at an invalid stage".into()),
        }
    }
}

/// Build the native Rust program and the callback table used by its two finite
/// `UpdateFromFunc` intervals.
pub fn program() -> Result<(LiveProgram<MovingZoomedSceneAround>, RustHostCallbackTable), String> {
    let mut scene = Scene::new();
    scene.camera_frame().map_err(message)?;

    // Manim's matrix is 2 rows by 4 grayscale pixels. Keep the same values and
    // row order in the shared RGBA resource used by native and browser hosts.
    let grayscale = [0_u8, 100, 30, 200, 255, 0, 5, 33];
    let mut rgba = Vec::with_capacity(grayscale.len() * 4);
    for value in grayscale {
        rgba.extend_from_slice(&[value, value, value, 255]);
    }
    let mut image_options = ImageMobjectOptions::rgba8(4, 2, rgba).map_err(message)?;
    image_options.set_height(7.0).map_err(message)?;
    let image = scene.image(image_options).map_err(message)?;

    let dot = scene
        .geometry(ManimGeometryOptions::dot(-2.0, 2.0, 0.08).map_err(message)?)
        .map_err(message)?;
    scene
        .add_many(&[(&image).into(), (&dot).into()])
        .map_err(message)?;

    let view = scene
        .zoomed_view(ZoomedSceneOptions {
            display_height: 1.0,
            display_width: 6.0,
            zoom_factor: 0.3,
            camera_frame_stroke_width: 3.0,
            image_frame_stroke_width: 20.0,
            ..ZoomedSceneOptions::default()
        })
        .map_err(message)?;
    let mut frame = view.camera_frame().clone();
    frame.set_translation(-2.0, 2.0).map_err(message)?;
    let mut display = view.display().clone();
    display.shift(0.0, -1.0).map_err(message)?;
    frame
        .set_stroke_color(
            f64::from(Color::PURPLE.red),
            f64::from(Color::PURPLE.green),
            f64::from(Color::PURPLE.blue),
            1.0,
        )
        .map_err(message)?;
    display
        .set_stroke_color(
            f64::from(Color::RED.red),
            f64::from(Color::RED.green),
            f64::from(Color::RED.blue),
            1.0,
        )
        .map_err(message)?;

    let frame_text = scene
        .text(Text::new("Frame").with_font_size(67.0).color(Color::PURPLE))
        .map_err(message)?;
    let mut frame_text = frame_text;
    frame_text
        .manim_next_to_handle(
            &frame,
            ManimNextToArgs {
                direction: (0.0, -1.0),
                buff: 0.25,
                aligned_edge: (0.0, 0.0),
                mask: (1.0, 1.0),
            },
        )
        .map_err(message)?;
    let zoomed_camera_text = scene
        .text(
            Text::new("Zoomed camera")
                .with_font_size(67.0)
                .color(Color::RED),
        )
        .map_err(message)?;
    let zoomed_camera_text = zoomed_camera_text;
    let center = display.center().map_err(message)?;
    let mut helper_options = ManimGeometryOptions::rectangle(6.5, 1.5).map_err(message)?;
    helper_options
        .set_translation(center.0, center.1)
        .map_err(message)?;
    helper_options.set_fill_opacity(0.0).map_err(message)?;
    helper_options.set_stroke_width(0.0).map_err(message)?;
    let display_helper = scene.geometry(helper_options).map_err(message)?;
    scene
        .add_foreground_many(&[(&display_helper).into()])
        .map_err(message)?;

    let rect_anchor = LayoutAnchor::from(&display_helper);
    let display_anchor = LayoutAnchor::from(&display);
    let mut callbacks = RustHostCallbackTable::new();
    callbacks
        .insert(UNFOLD_RECT, move |context| {
            context
                .replace_layout(&rect_anchor, &display_anchor, LayoutDimension::Width, false)
                .map_err(std::io::Error::other)?;
            Ok::<(), std::io::Error>(())
        })
        .map_err(message)?;

    let program = scene
        .into_live_program(MovingZoomedSceneAround {
            frame,
            display,
            zoomed_view: view,
            display_helper,
            frame_text,
            zoomed_camera_text,
            stage: 0,
        })
        .map_err(message)?;
    Ok((program, callbacks))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LiveProgramStatus;

    fn admit(program: &mut LiveProgram<MovingZoomedSceneAround>, status: LiveProgramStatus) {
        if let LiveProgramStatus::PublicationPending(expected) = status {
            let publication = program.take_renderer_publication().context();
            assert_eq!(publication, expected);
            program.admit_publication(publication).unwrap();
        }
    }

    fn finish_segment(
        program: &mut LiveProgram<MovingZoomedSceneAround>,
        callbacks: &mut RustHostCallbackTable,
        time: f64,
    ) {
        let status = program.drive_to(callbacks, time).unwrap();
        admit(program, status);
    }

    #[test]
    fn native_program_runs_both_bounded_layout_callbacks_and_finishes_at_twelve_seconds() {
        let (mut program, mut callbacks) = program().unwrap();
        for endpoint in 1..=12 {
            assert!(matches!(
                program.resume().unwrap(),
                LiveProgramStatus::Awaiting(_)
            ));
            finish_segment(&mut program, &mut callbacks, endpoint as f64);

            if endpoint == 1 {
                assert_eq!(program.session().frame().time, 1.0);
            }
            if endpoint == 2 || endpoint == 10 {
                assert_eq!(program.session().frame().time, endpoint as f64);
            }
        }
        assert_eq!(program.status(), LiveProgramStatus::ReadyToResume);
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        assert_eq!(program.session().frame().time, 12.0);
    }
}
