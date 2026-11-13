//! Construct coordinates after a wait, move them, then plot in the new frame.
//! Native and direct WASM use this same ordinary live continuation.
use crate::{
    AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram, ManimAxes, ManimAxesOptions,
    ManimGeometryOptions, ManimNumberLineOptions, Mobject, RateFunction, Scene, Text, BLUE, GREEN,
    ORANGE,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub const RUN_TIME: f64 = 1.5;
pub struct LiveCoordinatePlayback {
    phase: u8,
    axes: Option<ManimAxes>,
    sentinel: Mobject,
}

impl LiveContinuation for LiveCoordinatePlayback {
    type Error = String;
    fn resume(
        &mut self,
        scene: &mut crate::ContinuationScene,
    ) -> std::result::Result<ContinuationStep, String> {
        self.step(scene).map_err(|error| error.to_string())
    }
}
impl LiveCoordinatePlayback {
    fn step(&mut self, scene: &mut crate::ContinuationScene) -> Result<ContinuationStep> {
        let phase = self.phase;
        self.phase += 1;
        let options = |duration| {
            AnimationOptions::new()
                .run_time(duration)
                .rate_func(RateFunction::Linear)
        };
        match phase {
            0 => Ok(ContinuationStep::Await(scene.live().wait_segment(0.25)?)),
            1 => {
                let axes = scene.axes(&ManimAxesOptions::new(
                    [-2.0, 2.0, 1.0],
                    [-1.0, 1.0, 1.0],
                    8.0,
                    3.0,
                ))?;
                scene.shift_family(axes.family(), 0.0, -0.5)?;
                let mut line_options = ManimNumberLineOptions::new([0.0, 4.0, 1.0]);
                line_options.length = Some(4.0);
                line_options.style.stroke = Some(crate::SemanticPaint::Solid(ORANGE));
                let line = scene.number_line(&line_options)?;
                scene.shift_family(line.family(), 0.0, 2.0)?;
                scene.add_many(&[axes.family().into(), line.family().into()])?;
                let target = scene.live().copy_family(axes.family())?;
                scene.shift_family(target.root(), 0.5, 0.0)?;
                let segment = scene.live().declare_and_activate_family_transform_to(
                    axes.family(),
                    target.root(),
                    options(0.5),
                )?;
                self.axes = Some(axes);
                Ok(ContinuationStep::Await(segment))
            }
            2 => {
                let axes = self.axes.as_ref().expect("axes created after wait");
                let frame = scene.effective_axes_frame(axes)?;
                let sampling = axes.plot_sampling(Some(&[-2.0, 2.0, 0.5]))?;
                let mut geometry =
                    ManimGeometryOptions::axes_function_plot(frame, &sampling, |x| 0.5 * x, false)?;
                geometry.set_color(BLUE.red.into(), BLUE.green.into(), BLUE.blue.into(), 1.0)?;
                geometry.set_stroke_width(0.04)?;
                let graph = scene.geometry(geometry)?;
                Ok(ContinuationStep::Await(
                    scene
                        .live()
                        .declare_and_activate_create(&graph, options(0.5))?,
                ))
            }
            3 => Ok(ContinuationStep::Await(scene.live().wait_segment(0.25)?)),
            _ => {
                let state = scene.effective(&self.sentinel)?;
                assert!((state.transform.translation.x + 5.0).abs() < 2e-5);
                Ok(ContinuationStep::Finished)
            }
        }
    }
}

pub fn program() -> Result<LiveProgram<LiveCoordinatePlayback>> {
    let mut scene = Scene::new();
    let mut title =
        scene.text(Text::new("Create axes after playback starts").with_font_size(26.0))?;
    title.move_to(0.0, 3.0)?;
    let mut options = ManimGeometryOptions::circle(0.12)?;
    options.set_color(GREEN.red.into(), GREEN.green.into(), GREEN.blue.into(), 1.0)?;
    options.set_fill_opacity(1.0)?;
    options.set_stroke_width(0.0)?;
    let mut sentinel = scene.geometry(options)?;
    sentinel.move_to(-5.0, 2.6)?;
    scene.add_many(&[(&title).into(), (&sentinel).into()])?;
    Ok(scene.into_live_program(LiveCoordinatePlayback {
        phase: 0,
        axes: None,
        sentinel,
    })?)
}
