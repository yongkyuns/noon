//! Query results drive normal native geometry through three live publications.
#![cfg(not(target_arch = "wasm32"))]

use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
use noon::RustHostCallbackTable;
use noon_export::{capture_frames, CaptureOptions};

#[test]
#[ignore = "requires native GPU; selected by native output gate"]
fn native_member_queries_render_captured_current_and_own_root_selections() {
    let mut program = noon::example_scenes::family_member_queries::program().unwrap();
    let evidence =
        std::env::var_os("NOON_FAMILY_QUERY_NATIVE_ARTIFACTS").map(std::path::PathBuf::from);
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
    assert_eq!(summary.sampling.frames, 6);
    for (index, frame) in frames.iter().enumerate() {
        let mut colors = [0; 4];
        let mut white = 0;
        for pixel in frame.as_chunks::<4>().0 {
            colors[0] += usize::from(pixel[0] > 200 && pixel[1] < 30 && pixel[2] < 30);
            colors[1] += usize::from(pixel[0] < 30 && pixel[1] < 30 && pixel[2] > 200);
            colors[2] += usize::from(pixel[0] > 200 && pixel[1] > 200 && pixel[2] < 30);
            colors[3] += usize::from(pixel[0] < 30 && pixel[1] > 200 && pixel[2] < 30);
            white += usize::from(pixel[0] > 200 && pixel[1] > 200 && pixel[2] > 200);
        }
        let expected = match index / 2 {
            0 => [true, false, true, false],
            1 => [false, true, true, false],
            2 => [false, true, true, true],
            _ => panic!("three held selections"),
        };
        for (color, present) in colors.into_iter().zip(expected) {
            if present {
                assert!(color > 100, "selected color is missing in frame {index}");
            } else {
                assert_eq!(color, 0, "stale/unselected color in frame {index}");
            }
        }
        assert_eq!(white, 0, "ordinary Group own query points must not draw");
    }
    assert_eq!(frames[0], frames[1]);
    assert_eq!(frames[2], frames[3]);
    assert_eq!(frames[4], frames[5]);
}
