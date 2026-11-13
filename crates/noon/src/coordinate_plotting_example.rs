//! Paired plotting example shared unchanged by native and direct Rust/WASM.
//! Samples are illustrative, not navigation simulation results.

use crate::{
    AnimationCompositionRequest as Request, AnimationOptions, ContinuationEffects,
    ContinuationStep, LiveContinuation, LiveProgram, ManimAxes, ManimAxesOptions, Mobject,
    RateFunction, Scene, SceneExecution, SemanticAnimationCompositionKind as Kind, Text, BLUE,
    YELLOW,
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
    let label = scene.text(Text::new("Time (s)").with_font_size(22.0))?;
    crate::LayoutAnchor::from(&label).next_to_edge_in_frame(
        &crate::LayoutAnchor::from(axes.x_axis()?.family()),
        (1.0, 0.0),
        crate::ManimNextToArgs {
            direction: (1.0, 0.0),
            buff: 0.2,
            aligned_edge: (0.0, 0.0),
            mask: (1.0, 1.0),
        },
        f64::from(noon_core::MED_SMALL_BUFF),
    )?;
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
    const EFFECTS: ContinuationEffects = ContinuationEffects::SceneOnly;
    type Error = String;

    fn resume(&mut self, scene: &mut crate::ContinuationScene) -> Result<ContinuationStep, String> {
        let live = &mut scene.live();
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

pub fn session() -> Result<SceneExecution, Box<dyn std::error::Error>> {
    Ok(scene()?.into_execution()?)
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

    #[cfg(feature = "prepared-prefix")]
    #[test]
    fn prepared_plot_retains_curve_reveal_and_paused_history_across_completion() {
        use crate::PreparedSourceStatus;
        use noon_runtime::ReplayLimits;

        fn append(
            source: &mut crate::PreparedSource<CoordinatePlayback>,
            reader: &mut crate::PrefixReader,
        ) {
            assert_eq!(
                source.prepare_next().unwrap(),
                PreparedSourceStatus::BatchReady
            );
            let batch = source.pending_batch().unwrap();
            reader.reserve_append_capacity(batch).unwrap();
            reader.preflight_append(batch).unwrap();
            reader.append(source.take_batch().unwrap()).unwrap();
        }

        let mut source = program()
            .unwrap()
            .into_prepared_source(ReplayLimits::default())
            .unwrap();
        assert_eq!(
            source.prepare_next().unwrap(),
            PreparedSourceStatus::ReaderReady
        );
        let mut reader = source.take_reader().unwrap();
        while reader.frontier() < 3.3 {
            append(&mut source, &mut reader);
        }
        reader.seek(2.3).unwrap();
        let midpoint = reader.frame().clone();
        let curve = midpoint
            .objects
            .iter()
            .position(|object| object.style.stroke == Some(BLUE))
            .unwrap();
        assert!((midpoint.reveal(curve) - 0.5).abs() < 1e-6);
        let publication = reader.publication_context();
        while reader.end_time().is_none() {
            append(&mut source, &mut reader);
            assert_eq!(reader.frame(), &midpoint);
            assert_eq!(reader.publication_context(), publication);
        }
        reader.seek(RUN_TIME).unwrap();
        assert_eq!(reader.frame().reveal(curve), 1.0);
        reader.seek(2.3).unwrap();
        let frame = reader.frame();
        let retained = midpoint.objects.len();
        assert_eq!(frame.time, midpoint.time);
        assert_eq!(&frame.objects[..retained], midpoint.objects);
        assert_eq!(&frame.presences[..retained], midpoint.presences);
        assert_eq!(&frame.reveals[..retained], midpoint.reveals);
        assert!(frame.presences[retained..].iter().all(|present| !present));
    }
}
