//! Paired explicit-outage example. Measurements are illustrative, not GNSS data.
//! Reuses the ordinary scene, Text, membership and animation operations.

use super::{color, line, BuildResult, RECORDINGS, RUN_TIME};
use crate::plot_presentation::{number_labels, TimedPlotSample};
use crate::synchronized_plot_presentation::GappedTimeSeriesPlan;
use crate::{
    AnimationCompositionRequest as Request, AnimationOptions, ContinuationStep,
    LiveContinuation, LiveProgram, LiveSession, ManimAxesOptions, ManimGeometryOptions,
    Mobject, RateFunction, Scene, SemanticAnimationCompositionKind as Kind, Text,
    TransformToRequest, BLUE, GREEN, ORANGE, WHITE,
};

struct Stroke {
    line: Mobject,
    target: Mobject,
    start: [f64; 2],
}
struct Drawing {
    marker: Mobject,
    strokes: Vec<Option<Stroke>>,
    shown: bool,
}

pub struct GappedPlayback {
    drawings: Vec<Drawing>,
    cursor: Mobject,
    cursor_targets: Vec<Mobject>,
    durations: Vec<f64>,
    next_interval: usize,
}

impl LiveContinuation for GappedPlayback {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let index = self.next_interval;
        if index == self.durations.len() { return Ok(ContinuationStep::Finished); }
        // Ordinary source control flow at interval boundaries, not per-frame
        // callbacks. Reuse marker identity; never animate across a missing span.
        for drawing in &mut self.drawings {
            match &drawing.strokes[index] {
                None if drawing.shown => {
                    live.remove(&drawing.marker).map_err(|e| e.to_string())?;
                    drawing.shown = false;
                }
                Some(stroke) if !drawing.shown => {
                    live.move_to_point(&drawing.marker, stroke.start[0], stroke.start[1])
                        .map_err(|e| e.to_string())?;
                    live.add(&drawing.marker).map_err(|e| e.to_string())?;
                    drawing.shown = true;
                }
                _ => {}
            }
        }
        let options = AnimationOptions::new().run_time(self.durations[index])
            .rate_func(RateFunction::Linear);
        let mut children = Vec::with_capacity(self.drawings.len() * 2 + 1);
        for drawing in &self.drawings {
            if let Some(stroke) = &drawing.strokes[index] {
                children.push(Request::Create { target: &stroke.line, options });
                children.push(Request::TransformTo(TransformToRequest::new(
                    &drawing.marker, &stroke.target, options,
                )));
            }
        }
        children.push(Request::TransformTo(TransformToRequest::new(
            &self.cursor, &self.cursor_targets[index], options,
        )));
        let request = Request::Composition { kind: Kind::Parallel, children, options };
        let segment = live.declare_and_activate_composition(&request, options)
            .map_err(|e| e.to_string())?;
        self.next_interval += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> BuildResult<LiveProgram<GappedPlayback>> {
    let mut scene = Scene::new();
    let axes = scene.axes(&ManimAxesOptions::new(
        [0.0, 10.0, 2.0], [0.0, 3.0, 1.0], 10.0, 4.0,
    ))?;
    let frame = axes.authored_frame()?;
    let data: Vec<Vec<TimedPlotSample>> = RECORDINGS.iter().map(|row| {
        row.iter().map(|&[time, value]| TimedPlotSample { time, value }).collect()
    }).collect();
    let refs: Vec<_> = data.iter().map(Vec::as_slice).collect();
    // Break A after its third sample: measured endpoints at t=2 and t=5.
    let plan = GappedTimeSeriesPlan::new(frame, &refs, &[&[2], &[]], [0.0, 10.0], RUN_TIME)?;
    scene.add_many(&[axes.family().into()])?;
    for (axis, direction, exclude_zero) in [
        (frame.x(), (0.0, -1.0), false), (frame.y(), (-1.0, 0.0), true),
    ] {
        for label in number_labels(axis, None, 0, exclude_zero)? {
            let mut text = scene.text(Text::new(label.text).with_font_size(18.0))?;
            text.next_to_point(label.point[0], label.point[1], direction.0, direction.1, 0.12)?;
            scene.add(&text)?;
        }
    }
    for (source, size, x, y, tint) in [
        ("Missing measurements stay missing", 28.0, 0.0, 3.2, WHITE),
        ("Explicit blue gap: 2-5 s; orange recording continues", 17.0, 0.0, 2.72, WHITE),
        ("Data time (s)", 20.0, 0.0, -2.85, WHITE),
        ("Interrupted recording", 18.0, -2.4, 2.25, BLUE),
        ("Continuous recording", 18.0, 2.4, 2.25, ORANGE),
    ] {
        let mut text = scene.text(Text::new(source).with_font_size(size).color(tint))?;
        text.move_to(x, y)?;
        scene.add(&text)?;
    }
    // Even the dim reference is disconnected: no hidden bridge under the trace.
    for row in plan.series() {
        for &[start, end] in row.segments().iter().flatten() {
            let mut reference = line(&mut scene, start, end, WHITE, 0.018)?;
            reference.set_opacity(0.2)?;
            scene.add(&reference)?;
        }
    }
    let cursor = line(&mut scene, frame.coords_to_point(0.0, 0.0)?,
        frame.coords_to_point(0.0, 3.0)?, GREEN, 0.025)?;
    scene.add(&cursor)?;
    let mut drawings = Vec::new();
    for (row, tint) in plan.series().iter().zip([BLUE, ORANGE]) {
        let mut options = ManimGeometryOptions::circle(0.08)?;
        color(&mut options, tint)?;
        options.set_fill_opacity(1.0)?;
        options.set_stroke_width(0.0)?;
        let mut marker = scene.geometry(options)?;
        let start = row.points()[0].ok_or("example starts at a known sample")?;
        marker.move_to(start[0], start[1])?;
        scene.add(&marker)?;
        let mut strokes = Vec::new();
        for segment in row.segments() {
            strokes.push(match *segment {
                None => None,
                Some([start, end]) => {
                    let line = line(&mut scene, start, end, tint, 0.04)?;
                    let mut target = marker.copy_handle()?;
                    target.move_to(end[0], end[1])?;
                    Some(Stroke { line, target, start })
                }
            });
        }
        drawings.push(Drawing { marker, strokes, shown: true });
    }
    let mut cursor_targets = Vec::new();
    for &[x, y] in &plan.cursor_points()[1..] {
        let mut target = cursor.copy_handle()?;
        target.move_to(x, y)?;
        cursor_targets.push(target);
    }
    Ok(scene.into_live_program(GappedPlayback {
        drawings, cursor, cursor_targets, durations: plan.durations().to_vec(), next_interval: 0,
    })?)
}

#[cfg(test)]
mod tests;
