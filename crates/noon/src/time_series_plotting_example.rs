//! Numeric ticks and timestamp-synchronized drawing over the ordinary runtime.
//! The measurements are illustrative, not an INS/GNSS simulation result.

use crate::plot_presentation::{number_labels, TimeSeriesPlan, TimedPlotSample};
use crate::{
    AnimationCompositionRequest as Request, AnimationOptions, Color, ContinuationStep,
    LiveContinuation, LiveProgram, LiveSession, ManimAxesOptions, ManimGeometryOptions, Mobject,
    RateFunction, Scene, SemanticAnimationCompositionKind as Kind, Text, TransformToRequest, BLUE,
    GREEN, WHITE, YELLOW,
};

pub const RUN_TIME: f64 = 6.0;
pub const SAMPLES: [TimedPlotSample; 7] = [
    TimedPlotSample {
        time: 0.0,
        value: 0.4,
    },
    TimedPlotSample {
        time: 0.5,
        value: 1.0,
    },
    TimedPlotSample {
        time: 1.5,
        value: 1.7,
    },
    TimedPlotSample {
        time: 2.0,
        value: 1.2,
    },
    TimedPlotSample {
        time: 4.0,
        value: 0.6,
    },
    TimedPlotSample {
        time: 7.0,
        value: 2.1,
    },
    TimedPlotSample {
        time: 10.0,
        value: 1.4,
    },
];

pub struct TimeSeriesPlayback {
    marker: Mobject,
    cursor: Mobject,
    segments: Vec<Mobject>,
    marker_targets: Vec<Mobject>,
    cursor_targets: Vec<Mobject>,
    durations: Vec<f64>,
    next_interval: usize,
}

impl LiveContinuation for TimeSeriesPlayback {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let index = self.next_interval;
        if index == self.segments.len() {
            return Ok(ContinuationStep::Finished);
        }
        let options = AnimationOptions::new()
            .run_time(self.durations[index])
            .rate_func(RateFunction::Linear);
        let request = Request::Composition {
            kind: Kind::Parallel,
            children: vec![
                Request::Create {
                    target: &self.segments[index],
                    options,
                },
                Request::TransformTo(TransformToRequest::new(
                    &self.marker,
                    &self.marker_targets[index],
                    options,
                )),
                Request::TransformTo(TransformToRequest::new(
                    &self.cursor,
                    &self.cursor_targets[index],
                    options,
                )),
            ],
            options,
        };
        let segment = live
            .declare_and_activate_composition(&request, options)
            .map_err(|error| error.to_string())?;
        self.next_interval += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

fn color_options(options: &mut ManimGeometryOptions, color: Color) -> Result<(), String> {
    options
        .set_color(color.red.into(), color.green.into(), color.blue.into(), 1.0)
        .map_err(|error| error.to_string())
}

pub fn program() -> Result<LiveProgram<TimeSeriesPlayback>, String> {
    let mut scene = Scene::new();
    let axes = scene
        .axes(&ManimAxesOptions::new(
            [0.0, 10.0, 2.0],
            [0.0, 2.5, 0.5],
            10.0,
            4.0,
        ))
        .map_err(|error| error.to_string())?;
    let frame = axes.authored_frame().map_err(|error| error.to_string())?;
    let plan = TimeSeriesPlan::new(frame, &SAMPLES, RUN_TIME).map_err(|error| error.to_string())?;
    scene
        .add_many(&[axes.family().into()])
        .map_err(|error| error.to_string())?;

    // These are ordinary authoring operations, not an atomic numeric-label
    // constructor or an alternative text/layout engine. The shared pure plan
    // supplies formatting/anchors; normal Text performs shaping and placement.
    for (axis, decimals, direction, exclude_zero) in [
        (frame.x(), 0, (0.0, -1.0), false),
        (frame.y(), 1, (-1.0, 0.0), true),
    ] {
        for label in
            number_labels(axis, None, decimals, exclude_zero).map_err(|error| error.to_string())?
        {
            let mut text = scene
                .text(Text::new(label.text).with_font_size(18.0))
                .map_err(|error| error.to_string())?;
            text.next_to_point(
                label.point[0],
                label.point[1],
                direction.0,
                direction.1,
                0.12,
            )
            .map_err(|error| error.to_string())?;
            scene.add(&text).map_err(|error| error.to_string())?;
        }
    }
    for (source, size, y) in [
        ("Time-synchronized sampled data", 28.0, 3.1),
        ("Uneven timestamps; one shared clock", 18.0, 2.55),
        ("Data time (s)", 20.0, -2.85),
    ] {
        let mut text = scene
            .text(Text::new(source).with_font_size(size))
            .map_err(|error| error.to_string())?;
        text.move_to(0.0, y).map_err(|error| error.to_string())?;
        scene.add(&text).map_err(|error| error.to_string())?;
    }
    let mut reference =
        ManimGeometryOptions::sampled_plot(plan.points()).map_err(|error| error.to_string())?;
    color_options(&mut reference, WHITE)?;
    reference
        .set_stroke_width(0.018)
        .map_err(|error| error.to_string())?;
    reference
        .set_object_opacity(0.25)
        .map_err(|error| error.to_string())?;
    let reference = scene
        .geometry(reference)
        .map_err(|error| error.to_string())?;
    scene.add(&reference).map_err(|error| error.to_string())?;

    let [sx, sy] = frame
        .coords_to_point(SAMPLES[0].time, frame.y().range()[0])
        .map_err(|error| error.to_string())?;
    let [ex, ey] = frame
        .coords_to_point(SAMPLES[0].time, frame.y().range()[1])
        .map_err(|error| error.to_string())?;
    let mut cursor_options =
        ManimGeometryOptions::line(sx, sy, ex, ey).map_err(|error| error.to_string())?;
    color_options(&mut cursor_options, GREEN)?;
    cursor_options
        .set_stroke_width(0.025)
        .map_err(|error| error.to_string())?;
    let cursor = scene
        .geometry(cursor_options)
        .map_err(|error| error.to_string())?;
    let mut marker_options =
        ManimGeometryOptions::circle(0.075).map_err(|error| error.to_string())?;
    color_options(&mut marker_options, YELLOW)?;
    marker_options
        .set_fill_opacity(1.0)
        .map_err(|error| error.to_string())?;
    marker_options
        .set_stroke_width(0.0)
        .map_err(|error| error.to_string())?;
    let mut marker = scene
        .geometry(marker_options)
        .map_err(|error| error.to_string())?;
    marker
        .move_to(plan.points()[0][0], plan.points()[0][1])
        .map_err(|error| error.to_string())?;
    scene
        .add_many(&[(&cursor).into(), (&marker).into()])
        .map_err(|error| error.to_string())?;
    let mut segments = Vec::new();
    let mut marker_targets = Vec::new();
    let mut cursor_targets = Vec::new();
    for index in 0..plan.durations().len() {
        let [sx, sy] = plan.points()[index];
        let [ex, ey] = plan.points()[index + 1];
        let mut options =
            ManimGeometryOptions::line(sx, sy, ex, ey).map_err(|error| error.to_string())?;
        color_options(&mut options, BLUE)?;
        options
            .set_stroke_width(0.04)
            .map_err(|error| error.to_string())?;
        segments.push(scene.geometry(options).map_err(|error| error.to_string())?);
        let mut target = marker.copy_handle().map_err(|error| error.to_string())?;
        target.move_to(ex, ey).map_err(|error| error.to_string())?;
        marker_targets.push(target);
        let mut target = cursor.copy_handle().map_err(|error| error.to_string())?;
        let [cx, cy] = plan.cursor_points()[index + 1];
        target.move_to(cx, cy).map_err(|error| error.to_string())?;
        cursor_targets.push(target);
    }
    scene
        .into_live_program(TimeSeriesPlayback {
            marker,
            cursor,
            segments,
            marker_targets,
            cursor_targets,
            durations: plan.durations().to_vec(),
            next_interval: 0,
        })
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    fn drive(program: &mut LiveProgram<TimeSeriesPlayback>, time: f64) {
        let mut callbacks = RustHostCallbackTable::new();
        for _ in 0..40 {
            match program.status() {
                LiveProgramStatus::ReadyToResume => {
                    program.resume().unwrap();
                }
                LiveProgramStatus::PublicationPending(_) => {
                    let context = program.take_renderer_publication().context();
                    program.admit_publication(context).unwrap();
                }
                LiveProgramStatus::Awaiting(_) => {
                    if (program.session().frame().time - time).abs() < 1e-9 {
                        return;
                    }
                    program.drive_to(&mut callbacks, time).unwrap();
                }
                LiveProgramStatus::Finished => return,
                other => panic!("unexpected status {other:?}"),
            }
        }
        panic!("time-series continuation failed to converge");
    }

    #[test]
    fn ordinary_runtime_keeps_marker_and_cursor_on_the_same_data_time() {
        let mut program = program().unwrap();
        program.resume().unwrap();
        drive(&mut program, 1.8); // data timestamp 3, not the third sample index
        let frame = program.session().frame();
        let marker = frame
            .objects
            .iter()
            .find(|o| o.style.fill == Some(YELLOW))
            .unwrap();
        let cursor = frame
            .objects
            .iter()
            .find(|o| o.style.stroke == Some(GREEN))
            .unwrap();
        assert!((marker.transform.translation.x + 2.0).abs() < 2e-5);
        assert!((marker.transform.translation.y + 0.56).abs() < 2e-5);
        // The cursor's base geometry is centered at x=-5, so its translation is 3.
        assert!((cursor.transform.translation.x - 3.0).abs() < 2e-5);
        let revision = program.session().publication_context().scene_revision();
        drive(&mut program, 1.9);
        assert_eq!(
            program.session().publication_context().scene_revision(),
            revision
        );
        drive(&mut program, RUN_TIME);
        assert_eq!(program.status(), LiveProgramStatus::Finished);
    }

    #[test]
    fn forward_and_jump_sampling_agree_after_crossing_interval_boundaries() {
        let mut direct = program().unwrap();
        let mut forward = program().unwrap();
        direct.resume().unwrap();
        forward.resume().unwrap();
        drive(&mut direct, 4.5);
        for index in 1..=45 {
            drive(&mut forward, f64::from(index) * 0.1);
        }
        let project = |program: &LiveProgram<TimeSeriesPlayback>| {
            program
                .session()
                .frame()
                .objects
                .iter()
                .map(|o| (o.transform, o.style, o.appearance))
                .collect::<Vec<_>>()
        };
        assert_eq!(project(&direct), project(&forward));
    }
}
