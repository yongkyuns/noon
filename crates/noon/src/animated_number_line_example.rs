//! Animated NumberLine with native labels and effective post-transform queries.
use crate::plot_presentation::NumberLabelOptions;
use crate::{
    AnimationCompositionRequest as Request, AnimationOptions, ContinuationStep, FadeEndpoint,
    LiveContinuation, LiveProgram, ManimGeometryOptions, ManimNumberLine, ManimNumberLineOptions,
    Mobject, RateFunction, Scene, SemanticAnimationCompositionKind as Kind, SemanticFadeDirection,
    Text, TransformToRequest, YELLOW,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub const RUN_TIME: f64 = 8.8;

pub struct NumberLinePlayback {
    line: ManimNumberLine,
    marker: Mobject,
    title: Mobject,
    caption: Mobject,
    phase: usize,
}

impl LiveContinuation for NumberLinePlayback {
    type Error = String;
    fn resume(
        &mut self,
        scene: &mut crate::ContinuationScene,
    ) -> std::result::Result<ContinuationStep, String> {
        self.step(scene).map_err(|error| error.to_string())
    }
}

impl NumberLinePlayback {
    fn step(&mut self, scene: &mut crate::ContinuationScene) -> Result<ContinuationStep> {
        let durations = [0.5, 1.0, 0.5, 1.5, 0.5, 1.2, 0.5, 0.8, 1.5, 0.8];
        let Some(&duration) = durations.get(self.phase) else {
            return Ok(ContinuationStep::Finished);
        };
        let options = AnimationOptions::new()
            .run_time(duration)
            .rate_func(RateFunction::Linear);
        let segment = match self.phase {
            0 => scene
                .live()
                .declare_and_activate_text_write(&self.title, false, options)?,
            1 => scene.live().declare_and_activate_family_reveal(
                self.line.family(),
                false,
                options,
            )?,
            2 => scene.live().declare_and_activate_composition(
                &Request::Composition {
                    kind: Kind::Parallel,
                    children: vec![
                        Request::Fade {
                            target: &self.marker,
                            direction: SemanticFadeDirection::In,
                            endpoint: FadeEndpoint::default(),
                            options,
                        },
                        Request::TextWrite {
                            target: &self.caption,
                            reverse_member_order: false,
                            options,
                        },
                    ],
                    options,
                },
                options,
            )?,
            3 | 5 | 8 => {
                let value = match self.phase {
                    3 => 0.0,
                    5 => 3.0,
                    _ => -2.0,
                };
                let frame = scene.effective_number_line_frame(&self.line)?;
                let [x, y] = frame.number_to_point(value)?;
                let target = scene.target_editor(&self.marker)?;
                scene.live().set_translation(&target, x, y)?;
                scene
                    .live()
                    .declare_and_activate_transform_to(&self.marker, &target, options)?
            }
            7 => {
                let line_target = scene.copy_family(self.line.family())?;
                scene.shift_family(line_target.root(), 0.0, 0.6)?;
                let marker_target = scene.target_editor(&self.marker)?;
                scene.live().shift(&marker_target, 0.0, 0.6)?;
                scene.live().declare_and_activate_composition(
                    &Request::Composition {
                        kind: Kind::Parallel,
                        children: vec![
                            Request::FamilyTransformTo {
                                source: self.line.family(),
                                target_state: line_target.root(),
                                options,
                            },
                            Request::TransformTo(TransformToRequest::new(
                                &self.marker,
                                &marker_target,
                                options,
                            )),
                        ],
                        options,
                    },
                    options,
                )?
            }
            _ => scene.live().wait_segment(duration)?,
        };
        self.phase += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> Result<LiveProgram<NumberLinePlayback>> {
    let mut scene = Scene::new();
    let mut title =
        scene.text(Text::new("NumberLine: position and direction").with_font_size(28.0))?;
    title.shift(0.0, 3.0)?;
    let mut caption = scene.text(Text::new("Coordinates follow the line").with_font_size(22.0))?;
    caption.shift(0.0, 2.0)?;
    let mut options = ManimNumberLineOptions::new([-4.0, 4.0, 1.0]);
    options.length = Some(8.8);
    let line = scene.number_line(&options)?;
    line.add_numbers(
        None,
        &NumberLabelOptions {
            exclude_zero: false,
            ..Default::default()
        },
    )?;
    let [x, y] = line.authored_frame()?.number_to_point(-4.0)?;
    let mut options = ManimGeometryOptions::dot(x, y, 0.11)?;
    options.set_color(
        YELLOW.red.into(),
        YELLOW.green.into(),
        YELLOW.blue.into(),
        1.0,
    )?;
    let marker = scene.geometry(options)?;
    Ok(scene.into_live_program(NumberLinePlayback {
        line,
        marker,
        title,
        caption,
        phase: 0,
    })?)
}
