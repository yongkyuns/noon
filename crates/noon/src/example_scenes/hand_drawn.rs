//! Native Rust counterpart of the Python hand-drawn diagram.
use crate::{
    AnimationOptions, Color, ContinuationEffects, ContinuationScene, ContinuationStep,
    HandDrawnShape as Shape, HatchPaint, HatchPaintOptions, LiveContinuation, LiveProgram,
    ManimGeometryOptions, Mobject, RateFunction, Scene, ScriptError, BLUE, TEAL, YELLOW,
};

pub struct HandDrawnDiagram {
    pens: [Mobject; 4],
    step: u8,
}

impl LiveContinuation for HandDrawnDiagram {
    const EFFECTS: ContinuationEffects = ContinuationEffects::SceneOnly;
    type Error = ScriptError;

    fn resume(&mut self, scene: &mut ContinuationScene) -> Result<ContinuationStep, ScriptError> {
        let segment = match self.step {
            0 => scene.live().declare_and_activate_create_parallel(
                &self
                    .pens
                    .each_ref()
                    .map(|pen| (pen, AnimationOptions::new())),
                AnimationOptions::new()
                    .run_time(2.4)
                    .rate_func(RateFunction::Linear),
            )?,
            1 => scene.live().wait_segment(0.6)?,
            _ => return Ok(ContinuationStep::Finished),
        };
        self.step += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> Result<LiveProgram<HandDrawnDiagram>, ScriptError> {
    let mut scene = Scene::new();
    let mut pen = |shape, roughness, seed, color: Color| {
        let mut options = ManimGeometryOptions::hand_drawn(shape, roughness, seed)?;
        options.set_stroke_color(color.red.into(), color.green.into(), color.blue.into(), 1.0)?;
        scene.geometry(options)
    };
    let mut panel = pen(
        Shape::Rectangle {
            width: 2.6,
            height: 1.6,
        },
        1.25,
        8,
        TEAL,
    )?;
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
    let arrow = pen(
        Shape::Arrow {
            start: (-0.9, 0.0),
            end: (1.15, 0.0),
        },
        1.1,
        12,
        YELLOW,
    )?;
    let mut circle = pen(Shape::Circle { radius: 0.82 }, 1.45, 20, BLUE)?;
    circle.set_translation(2.25, 0.0)?;
    let underline = pen(
        Shape::Line {
            start: (-3.55, -1.12),
            end: (-1.45, -1.12),
        },
        1.0,
        25,
        TEAL,
    )?;
    let mut frame = scene.rectangle(8.8, 3.7)?;
    frame.set_stroke_opacity(0.25)?;
    scene.add(&frame)?;
    Ok(scene.into_live_program(HandDrawnDiagram {
        pens: [panel, arrow, circle, underline],
        step: 0,
    })?)
}

#[cfg(all(test, feature = "prepared-prefix"))]
#[test]
fn sketch_create_reuses_the_same_geometry_when_scrubbed() {
    use crate::{GeometryRef, PlaybackProgram, RustHostCallbackTable};
    use std::sync::Arc;

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
        let Some(GeometryRef::VectorPath(path)) = object.geometry() else {
            panic!("sketch must use an ordinary retained path");
        };
        (frame.reveal(index), path.clone())
    };

    player.advance_to(&mut callbacks, 1.2).unwrap();
    let (mid, path) = sample(&player);
    assert!((mid - 0.5).abs() < 1e-6);
    player.advance_to(&mut callbacks, 2.7).unwrap();
    assert_eq!(sample(&player).0, 1.0);
    player.seek(0.3).unwrap();
    assert!(sample(&player).0 < mid);
    player.seek(1.2).unwrap();
    let (scrubbed, scrubbed_path) = sample(&player);
    assert_eq!(mid, scrubbed);
    assert!(Arc::ptr_eq(&path, &scrubbed_path));
    player.advance_to(&mut callbacks, 2.7).unwrap();
    let (resumed, resumed_path) = sample(&player);
    assert_eq!(resumed, 1.0);
    assert!(Arc::ptr_eq(&path, &resumed_path));
}
