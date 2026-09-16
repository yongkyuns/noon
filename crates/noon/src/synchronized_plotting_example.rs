//! Two differently sampled illustrative recordings over one ordinary runtime.
//! This is not navigation simulation data or a long-recording streaming player.

use crate::plot_presentation::{number_labels, TimedPlotSample};
use crate::synchronized_plot_presentation::SynchronizedTimeSeriesPlan;
use crate::{
    AnimationCompositionRequest as Request, AnimationOptions, Color, ContinuationStep,
    LiveContinuation, LiveProgram, LiveSession, ManimAxesOptions, ManimGeometryOptions, Mobject,
    RateFunction, Scene, SemanticAnimationCompositionKind as Kind, Text, TransformToRequest, BLUE,
    GREEN, ORANGE, WHITE,
};

type BuildResult<T> = Result<T, Box<dyn std::error::Error>>;
pub const RUN_TIME: f64 = 6.0;
pub const RECORDINGS: [&[[f64; 2]]; 2] = [
    &[[0.0, 0.4], [0.5, 1.0], [2.0, 0.8], [5.0, 1.2], [10.0, 0.6]],
    &[[-1.0, 2.5], [1.5, 1.9], [4.0, 2.4], [8.0, 1.8], [12.0, 2.6]],
];

struct Drawing {
    marker: Mobject,
    segments: Vec<Mobject>,
    targets: Vec<Mobject>,
}

pub struct SynchronizedPlayback {
    drawings: Vec<Drawing>,
    cursor: Mobject,
    cursor_targets: Vec<Mobject>,
    durations: Vec<f64>,
    next_interval: usize,
}

impl LiveContinuation for SynchronizedPlayback {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let index = self.next_interval;
        if index == self.durations.len() {
            return Ok(ContinuationStep::Finished);
        }
        let options = AnimationOptions::new()
            .run_time(self.durations[index])
            .rate_func(RateFunction::Linear);
        let mut children = Vec::with_capacity(self.drawings.len() * 2 + 1);
        for drawing in &self.drawings {
            children.push(Request::Create {
                target: &drawing.segments[index],
                options,
            });
            children.push(Request::TransformTo(TransformToRequest::new(
                &drawing.marker,
                &drawing.targets[index],
                options,
            )));
        }
        children.push(Request::TransformTo(TransformToRequest::new(
            &self.cursor,
            &self.cursor_targets[index],
            options,
        )));
        let request = Request::Composition {
            kind: Kind::Parallel,
            children,
            options,
        };
        let segment = live
            .declare_and_activate_composition(&request, options)
            .map_err(|error| error.to_string())?;
        self.next_interval += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

fn color(options: &mut ManimGeometryOptions, color: Color) -> BuildResult<()> {
    options.set_color(color.red.into(), color.green.into(), color.blue.into(), 1.0)?;
    Ok(())
}
fn line(
    scene: &mut Scene,
    a: [f64; 2],
    b: [f64; 2],
    tint: Color,
    width: f64,
) -> BuildResult<Mobject> {
    let mut options = ManimGeometryOptions::line(a[0], a[1], b[0], b[1])?;
    color(&mut options, tint)?;
    options.set_stroke_width(width)?;
    Ok(scene.geometry(options)?)
}

/// Native and direct WASM call this same typed scene/continuation builder.
pub fn program() -> BuildResult<LiveProgram<SynchronizedPlayback>> {
    let mut scene = Scene::new();
    let axes = scene.axes(&ManimAxesOptions::new(
        [0.0, 10.0, 2.0],
        [0.0, 3.0, 1.0],
        10.0,
        4.0,
    ))?;
    let frame = axes.authored_frame()?;
    let data: Vec<Vec<TimedPlotSample>> = RECORDINGS
        .iter()
        .map(|row| {
            row.iter()
                .map(|&[time, value]| TimedPlotSample { time, value })
                .collect()
        })
        .collect();
    let refs: Vec<_> = data.iter().map(Vec::as_slice).collect();
    let plan = SynchronizedTimeSeriesPlan::new(frame, &refs, [0.0, 10.0], RUN_TIME)?;
    scene.add_many(&[axes.family().into()])?;
    for (axis, direction, exclude_zero) in [
        (frame.x(), (0.0, -1.0), false),
        (frame.y(), (-1.0, 0.0), true),
    ] {
        for label in number_labels(axis, None, 0, exclude_zero)? {
            let mut text = scene.text(Text::new(label.text).with_font_size(18.0))?;
            text.next_to_point(
                label.point[0],
                label.point[1],
                direction.0,
                direction.1,
                0.12,
            )?;
            scene.add(&text)?;
        }
    }
    for (source, size, x, y, tint) in [
        ("Two recordings, one data clock", 28.0, 0.0, 3.2, WHITE),
        (
            "Different sample times; piecewise-linear interpolation",
            17.0,
            0.0,
            2.72,
            WHITE,
        ),
        ("Data time (s)", 20.0, 0.0, -2.85, WHITE),
        ("Series A", 18.0, -2.0, 2.25, BLUE),
        ("Series B", 18.0, 2.0, 2.25, ORANGE),
    ] {
        let mut text = scene.text(Text::new(source).with_font_size(size).color(tint))?;
        text.move_to(x, y)?;
        scene.add(&text)?;
    }
    for row in plan.series() {
        let mut options = ManimGeometryOptions::sampled_plot(row.points())?;
        color(&mut options, WHITE)?;
        options.set_stroke_width(0.018)?;
        options.set_object_opacity(0.2)?;
        let reference = scene.geometry(options)?;
        scene.add(&reference)?;
    }
    let cursor = line(
        &mut scene,
        frame.coords_to_point(0.0, 0.0)?,
        frame.coords_to_point(0.0, 3.0)?,
        GREEN,
        0.025,
    )?;
    scene.add(&cursor)?;
    let mut drawings = Vec::new();
    for (row, tint) in plan.series().iter().zip([BLUE, ORANGE]) {
        let mut options = ManimGeometryOptions::circle(0.08)?;
        color(&mut options, tint)?;
        options.set_fill_opacity(1.0)?;
        options.set_stroke_width(0.0)?;
        let mut marker = scene.geometry(options)?;
        marker.move_to(row.points()[0][0], row.points()[0][1])?;
        scene.add(&marker)?;
        let mut segments = Vec::new();
        let mut targets = Vec::new();
        for pair in row.points().windows(2) {
            segments.push(line(&mut scene, pair[0], pair[1], tint, 0.04)?);
            let mut target = marker.copy_handle()?;
            target.move_to(pair[1][0], pair[1][1])?;
            targets.push(target);
        }
        drawings.push(Drawing {
            marker,
            segments,
            targets,
        });
    }
    let mut cursor_targets = Vec::new();
    for &[x, y] in &plan.cursor_points()[1..] {
        let mut target = cursor.copy_handle()?;
        target.move_to(x, y)?;
        cursor_targets.push(target);
    }
    Ok(scene.into_live_program(SynchronizedPlayback {
        drawings,
        cursor,
        cursor_targets,
        durations: plan.durations().to_vec(),
        next_interval: 0,
    })?)
}

#[cfg(test)]
mod tests;
