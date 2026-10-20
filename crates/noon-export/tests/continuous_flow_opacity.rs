#![cfg(not(target_arch = "wasm32"))]

use noon::{AnimationOptions, ExecutionSession, Mobject, Scene, StrokePattern};
use noon_core::{SemanticAnimationCompositionKind, SemanticAnimationIntent};
use noon_export::{Backends, CaptureOptions, SessionCapture};

const WIDTH: usize = 640;
const HEIGHT: usize = 360;
const TIMES: [f64; 8] = [0.0, 0.5, 0.75, 1.0, 1.5, 2.0, 2.5, 3.0];

fn pattern(dotted: bool, phase: f32, opacity: f32) -> StrokePattern {
    let shape = if dotted {
        StrokePattern::dotted(0.5)
    } else {
        StrokePattern::dashed(0.25, 0.25)
    };
    shape
        .unwrap()
        .with_phase(phase)
        .unwrap()
        .with_gap_opacity(opacity)
        .unwrap()
}

fn line(scene: &mut Scene, pattern: StrokePattern, progress: f32) -> Mobject {
    let progress = f64::from(progress);
    let mut object = scene
        .line((-3.0 + progress, progress), (3.0 - progress, progress))
        .unwrap();
    object.set_stroke_color(1.0, 1.0, 1.0, 1.0).unwrap();
    object.set_stroke_width(0.2).unwrap();
    object.set_stroke_pattern(Some(pattern)).unwrap();
    object
}

fn options() -> CaptureOptions {
    let mut options = CaptureOptions::new(WIDTH as u32, HEIGHT as u32);
    options.backends = Backends::VULKAN;
    options.force_fallback_adapter = true;
    options
}

fn animated(dotted: bool, speed: f32) -> ExecutionSession {
    let mut scene = Scene::new();
    let source = line(&mut scene, pattern(dotted, 0.125, 0.125), 0.0);
    scene.add(&source).unwrap();
    // An unrelated target phase must not override signed speed. Both endpoints
    // move, the connection shortens, and its dim rail becomes more opaque.
    let target = line(&mut scene, pattern(dotted, 100.0, 0.875), 1.0);
    scene.wait(0.5).unwrap();
    let flow = scene.flow_stroke_to(&source, &target, speed, 2.0).unwrap();
    scene.wait(2.5).unwrap();
    let root = scene
        .declare_animation(
            SemanticAnimationIntent::Composition {
                kind: SemanticAnimationCompositionKind::Parallel,
                children: vec![flow.node_id()],
            },
            AnimationOptions::new(),
        )
        .unwrap();
    scene.execution_session_with_animation_root(&root).unwrap()
}

fn independent_pixels(dotted: bool, speed: f32, time: f64) -> Vec<u8> {
    // This oracle authors a static scene from independent expected values. It
    // never calls flow_stroke_to, StrokePattern::interpolate or a runtime track.
    let elapsed = (time - 0.5).clamp(0.0, 2.0) as f32;
    let progress = elapsed * 0.5;
    let phase = 0.125 + speed * elapsed;
    let opacity = 0.125 + 0.75 * progress;
    let mut scene = Scene::new();
    let object = line(&mut scene, pattern(dotted, phase, opacity), progress);
    scene.add(&object).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut capture = SessionCapture::new(&session, options()).unwrap();
    capture.capture(&mut session).unwrap().rgba.to_vec()
}

fn gap_brightness(rgba: &[u8]) -> u8 {
    assert_eq!(rgba.len(), WIDTH * HEIGHT * 4);
    let pixels = rgba.as_chunks::<4>().0;
    assert!(pixels.iter().all(|p| p[3] == 255));
    // Inspect the interior of the brightest scanline rather than assuming a
    // camera origin, color-space transfer function, or exact antialiasing edge.
    let row = pixels
        .as_chunks::<WIDTH>()
        .0
        .iter()
        .max_by_key(|row| row.iter().map(|p| u64::from(p[0])).sum::<u64>())
        .unwrap();
    let first = row.iter().position(|p| p[0] > 0).expect("visible line");
    let last = row.iter().rposition(|p| p[0] > 0).unwrap();
    assert!(last > first + 16, "connection must have a visible interior");
    let interior = &row[first + 8..last - 8];
    let dim = interior.iter().map(|p| p[0]).min().unwrap();
    let ink = interior.iter().map(|p| p[0]).max().unwrap();
    assert!(ink >= 240, "pattern ink must stay bright: {ink}");
    assert!(dim > 0 && dim < ink, "gap must remain visible and distinct");
    dim
}

fn proof(name: &str, rgba: &[u8]) {
    let Some(dir) = std::env::var_os("NOON_CAPTURE_PROOF_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut ppm = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
    for pixel in rgba.as_chunks::<4>().0 {
        ppm.extend_from_slice(&pixel[..3]);
    }
    std::fs::write(dir.join(format!("{name}.ppm")), ppm).unwrap();
}

#[test]
#[ignore = "requires software Vulkan; run by native export qualification"]
fn moving_flow_opacity_matches_static_oracle_and_retained_seek_pixels() {
    for dotted in [false, true] {
        for speed in [-0.5, 0.5] {
            let shape = if dotted { "dots" } else { "dashes" };
            let direction = if speed < 0.0 { "reverse" } else { "forward" };
            let mut session = animated(dotted, speed);
            let identity = session.frame().objects[0].id;
            let mut capture = SessionCapture::new(&session, options()).unwrap();
            let mut images = Vec::new();
            let mut gaps = Vec::new();
            for (index, time) in TIMES.into_iter().enumerate() {
                session.advance_to(time).unwrap();
                assert_eq!(session.frame().objects.len(), 1);
                assert_eq!(session.frame().objects[0].id, identity);
                let frame = capture.capture(&mut session).unwrap();
                assert_eq!(frame.receipt.published_time, time);
                let pixels = frame.rgba.to_vec();
                assert_eq!(
                    pixels,
                    independent_pixels(dotted, speed, time),
                    "independent {shape}/{direction} reference at t={time}"
                );
                gaps.push(gap_brightness(&pixels));
                let name = format!("flow-opacity-{shape}-{direction}-{index:02}");
                proof(&name, &pixels);
                images.push(pixels);
            }
            assert_eq!(images[0], images[1], "pre-start state must be held");
            assert_eq!(images[6], images[7], "completed state must freeze");
            assert_ne!(images[1], images[2], "active flow must visibly change");
            assert!(
                u16::from(gaps[6]) > u16::from(gaps[1]) + 32,
                "the rail must actually brighten, not just change geometry"
            );
            for index in [7, 2, 0, 5, 3, 6, 1, 4, 2, 0] {
                session.seek(TIMES[index]).unwrap();
                let frame = capture.capture(&mut session).unwrap();
                assert_eq!(frame.receipt.published_time, TIMES[index]);
                assert_eq!(
                    frame.rgba, images[index],
                    "retained {shape}/{direction} pixels at t={}",
                    TIMES[index]
                );
            }
        }
    }
}
