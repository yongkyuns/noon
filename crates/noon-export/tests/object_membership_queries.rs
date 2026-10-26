//! Both live membership phases reach native pixels through the shared program.
#![cfg(not(target_arch = "wasm32"))]
use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
use noon::RustHostCallbackTable;
use noon_export::{capture_frames, CaptureOptions};

fn visible_patch(frame: &[u8], x: f64, y: f64) -> bool {
    // The default 640x360 camera spans 128/9 by 8 world units.
    let cx = (320.0 + x * 45.0).round() as usize;
    let cy = (180.0 - y * 45.0).round() as usize;
    (cy - 20..cy + 21).any(|row| {
        (cx - 20..cx + 21).any(|column| {
            let pixel = &frame[4 * (row * 640 + column)..][..4];
            pixel[..3].iter().any(|&value| value > 32)
        })
    })
}

#[test]
#[ignore = "requires native GPU; selected by native output gate"]
fn native_object_membership_before_and_after_copy_render_through_shared_capture() {
    let mut program = noon::example_scenes::object_membership_queries::program().unwrap();
    let evidence =
        std::env::var_os("NOON_OBJECT_MEMBERSHIP_NATIVE_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(path) = &evidence {
        std::fs::create_dir_all(path).unwrap();
    }
    let mut frames = Vec::new();
    let summary = capture_frames(
        &mut program,
        &mut RustHostCallbackTable::new(),
        ExportFrameOptions {
            frame_rate: FrameRate::new(8, 1).unwrap(),
            start_frame: 0,
            stop: ExportStop::SourceEnd,
            max_frames: 8,
            max_transitions_per_sample: 32,
            final_hold_seconds: 0.0,
        },
        CaptureOptions::new(640, 360),
        |frame| {
            if let Some(path) = &evidence {
                std::fs::write(
                    path.join(format!("frame-{}.rgba", frames.len())),
                    frame.rgba,
                )?;
            }
            frames.push(frame.rgba.to_vec());
            Ok::<_, std::io::Error>(())
        },
    )
    .unwrap();
    assert_eq!(summary.sampling.frames, 4);
    for (index, frame) in frames.iter().enumerate() {
        let copied = index >= 2;
        for (x, y, expected) in [
            (0.0, 0.0, true),
            (1.0, 0.0, true),
            (0.0, 1.0, !copied),
            (-1.0, 0.0, copied),
            (0.0, -1.0, copied),
            (-1.0, -1.0, copied),
            (1.0, -1.0, copied),
        ] {
            assert_eq!(
                visible_patch(frame, x, y),
                expected,
                "frame {index}, location ({x}, {y})"
            );
        }
    }
    assert_eq!(frames[0], frames[1]);
    assert_eq!(frames[2], frames[3]);
    assert_ne!(frames[0], frames[2]);
}
