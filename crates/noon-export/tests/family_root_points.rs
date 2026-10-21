//! Native production capture of visible root geometry across live edits.
#![cfg(not(target_arch = "wasm32"))]

use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
use noon::RustHostCallbackTable;
use noon_export::{capture_frames, CaptureOptions};

#[test]
#[ignore = "requires native GPU; selected by native output gate"]
fn native_family_root_points_preserve_drawing_children_and_restored_pixels() {
    let mut program = noon::example_scenes::family_root_points::program().unwrap();
    let evidence =
        std::env::var_os("NOON_FAMILY_ROOT_NATIVE_ARTIFACTS").map(std::path::PathBuf::from);
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
    assert_eq!(frames.len(), 6);
    for (index, frame) in frames.iter().enumerate() {
        let mut blue = [0; 2];
        let mut yellow = [0; 2];
        let mut white = 0;
        for (offset, pixel) in frame.as_chunks::<4>().0.iter().enumerate() {
            let half = usize::from(offset % 640 >= 320);
            blue[half] += usize::from(pixel[0] < 30 && pixel[1] < 30 && pixel[2] > 200);
            yellow[half] += usize::from(pixel[0] > 200 && pixel[1] > 200 && pixel[2] < 30);
            white += usize::from(pixel[0] > 200 && pixel[1] > 200 && pixel[2] > 200);
        }
        assert_eq!(
            blue[0], 0,
            "ordinary Group root must not draw, frame {index}"
        );
        assert_eq!(
            white, 0,
            "ordinary Group root outline must not draw, frame {index}"
        );
        assert!(
            yellow[0] > 10 && yellow[1] > 10,
            "children must paint over roots, frame {index}"
        );
        if (2..4).contains(&index) {
            assert_eq!(blue[1], 0, "cleared root must retire visible geometry");
        } else {
            assert!(
                blue[1] > 100,
                "pointful vector root must draw, frame {index}"
            );
        }
    }
    assert_eq!(frames[0], frames[1]);
    assert_eq!(frames[2], frames[3]);
    assert_eq!(frames[4], frames[5]);
    assert_eq!(
        frames[0], frames[4],
        "restoring owned points restores exact pixels"
    );
}
