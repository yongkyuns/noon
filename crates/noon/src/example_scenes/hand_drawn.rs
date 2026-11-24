//! Native Rust counterpart of the Python hand-drawn diagram.
use crate::{
    AnimationCompositionRequest as Animation, AnimationOptions, Color, ContinuationEffects,
    ContinuationScene, ContinuationStep, HatchPaint, HatchPaintOptions, LiveContinuation,
    LiveProgram, ManimArrow, ManimArrowOptions, ManimGeometryOptions, Mobject, RateFunction, Scene,
    ScriptError, BLUE, TEAL, YELLOW,
};

pub struct HandDrawnDiagram {
    pens: [Mobject; 3],
    arrow: ManimArrow,
    step: u8,
}

impl LiveContinuation for HandDrawnDiagram {
    const EFFECTS: ContinuationEffects = ContinuationEffects::SceneOnly;
    type Error = ScriptError;

    fn resume(&mut self, scene: &mut ContinuationScene) -> Result<ContinuationStep, ScriptError> {
        let segment = match self.step {
            0 => {
                let options = AnimationOptions::new();
                let mut animations = self
                    .pens
                    .iter()
                    .map(|target| Animation::Create { target, options })
                    .collect::<Vec<_>>();
                animations.insert(
                    1,
                    Animation::FamilyReveal {
                        target: self.arrow.family(),
                        reverse: false,
                        options,
                    },
                );
                scene.live().declare_and_activate_composition(
                    &Animation::Composition {
                        kind: crate::SemanticAnimationCompositionKind::Parallel,
                        children: animations,
                        options,
                    },
                    options.run_time(2.4).rate_func(RateFunction::Linear),
                )?
            }
            1 => scene.live().wait_segment(0.6)?,
            _ => return Ok(ContinuationStep::Finished),
        };
        self.step += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> Result<LiveProgram<HandDrawnDiagram>, ScriptError> {
    let mut scene = Scene::new();
    let mut pen = |mut options: ManimGeometryOptions, roughness, seed, color: Color| {
        options.set_sketch_stroke(color, roughness, seed)?;
        scene.geometry(options)
    };
    let mut panel = pen(ManimGeometryOptions::rectangle(2.6, 1.6)?, 1.25, 8, TEAL)?;
    panel.set_translation(-2.5, 0.0)?;
    panel.set_fill_hatch(
        HatchPaint::new(
            TEAL,
            Color::rgba(0.0, 0.0, 0.0, 0.0),
            HatchPaintOptions {
                spacing: 0.24,
                width: 0.028,
                angle: std::f64::consts::FRAC_PI_4,
                phase: [0.0; 2],
                cross: false,
            },
        )
        .expect("valid demo fill"),
        Some(0.55),
    )?;
    let mut circle = pen(ManimGeometryOptions::circle(0.82)?, 1.45, 20, BLUE)?;
    circle.set_translation(2.25, 0.0)?;
    let underline = pen(
        ManimGeometryOptions::line(-3.55, -1.12, -1.45, -1.12)?,
        1.0,
        25,
        TEAL,
    )?;
    let mut arrow = ManimArrowOptions::arrow(-0.9, 0.0, 1.15, 0.0)?;
    arrow.set_buff(0.0)?;
    arrow.set_sketch_stroke(YELLOW, 1.8, 3987)?;
    let arrow = scene.manim_arrow(arrow)?;
    let mut frame = scene.rectangle(8.8, 3.7)?;
    frame.set_stroke_opacity(0.25)?;
    scene.add(&frame)?;
    Ok(scene.into_live_program(HandDrawnDiagram {
        pens: [panel, circle, underline],
        arrow,
        step: 0,
    })?)
}

#[cfg(all(test, feature = "prepared-prefix"))]
#[test]
fn sketch_create_retains_analytic_geometry_and_stroke_when_scrubbed() {
    use crate::{GeometryRef, PlaybackProgram, RustHostCallbackTable};

    let mut player = program().unwrap().into_playback().unwrap();
    let mut callbacks = RustHostCallbackTable::new();
    for _ in 0..8 {
        if player.end_time().is_some() {
            break;
        }
        player.prepare_until(&mut callbacks, 4.0).unwrap();
    }
    assert_eq!(player.end_time(), Some(3.0));

    let sample = |player: &PlaybackProgram<HandDrawnDiagram>| {
        let frame = player.session().unwrap().frame();
        let (index, object) = frame
            .objects
            .iter()
            .enumerate()
            .find(|(_, object)| {
                object
                    .fill_paint
                    .as_ref()
                    .is_some_and(|paint| paint.as_hatch().is_some())
            })
            .unwrap();
        assert!(matches!(
            object.geometry(),
            Some(GeometryRef::Rectangle { .. })
        ));
        (frame.reveal(index), object.style.sketch_stroke.unwrap())
    };

    player.advance_to(&mut callbacks, 1.2).unwrap();
    let (mid, stroke) = sample(&player);
    assert!((mid - 0.5).abs() < 1e-6);
    player.advance_to(&mut callbacks, 2.7).unwrap();
    assert_eq!(sample(&player).0, 1.0);
    player.seek(0.3).unwrap();
    assert!(sample(&player).0 < mid);
    player.seek(1.2).unwrap();
    let (scrubbed, scrubbed_stroke) = sample(&player);
    assert_eq!(mid, scrubbed);
    assert_eq!(stroke, scrubbed_stroke);
    player.advance_to(&mut callbacks, 2.7).unwrap();
    let (resumed, resumed_stroke) = sample(&player);
    assert_eq!(resumed, 1.0);
    assert_eq!(stroke, resumed_stroke);
}
