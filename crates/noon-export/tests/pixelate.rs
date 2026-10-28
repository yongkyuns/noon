//! Ordinary high-level Rust programme and independent literal rectangle oracle.
#![cfg(not(target_arch = "wasm32"))]
use noon_export::{capture_frames, Backends, CaptureOptions};
fn verify(image: &[u8], time: f64) -> (usize, u8) {
    let size = if time < 0.5 {
        12.0
    } else if time < 1.5 {
        12.0 - 11.0 * (time - 0.5)
    } else if time < 2.0 || time >= 3.0 {
        1.0
    } else {
        8.0
    };
    let mut count = 0;
    let mut maximum = 0;
    for y in 148..212 {
        for x in 350..450 {
            let point = [x as f64 + 0.5, y as f64 + 0.5];
            let sample: [f64; 2] = if size <= 1.0 {
                point
            } else {
                std::array::from_fn(|a| {
                    let origin = [368.0, 164.0][a];
                    (origin + (((point[a] - origin) / size).floor() + 0.5) * size).floor() + 0.5
                })
            };
            // Operator sampling is separately qualified at every texel. This literal
            // source oracle excludes only the source renderer's AA boundary band.
            if [368.0, 384.0, 416.0, 432.0]
                .into_iter()
                .any(|edge| (sample[0] - edge).abs() < 1.0)
                || [164.0, 196.0]
                    .into_iter()
                    .any(|edge| (sample[1] - edge).abs() < 1.0)
            {
                continue;
            }
            let red = (368.0..416.0).contains(&sample[0]) && (164.0..196.0).contains(&sample[1]);
            let blue = (384.0..432.0).contains(&sample[0]) && (164.0..196.0).contains(&sample[1]);
            let mut premul: [f64; 4] = [0.0; 4];
            if red {
                premul = [0.6, 0.0, 0.0, 0.6];
            }
            if blue {
                premul = std::array::from_fn(|c| {
                    premul[c] * 0.5 + if c == 2 || c == 3 { 0.5 } else { 0.0 }
                });
            }
            let source = premul.map(|v| (v * 255.0).round() / 255.0 * 0.75);
            for c in 0..4 {
                let expected = (255.0 * source[c]
                    + [24.0, 48.0, 64.0, 255.0][c] * (1.0 - source[3]))
                    .round() as u8;
                let actual = image[(y as usize * 640 + x as usize) * 4 + c];
                let error = actual.abs_diff(expected);
                maximum = maximum.max(error);
                assert!(error<=2,"literal Pixelate t={time} size={size} at={x},{y} c={c} sample={sample:?} got={actual} expected={expected}");
                count += 1;
            }
        }
    }
    if size <= 1.0 {
        for y in 148..212 {
            for x in 350..450 {
                let a = (y * 640 + x) * 4;
                let b = (y * 640 + x - 160) * 4;
                assert_eq!(
                    &image[a..a + 4],
                    &image[b..b + 4],
                    "neutral/removed composition is exact"
                );
            }
        }
    }
    (count, maximum)
}
#[test]
#[ignore = "requires actual native GPU; paired high-level Pixelate programme and independent source/composition oracle"]
fn authored_pixelate_animation_identity_units_removal_and_export_match_literal_reference() {
    use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
    let mut program = noon::example_scenes::pixelate::program().unwrap();
    let mut options = CaptureOptions::new(640, 360);
    options.backends = if cfg!(target_os = "macos") {
        Backends::METAL
    } else {
        Backends::VULKAN
    };
    let mut count = 0;
    let mut maximum = 0;
    let mut retained = std::collections::BTreeMap::new();
    let summary = capture_frames(
        &mut program,
        &mut noon::RustHostCallbackTable::new(),
        ExportFrameOptions {
            frame_rate: FrameRate::new(8, 1).unwrap(),
            start_frame: 0,
            stop: ExportStop::SourceEnd,
            max_frames: 29,
            max_transitions_per_sample: 64,
            final_hold_seconds: 0.0,
        },
        options,
        |frame| {
            let time = frame.observation.requested_time;
            let (n, e) = verify(frame.rgba, time);
            count += n;
            maximum = maximum.max(e);
            if [0.25, 0.75, 1.25, 1.75, 2.25, 2.75, 3.25].contains(&time) {
                retained.insert((time * 8.0) as u32, frame.rgba.to_vec());
            }
            Ok::<_, std::convert::Infallible>(())
        },
    )
    .unwrap();
    assert_eq!(summary.sampling.frames, 28);
    assert_eq!(retained.len(), 7);
    assert_eq!(
        retained[&18], retained[&22],
        "scene .2 and output8 pixels agree at this view"
    );
    assert_eq!(
        retained[&14], retained[&26],
        "one-pixel identity and removal agree"
    );
    if let Some(path) = std::env::var_os("NOON_PIXELATE_ARTIFACTS") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        for (at, rgba) in retained {
            std::fs::write(path.join(format!("frame-{at}.rgba")), rgba).unwrap();
        }
    }
    eprintln!("PIXELATE_PROGRAMME_PASS frames=28 independent_channels={count} maximum_byte_error={maximum} identity_exact=true scene_pixel_units_exact=true");
}
