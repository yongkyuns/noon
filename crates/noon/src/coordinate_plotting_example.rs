//! Paired plotting example shared unchanged by native and direct Rust/WASM.
//! Samples are illustrative, not navigation simulation results.

use crate::{
    AnimationCompositionRequest as Request, AnimationOptions, ContinuationStep, ExecutionSession,
    LiveContinuation, LiveProgram, LiveSession, ManimAxes, ManimAxesOptions, Mobject, RateFunction,
    Scene, SemanticAnimationCompositionKind as Kind, Text, BLUE, YELLOW,
};

struct PlotObjects {
    axes: ManimAxes,
    curve: Mobject,
    data: Mobject,
    title: Mobject,
    label: Mobject,
}

fn objects(scene: &mut Scene) -> Result<PlotObjects, Box<dyn std::error::Error>> {
    let axes = scene.axes(&ManimAxesOptions::new(
        [0.0, 10.0, 2.0],
        [-1.5, 1.5, 0.5],
        10.0,
        4.0,
    ))?;
    let mut curve = axes.plot(|t| (t * 0.8).sin(), Some(&[0.0, 10.0, 0.05]), true)?;
    curve.set_color(BLUE.red.into(), BLUE.green.into(), BLUE.blue.into(), 1.0)?;
    let samples = [
        [0.0, 0.1],
        [2.0, 0.95],
        [4.0, -0.1],
        [6.0, -0.9],
        [8.0, 0.2],
        [10.0, 1.0],
    ];
    let mut data = axes.plot_samples(&samples)?;
    data.set_color(
        YELLOW.red.into(),
        YELLOW.green.into(),
        YELLOW.blue.into(),
        1.0,
    )?;
    let mut title =
        scene.text(Text::new("Shared axes: function and sampled data").with_font_size(28.0))?;
    title.shift(0.0, 3.0)?;
    let mut label = scene.text(Text::new("Time (s)").with_font_size(22.0))?;
    label.shift(0.0, -2.7)?;
    Ok(PlotObjects {
        axes,
        curve,
        data,
        title,
        label,
    })
}

pub fn scene() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let objects = objects(&mut scene)?;
    scene.add_many(&[
        objects.axes.family().into(),
        (&objects.curve).into(),
        (&objects.data).into(),
        (&objects.title).into(),
        (&objects.label).into(),
    ])?;
    Ok(scene)
}

pub const RUN_TIME: f64 = 5.1;

pub struct CoordinatePlayback {
    objects: PlotObjects,
    phase: usize,
}

impl LiveContinuation for CoordinatePlayback {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let duration = match self.phase {
            0 => 0.5,
            1 => 0.8,
            2 => 2.0,
            3 => 1.0,
            4 => 0.8,
            _ => return Ok(ContinuationStep::Finished),
        };
        let options = AnimationOptions::new()
            .run_time(duration)
            .rate_func(RateFunction::Linear);
        let objects = &self.objects;
        let segment = match self.phase {
            0 => live.declare_and_activate_text_write(&objects.title, false, options),
            1 => live.declare_and_activate_composition(
                &Request::Composition {
                    kind: Kind::Parallel,
                    children: vec![
                        Request::FamilyReveal {
                            target: objects.axes.family(),
                            reverse: false,
                            options,
                        },
                        Request::TextWrite {
                            target: &objects.label,
                            reverse_member_order: false,
                            options,
                        },
                    ],
                    options,
                },
                options,
            ),
            2 => live.declare_and_activate_create(&objects.curve, options),
            3 => live.declare_and_activate_create(&objects.data, options),
            4 => live.wait_segment(duration),
            _ => unreachable!(),
        }
        .map_err(|error| error.to_string())?;
        self.phase += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

/// Native and direct WASM run this same typed continuation; Python is optional.
pub fn program() -> Result<LiveProgram<CoordinatePlayback>, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let objects = objects(&mut scene)?;
    Ok(scene.into_live_program(CoordinatePlayback { objects, phase: 0 })?)
}

pub fn session() -> Result<ExecutionSession, Box<dyn std::error::Error>> {
    Ok(scene()?.execution_session()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_plot_is_static_and_reuses_its_retained_resources() {
        let scene = scene().unwrap();
        let revision = scene.revision();
        let mut session = scene.execution_session().unwrap();
        let object_count = session.frame().objects.len();
        assert_eq!(object_count, 17);
        assert!(!session.has_required_callbacks());
        session.take_renderer_publication();
        session.seek(0.75).unwrap();
        assert_eq!(session.frame().objects.len(), object_count);
        assert_eq!(scene.revision(), revision);
    }
}
