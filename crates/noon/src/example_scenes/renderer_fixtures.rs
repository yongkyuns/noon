//! Filled morph and reveal-continuity examples shared by native and direct WASM.
use crate::{
    AnimationCompositionRequest, AnimationOptions, Color, ContinuationStep, ExecutionSession,
    LiveContinuation, LiveProgram, LiveSession, Mobject, RateFunction, Scene, SemanticPaint,
    SemanticStyle, TransformToRequest, Vec2, VectorPath, BLUE, PINK, PURPLE, WHITE,
};

fn style(fill: Option<Color>, stroke: Color, width: f64) -> SemanticStyle {
    SemanticStyle {
        fill: fill.map(SemanticPaint::Solid),
        stroke: Some(SemanticPaint::Solid(stroke)),
        stroke_width: width,
        stroke_width_mode: crate::StrokeWidthMode::ScreenSpace,
        stroke_join: crate::StrokeJoin::Miter,
        stroke_cap: crate::StrokeCap::Butt,
        ..SemanticStyle::default()
    }
}

fn paint(
    object: &mut Mobject,
    fill: Option<Color>,
    stroke: Color,
    width: f64,
) -> Result<(), String> {
    let mut state = object.state().map_err(|error| error.to_string())?;
    state.style = style(fill, stroke, width);
    object
        .commit_state(state)
        .map_err(|error| error.to_string())
}

fn options() -> AnimationOptions {
    AnimationOptions::new()
        .run_time(3.2)
        .rate_func(RateFunction::EaseInOutCubic)
}

fn rounded_loop(radius: f32) -> VectorPath {
    let handle = radius * 0.58;
    VectorPath::new()
        .move_to(Vec2::new(0.0, radius))
        .cubic_to(
            Vec2::new(handle, radius),
            Vec2::new(radius, handle),
            Vec2::new(radius, 0.0),
        )
        .cubic_to(
            Vec2::new(radius, -handle),
            Vec2::new(handle, -radius),
            Vec2::new(0.0, -radius),
        )
        .cubic_to(
            Vec2::new(-handle, -radius),
            Vec2::new(-radius, -handle),
            Vec2::new(-radius, 0.0),
        )
        .cubic_to(
            Vec2::new(-radius, handle),
            Vec2::new(-handle, radius),
            Vec2::new(0.0, radius),
        )
        .close()
}

fn star(outer: f32, inner: f32, phase: f32) -> VectorPath {
    let mut star = VectorPath::new();
    for index in 0..10 {
        let angle = phase + std::f32::consts::FRAC_PI_2 - index as f32 * std::f32::consts::PI / 5.0;
        let radius = if index % 2 == 0 { outer } else { inner };
        let point = Vec2::new(angle.cos() * radius, angle.sin() * radius);
        star = if index == 0 {
            star.move_to(point)
        } else {
            star.line_to(point)
        };
    }
    star.close()
}

/// Pair: `web/python/examples/ordinary_filled_path_transform.py`.
pub fn filled_path_transform() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let shape = scene
        .path(rounded_loop(1.35), style(Some(BLUE), WHITE, 0.08))
        .map_err(|error| error.to_string())?;
    let target = scene
        .path(star(1.7, 0.7, 0.0), style(Some(PURPLE), WHITE, 0.08))
        .map_err(|error| error.to_string())?;
    scene.add(&shape).map_err(|error| error.to_string())?;
    let mut session = scene
        .execution_session()
        .map_err(|error| error.to_string())?;
    scene
        .live(&mut session)
        .declare_and_activate_composition(
            &AnimationCompositionRequest::TransformTo(TransformToRequest::point_correspondence(
                &shape,
                &target,
                options(),
            )),
            AnimationOptions::new(),
        )
        .map_err(|error| error.to_string())?;
    Ok(session)
}

/// One retained path concurrently follows point correspondence and Create reveal.
/// Pair: `web/python/examples/ordinary_morph_reveal.py`.
pub fn morph_reveal() -> Result<LiveProgram<MorphReveal>, String> {
    let mut scene = Scene::new();
    let source = VectorPath::new()
        .move_to(Vec2::new(-2.4, -0.8))
        .cubic_to(
            Vec2::new(-2.0, 2.5),
            Vec2::new(0.8, -2.4),
            Vec2::new(1.0, 0.2),
        )
        .line_to(Vec2::new(2.4, 1.0));
    let target = VectorPath::new()
        .move_to(Vec2::new(-2.4, -0.8))
        .cubic_to(
            Vec2::new(-0.5, -2.6),
            Vec2::new(0.4, 2.8),
            Vec2::new(1.0, 0.2),
        )
        .line_to(Vec2::new(2.0, -1.4));
    let shape = scene
        .path(source, style(None, BLUE, 0.09))
        .map_err(|error| error.to_string())?;
    let target = scene
        .path(target, style(None, PINK, 0.09))
        .map_err(|error| error.to_string())?;
    scene
        .into_live_program(MorphReveal {
            shape,
            target,
            started: false,
        })
        .map_err(|error| error.to_string())
}

/// Complete the same source-owned segment as the paired Python Scene.play.
/// Sampling an uncompleted ExecutionSession at its endpoint would retain the
/// interpolated mesh instead of publishing the authored target geometry.
pub struct MorphReveal {
    shape: Mobject,
    target: Mobject,
    started: bool,
}

impl LiveContinuation for MorphReveal {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        if self.started {
            return Ok(ContinuationStep::Finished);
        }
        let options = AnimationOptions::new()
            .run_time(3.0)
            .rate_func(RateFunction::Linear);
        let request = AnimationCompositionRequest::Composition {
            kind: crate::SemanticAnimationCompositionKind::Parallel,
            children: vec![
                AnimationCompositionRequest::Create {
                    target: &self.shape,
                    options,
                },
                AnimationCompositionRequest::TransformTo(TransformToRequest::point_correspondence(
                    &self.shape,
                    &self.target,
                    options,
                )),
            ],
            options: AnimationOptions::new(),
        };
        let segment = live
            .declare_and_activate_composition(&request, AnimationOptions::new())
            .map_err(|error| error.to_string())?;
        self.started = true;
        Ok(ContinuationStep::Await(segment))
    }
}

/// Pair: `web/python/examples/ordinary_create_shapes.py`.
pub fn create_shapes() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let mut circle = scene.circle(0.9).map_err(|error| error.to_string())?;
    circle
        .set_translation(-3.0, 1.0)
        .map_err(|error| error.to_string())?;
    paint(&mut circle, Some(BLUE), WHITE, 0.055)?;
    let mut square = scene.square(1.7).map_err(|error| error.to_string())?;
    square
        .set_translation(0.0, 1.0)
        .map_err(|error| error.to_string())?;
    paint(&mut square, Some(PINK), WHITE, 0.055)?;
    let mut line = scene
        .line((1.75, 1.0), (4.25, 1.0))
        .map_err(|error| error.to_string())?;
    paint(&mut line, None, BLUE, 0.055)?;
    let wave = VectorPath::new().move_to(Vec2::new(-2.4, -1.6)).cubic_to(
        Vec2::new(-1.2, -2.6),
        Vec2::new(1.2, -0.6),
        Vec2::new(2.4, -1.6),
    );
    let wave = scene
        .path(wave, style(None, PINK, 0.05))
        .map_err(|error| error.to_string())?;
    let mut session = scene
        .execution_session()
        .map_err(|error| error.to_string())?;
    scene
        .live(&mut session)
        .declare_and_activate_create_parallel(
            &[
                (&circle, options()),
                (&square, options()),
                (&line, options()),
                (&wave, options()),
            ],
            AnimationOptions::new(),
        )
        .map_err(|error| error.to_string())?;
    Ok(session)
}

/// Mixed Text and analytic Circle family Create used by renderer qualifications.
#[cfg(feature = "native-text")]
pub fn mixed_family_reveal() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let mut text = scene.text("AB").map_err(|error| error.to_string())?;
    text.set_translation(-1.5, 0.0)
        .map_err(|error| error.to_string())?;
    let mut circle = scene.circle(0.9).map_err(|error| error.to_string())?;
    circle
        .set_translation(1.5, 0.0)
        .map_err(|error| error.to_string())?;
    paint(&mut circle, Some(BLUE), WHITE, 0.055)?;
    let family = scene
        .family(&[(&text).into(), (&circle).into()])
        .map_err(|error| error.to_string())?;
    scene
        .add_many(&[
            crate::MobjectTarget::Object(&text),
            crate::MobjectTarget::Object(&circle),
        ])
        .map_err(|error| error.to_string())?;
    let mut session = scene
        .execution_session()
        .map_err(|error| error.to_string())?;
    scene
        .live(&mut session)
        .declare_and_activate_family_reveal(
            &family,
            false,
            AnimationOptions::new()
                .run_time(3.2)
                .rate_func(RateFunction::Linear),
        )
        .map_err(|error| error.to_string())?;
    Ok(session)
}

/// A bounded, repeated path-morph workload. Python uses 96 objects in the shared
/// authoring gate; the native/WASM renderer gate uses 1,000 with the same semantics.
/// Pair: `web/python/examples/ordinary_morph_stress.py`.
pub fn morph_stress(count: usize) -> Result<ExecutionSession, String> {
    if !(12..=10_000).contains(&count) {
        return Err("morph object count must be between 12 and 10000".into());
    }
    let mut scene = Scene::new();
    let columns = (count as f64 * 1.5).sqrt().ceil() as usize;
    let rows = count.div_ceil(columns);
    let dx = 5.8 / (columns - 1).max(1) as f64;
    let dy = 3.8 / (rows - 1).max(1) as f64;
    let radius = (dx.min(dy) * 0.37) as f32;
    let width = f64::from(radius * 0.24).max(0.0025);
    let colors = [
        BLUE,
        crate::TEAL,
        crate::GREEN,
        crate::YELLOW,
        crate::ORANGE,
        crate::RED,
        PINK,
        PURPLE,
    ];
    let source = rounded_loop(radius);
    let targets: Vec<_> = (0..12)
        .map(|variant| {
            let outer = radius * (1.18 + 0.08 * (variant as f32 * 1.7).sin());
            let inner = outer * (0.42 + 0.05 * (variant as f32 * 0.9).cos());
            star(
                outer,
                inner,
                variant as f32 / 12.0 * std::f32::consts::PI * 0.36,
            )
        })
        .collect();
    let mut pairs = Vec::with_capacity(count);
    for index in 0..count {
        let paint = style(None, colors[index % colors.len()], width);
        let mut shape = scene
            .path(source.clone(), paint.clone())
            .map_err(|error| error.to_string())?;
        let mut target = scene
            .path(targets[index % targets.len()].clone(), paint)
            .map_err(|error| error.to_string())?;
        let x = -2.9 + (index % columns) as f64 * dx;
        let y = 1.9 - (index / columns) as f64 * dy;
        shape
            .set_translation(x, y)
            .map_err(|error| error.to_string())?;
        target
            .set_translation(x, y)
            .map_err(|error| error.to_string())?;
        scene.add(&shape).map_err(|error| error.to_string())?;
        pairs.push((shape, target));
    }
    let mut session = scene
        .execution_session()
        .map_err(|error| error.to_string())?;
    scene
        .live(&mut session)
        .declare_and_activate_composition(
            &AnimationCompositionRequest::Composition {
                kind: crate::SemanticAnimationCompositionKind::Parallel,
                children: pairs
                    .iter()
                    .map(|(shape, target)| {
                        AnimationCompositionRequest::TransformTo(
                            TransformToRequest::point_correspondence(
                                shape,
                                target,
                                options().run_time(3.4),
                            ),
                        )
                    })
                    .collect(),
                options: AnimationOptions::new(),
            },
            AnimationOptions::new(),
        )
        .map_err(|error| error.to_string())?;
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_morph_completion_publishes_target_content_without_replacing_identity() {
        let mut scene = Scene::new();
        let source = scene
            .path(rounded_loop(1.35), style(Some(BLUE), WHITE, 0.08))
            .unwrap();
        let target = scene
            .path(star(1.7, 0.7, 0.0), style(Some(PURPLE), WHITE, 0.08))
            .unwrap();
        scene.add(&source).unwrap();
        let identity = source.node_id();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_composition(
                &AnimationCompositionRequest::TransformTo(
                    TransformToRequest::point_correspondence(&source, &target, options()),
                ),
                AnimationOptions::new(),
            )
            .unwrap();
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        assert_eq!(source.node_id(), identity);
        assert_eq!(
            source.state().unwrap().content,
            target.state().unwrap().content
        );
        assert_eq!(source.state().unwrap().style, target.state().unwrap().style);
    }

    #[test]
    fn renderer_fixtures_advance_seek_and_keep_static_frames_clean() {
        for (build, count) in [
            (
                filled_path_transform as fn() -> Result<ExecutionSession, String>,
                1,
            ),
            (create_shapes, 4),
            (|| morph_stress(96), 96),
        ] {
            let mut forward = build().unwrap();
            let mut direct = build().unwrap();
            for time in [0.0, 0.8, 1.6, 3.199, 3.2, 3.4] {
                forward.advance_to(time).unwrap();
                direct.seek(time).unwrap();
                assert_eq!(forward.frame(), direct.frame());
                assert_eq!(forward.painter_order().len(), count);
                forward.take_frame_changes();
                forward.advance_to(time).unwrap();
                assert!(forward.take_frame_changes().is_empty());
            }
        }
    }

    #[test]
    fn morph_reveal_fixture_publishes_independent_drivers_and_completes_the_target() {
        use crate::{LiveProgramStatus, RustHostCallbackTable};
        let mut program = morph_reveal().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        program.take_renderer_publication();
        program.drive_to(&mut callbacks, 1.5).unwrap();
        assert_eq!(program.session().frame().objects.len(), 1);
        assert!((program.session().frame().morph(0) - 0.5).abs() < 1.0e-6);
        assert!((program.session().frame().reveal(0) - 0.5).abs() < 1.0e-6);
        program.take_renderer_publication();

        let LiveProgramStatus::PublicationPending(expected) =
            program.drive_to(&mut callbacks, 3.0).unwrap()
        else {
            panic!("morph/reveal endpoint must publish completion");
        };
        let context = program.take_renderer_publication().context();
        assert_eq!(context, expected);
        program.admit_publication(context).unwrap();
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        let Some(crate::GeometryRef::VectorPath(path)) =
            program.session().frame().render_geometry(0)
        else {
            panic!("completed morph must retain its target path");
        };
        assert!(
            path.morph_target().is_none(),
            "completion must retire interpolation geometry"
        );
        assert_eq!(
            path.commands().last(),
            Some(&crate::PathCommand::LineTo {
                to: Vec2::new(2.0, -1.4)
            })
        );
    }
}
