//! Filled morph and reveal-continuity examples shared by native and direct WASM.
use crate::{
    AnimationCompositionRequest, AnimationOptions, Color, ExecutionSession, Mobject, RateFunction,
    Scene, SemanticPaint, SemanticStyle, TransformToRequest, Vec2, VectorPath, BLUE, PINK, PURPLE,
    WHITE,
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
    let mut state = object.state()?;
    state.style = style(fill, stroke, width);
    object.commit_state(state)
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
    let shape = scene.path(rounded_loop(1.35), style(Some(BLUE), WHITE, 0.08))?;
    let target = scene.path(star(1.7, 0.7, 0.0), style(Some(PURPLE), WHITE, 0.08))?;
    scene.add(&shape)?;
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

/// Pair: `web/python/examples/ordinary_create_shapes.py`.
pub fn create_shapes() -> Result<ExecutionSession, String> {
    let scene = Scene::new();
    let mut circle = scene.circle(0.9)?;
    circle.set_translation(-3.0, 1.0)?;
    paint(&mut circle, Some(BLUE), WHITE, 0.055)?;
    let mut square = scene.square(1.7)?;
    square.set_translation(0.0, 1.0)?;
    paint(&mut square, Some(PINK), WHITE, 0.055)?;
    let mut line = scene.line((1.75, 1.0), (4.25, 1.0))?;
    paint(&mut line, None, BLUE, 0.055)?;
    let wave = VectorPath::new().move_to(Vec2::new(-2.4, -1.6)).cubic_to(
        Vec2::new(-1.2, -2.6),
        Vec2::new(1.2, -0.6),
        Vec2::new(2.4, -1.6),
    );
    let wave = scene.path(wave, style(None, PINK, 0.05))?;
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
        let mut shape = scene.path(source.clone(), paint.clone())?;
        let mut target = scene.path(targets[index % targets.len()].clone(), paint)?;
        let x = -2.9 + (index % columns) as f64 * dx;
        let y = 1.9 - (index / columns) as f64 * dy;
        shape.set_translation(x, y)?;
        target.set_translation(x, y)?;
        scene.add(&shape)?;
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
}
