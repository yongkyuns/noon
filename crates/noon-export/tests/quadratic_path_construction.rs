//! The existing paired Rust path builder reaches native pixels after degree elevation.
#![cfg(not(target_arch = "wasm32"))]
use noon_export::{CaptureOptions, SessionCapture};

#[test]
#[ignore = "requires native GPU; selected by native output gate"]
fn native_quadratic_path_construction_renders_through_shared_session_capture() {
    let mut session = noon::example_scenes::path_construction::session().unwrap();
    let mut capture =
        SessionCapture::new(session.session(), CaptureOptions::new(640, 360)).unwrap();
    let mut first: Option<Vec<u8>> = None;
    for time in [0.0, 0.1, 0.2] {
        session.seek(time).unwrap();
        assert_eq!(session.session().frame().objects.len(), 2);
        let frame = capture.capture(session.session_mut()).unwrap();
        assert_eq!(frame.receipt.published_time, time);
        // The quadratic from (-2, 0) through (-1, 2) to (0, 0) reaches
        // (-1, 1). The default camera maps that apex to pixel (275, 135).
        let blue_apex = (131..140).any(|row| {
            (271..280).any(|column| {
                let pixel = &frame.rgba[4 * (row * 640 + column)..][..4];
                u16::from(pixel[2]) > u16::from(pixel[0]) + 64
                    && u16::from(pixel[1]) > u16::from(pixel[0]) + 64
            })
        });
        assert!(blue_apex, "quadratic apex absent at {time}");
        if let Some(expected) = &first {
            assert_eq!(
                frame.rgba,
                expected.as_slice(),
                "static path changed at {time}"
            );
        } else {
            if let Some(path) = std::env::var_os("NOON_QUADRATIC_NATIVE_ARTIFACTS") {
                let path = std::path::PathBuf::from(path);
                std::fs::create_dir_all(&path).unwrap();
                std::fs::write(path.join("path-construction.rgba"), frame.rgba).unwrap();
            }
            first = Some(frame.rgba.to_vec());
        }
    }
}
