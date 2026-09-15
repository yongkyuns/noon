//! Explicit test-artifact exporter: ordinary Rust programs run through shared
//! lowering/runtime; the optional codec observes their actual execution frames.
use noon::{
    diagnostics::execution_frame_value, example_scenes, AnimationOptions, ContinuationStep,
    LiveContinuation, LiveProgram, LiveProgramStatus, LiveSession, Mobject, RateFunction,
    RustHostCallbackTable, Scene,
};
use serde_json::json;

fn emit<C: LiveContinuation<Error = String>>(
    name: &str,
    mut program: LiveProgram<C>,
    times: &[f64],
) -> Result<(), String> {
    let mut callbacks = RustHostCallbackTable::new();
    let mut frames = Vec::new();
    for &time in times {
        loop {
            match program.status() {
                LiveProgramStatus::ReadyToResume => {
                    program.resume().map_err(|e| e.to_string())?;
                }
                LiveProgramStatus::PublicationPending(expected) => {
                    // This diagnostic host admits consumed publications, without a GPU.
                    let publication = program.take_renderer_publication().context();
                    assert_eq!(publication, expected);
                    program
                        .admit_publication(publication)
                        .map_err(|e| e.to_string())?;
                }
                LiveProgramStatus::Awaiting(_) => {
                    let status = program
                        .drive_to(&mut callbacks, time)
                        .map_err(|e| e.to_string())?;
                    if matches!(status, LiveProgramStatus::Awaiting(_)) {
                        program.take_renderer_publication();
                        break;
                    }
                }
                LiveProgramStatus::Finished => break,
                LiveProgramStatus::Terminal => return Err(format!("{name}: terminal program")),
            }
        }
        let frame = execution_frame_value(program.session());
        assert_eq!(frame["time"], time, "{name}: sample time was not reached");
        frames.push(frame);
    }
    assert_eq!(
        program.status(),
        LiveProgramStatus::Finished,
        "{name}: source did not finish"
    );
    println!(
        "{}",
        json!({ "name": name, "times": times, "frames": frames })
    );
    Ok(())
}

struct TransformPathArc {
    source: Mobject,
    target: Mobject,
    stage: u8,
}

impl LiveContinuation for TransformPathArc {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {
                self.stage = 1;
                live.declare_and_activate_transform_to(
                    &self.source,
                    &self.target,
                    AnimationOptions::new()
                        .run_time(2.0)
                        .rate_func(RateFunction::Linear)
                        .path_arc(std::f64::consts::PI),
                )
                .map(ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            1 => {
                self.stage = 2;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("Transform path-arc continuation resumed after completion".into()),
        }
    }
}

fn transform_path_arc_program() -> Result<LiveProgram<TransformPathArc>, String> {
    let mut scene = Scene::new();
    let mut source = scene.circle(0.5).map_err(|error| error.to_string())?;
    source
        .set_fill(
            f64::from(noon_core::BLUE.red),
            f64::from(noon_core::BLUE.green),
            f64::from(noon_core::BLUE.blue),
            1.0,
        )
        .map_err(|error| error.to_string())?;
    source
        .set_translation(-2.0, 0.0)
        .map_err(|error| error.to_string())?;
    scene.add(&source).map_err(|error| error.to_string())?;
    let mut target = source.target_editor().map_err(|error| error.to_string())?;
    target
        .set_translation(2.0, 0.0)
        .map_err(|error| error.to_string())?;
    scene
        .into_live_program(TransformPathArc {
            source,
            target,
            stage: 0,
        })
        .map_err(|error| error.to_string())
}

fn main() -> Result<(), String> {
    emit(
        "ordinary_affine_play",
        example_scenes::ordinary_affine_continuation_program()?,
        &[0.0, 0.5, 1.0, 2.0, 2.5, 3.0, 3.5, 4.0],
    )?;
    emit(
        "ordinary_composition_continuation",
        example_scenes::ordinary_composition_continuation_program()?,
        &[0.0, 0.5, 1.0, 2.0, 2.5, 3.0, 3.5, 4.0],
    )?;
    emit(
        "manim_parity_square_to_circle",
        example_scenes::ordinary_create_then_content_morph_program()?,
        &[0.0, 0.25, 0.5, 1.0, 1.25, 1.5, 2.0, 2.5, 3.0],
    )?;
    emit(
        "manim_parity_different_rotations",
        example_scenes::ordinary_different_rotations_program()?,
        &[0.0, 0.25, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0],
    )?;
    emit(
        "manim_gallery_moving_around",
        example_scenes::moving_around::program()?,
        &[0.0, 0.25, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.25, 3.5, 4.0],
    )?;
    emit(
        "ordinary_fade_synchronous_continuation",
        example_scenes::ordinary_fade_continuation_program()?,
        &[0.0, 0.25, 0.5, 1.0, 1.5, 2.0, 2.125, 2.25],
    )?;
    emit(
        "ordinary_timed_composition",
        example_scenes::timed_composition::program()?,
        &[
            0.0, 0.1, 0.3, 0.5, 0.9, 1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5,
        ],
    )?;
    emit(
        "ordinary_draw_border_then_fill",
        example_scenes::draw_border_then_fill::program()?,
        &[0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.25],
    )?;
    emit(
        "ordinary_family_arrangement",
        example_scenes::family_arrangement::program()?,
        &[0.0, 0.25, 0.5, 0.75, 1.0],
    )?;
    emit(
        "manim_parity_cyclic_replace",
        example_scenes::cyclic_replace::program()?,
        &[0.0, 0.25, 0.5, 0.75, 1.0, 1.25],
    )?;
    emit(
        "manim_parity_transform_path_arc",
        transform_path_arc_program()?,
        &[0.0, 0.5, 1.0, 1.5, 2.0],
    )?;
    for (name, mut session) in [
        (
            "specialized_geometry",
            example_scenes::specialized_geometry::session()?,
        ),
        (
            "ordinary_family_placement",
            example_scenes::family_placement::session()?,
        ),
    ] {
        let times = [0.0, 0.5, 1.0];
        let mut frames = Vec::new();
        for time in times {
            session.advance_to(time).map_err(|e| e.to_string())?;
            frames.push(execution_frame_value(&session));
        }
        println!(
            "{}",
            json!({ "name": name, "times": times, "frames": frames })
        );
    }
    for layout in ["fit", "fixed", "overdraw"] {
        let context =
            json!({"object_count": 16, "layout": layout, "aspect": 16.0 / 9.0, "duration": 2.0});
        let mut session =
            example_scenes::analytic_profile::session(16, layout.parse()?, 16.0 / 9.0, 2.0)?;
        let times = [0.0, 0.5, 1.0, 2.0];
        let mut frames = Vec::new();
        for time in times {
            session
                .advance_to(time)
                .map_err(|error| error.to_string())?;
            frames.push(execution_frame_value(&session));
        }
        println!(
            "{}",
            json!({"name": format!("analytic_profile_{layout}"), "source": "analytic_profile", "context": context, "times": times, "frames": frames})
        );
    }
    Ok(())
}
