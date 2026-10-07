//! Native counterpart of the MCP agent_temporal_translation.py fixture.
//! Exercise typed authoring, shared execution, and production retained rendering;
//! this is not a native clone of the browser preview service.

use noon::{AnimationOptions, Color, ExecutionSession, RateFunction, Scene};

mod raster_support;
use raster_support::{Raster, SIZE, WORLD_SIZE};

fn sample_translation(y: f64, mut observe: impl FnMut(&mut ExecutionSession, f64, f64)) {
    let mut scene = Scene::new();
    let mut marker = scene.square(1.0).unwrap();
    let blue = Color::BLUE;
    marker
        .set_fill(blue.red.into(), blue.green.into(), blue.blue.into(), 1.0)
        .unwrap();
    marker.set_stroke_width(0.0).unwrap();
    marker.set_translation(-3.0, y).unwrap();
    scene.add(&marker).unwrap();
    let mut target = marker.target_editor().unwrap();
    target.set_translation(3.0, y).unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_transform_to(
            &marker,
            &target,
            AnimationOptions::new()
                .run_time(3.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();

    for (time, expected_x) in [(0.0, -3.0), (1.0, -1.0), (2.0, 1.0), (3.0, 3.0)] {
        session.advance_segment_to(segment, time).unwrap();
        assert_eq!(session.frame().time, time, "engine-published timestamp");
        assert_eq!(session.frame().objects.len(), 1);
        let position = session.frame().objects[0].transform.translation;
        assert!((f64::from(position.x) - expected_x).abs() < 1.0e-6);
        assert_eq!(position.y, y as f32);
        observe(&mut session, time, expected_x);
    }
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(marker.center().unwrap(), (3.0, y));
    assert_eq!(session.frame().time, 3.0);
}

#[test]
fn temporal_translation_has_expected_published_states() {
    sample_translation(0.0, |_, _, _| {});
}

fn blue_centroid(pixels: &[u8], width: u32, height: u32) -> (f64, f64) {
    assert_eq!(pixels.len(), (width * height * 4) as usize);
    let mut total_weight = 0.0;
    let mut weighted_x = 0.0;
    let mut weighted_y = 0.0;
    for (index, pixel) in pixels.as_chunks::<4>().0.iter().enumerate() {
        let blue_excess = pixel[2].saturating_sub(pixel[0].max(pixel[1]));
        let weight = f64::from(blue_excess) * f64::from(pixel[3]) / 255.0;
        total_weight += weight;
        weighted_x += (index % width as usize) as f64 * weight;
        weighted_y += (index / width as usize) as f64 * weight;
    }
    assert!(
        total_weight > 0.0,
        "rendered frame must contain the blue marker"
    );
    (weighted_x / total_weight, weighted_y / total_weight)
}

#[test]
#[ignore = "requires software Vulkan; executed by Native Host Smoke"]
fn native_temporal_translation_matches_published_states() {
    let capture_run = |width, height, y| {
        let mut raster = if (width, height) == (SIZE, SIZE) {
            pollster::block_on(Raster::new())
        } else {
            pollster::block_on(Raster::with_dimensions(width, height))
        };
        let mut frames = Vec::new();
        sample_translation(y, |session, time, expected_x| {
            let context = session.publication_context();
            let pixels = raster.capture(&session.take_renderer_publication());
            let (centroid_x, centroid_y) = blue_centroid(&pixels, width, height);
            // Pixel indices locate centers half a pixel below viewport coordinates.
            // The original 256-square oracle still expects x=31.5,95.5,159.5,223.5.
            // Odd, non-square targets additionally expose row padding and inversion.
            let density = f64::from(height) / f64::from(WORLD_SIZE);
            let expected_pixel_x = expected_x * density + f64::from(width) / 2.0 - 0.5;
            let expected_pixel_y = -y * density + f64::from(height) / 2.0 - 0.5;
            assert!(
                (centroid_x - expected_pixel_x).abs() <= 1.0,
                "{width}x{height}, time {time}: x {centroid_x}, expected {expected_pixel_x}"
            );
            assert!(
                (centroid_y - expected_pixel_y).abs() <= 1.0,
                "{width}x{height}, time {time}: y {centroid_y}, expected {expected_pixel_y}"
            );
            let repeated = raster.capture(&session.take_renderer_publication());
            assert!(pixels == repeated, "repeat capture changed pixels at {time}");
            assert_eq!(session.publication_context(), context);
            assert_eq!(
                session.frame().time,
                time,
                "capture must not advance execution"
            );
            eprintln!(
                "native temporal sample: {width}x{height}, time={time}, centroid=({centroid_x},{centroid_y})"
            );
            frames.push(pixels);
        });
        frames
    };
    for (width, height, y) in [(SIZE, SIZE, 0.0), (257, 129, 2.0), (65, 33, -2.0)] {
        let first = capture_run(width, height, y);
        let second = capture_run(width, height, y);
        assert_eq!(first.len(), 4);
        assert_eq!(second.len(), 4);
        for (index, (left, right)) in first.iter().zip(&second).enumerate() {
            assert!(
                left == right,
                "{width}x{height}: fresh-session pixels differ at sample {index}"
            );
        }
    }
}
