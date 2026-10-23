//! Actual native pixels protect painter ordering across both promotions.
#![cfg(not(target_arch = "wasm32"))]
use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
use noon::RustHostCallbackTable;
use noon_export::{capture_frames, CaptureOptions};

#[test]
#[ignore = "requires native GPU; selected by native output gate"]
fn native_point_promotion_moves_own_paint_after_existing_children() {
    let mut program = noon::example_scenes::point_promotion::program().unwrap();
    let artifacts =
        std::env::var_os("NOON_POINT_PROMOTION_NATIVE_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(path) = &artifacts {
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
            max_frames: 12,
            max_transitions_per_sample: 32,
            final_hold_seconds: 0.0,
        },
        CaptureOptions::new(640, 360),
        |frame| {
            if let Some(path) = &artifacts {
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
    assert_eq!(summary.sampling.frames, 6);
    for (index, frame) in frames.iter().enumerate() {
        for (column, expected) in [
            (266, if index < 2 { 2 } else { 0 }),
            (374, if index < 4 { 1 } else { 0 }),
        ] {
            let pixel = &frame[4 * (180 * 640 + column)..][..4];
            assert!(
                pixel[expected] > 200,
                "frame {index}, column {column}: {pixel:?}"
            );
            assert!(
                pixel[..3]
                    .iter()
                    .enumerate()
                    .all(|(channel, &value)| channel == expected || value < 32),
                "frame {index}, column {column}: {pixel:?}"
            );
        }
    }
    for first in [0, 2, 4] {
        assert_eq!(frames[first], frames[first + 1]);
    }
    assert_ne!(frames[0], frames[2]);
    assert_ne!(frames[2], frames[4]);
}
