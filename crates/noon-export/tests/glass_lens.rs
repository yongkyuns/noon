//! Normal Rust authoring and native export, with literal prefix/lifecycle witnesses.
#![cfg(all(
    not(target_arch = "wasm32"),
    feature = "native-text",
    feature = "bundled-fonts"
))]
use noon_export::{capture_frames, Backends, CaptureOptions};

#[test]
#[ignore = "requires actual native GPU; normal glass-card live programme and export"]
fn glass_lens_motion_neutral_removal_and_visibility_export() {
    use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
    let mut program = noon::example_scenes::glass_lens::program().unwrap();
    let mut options = CaptureOptions::new(320, 180);
    options.backends = if cfg!(target_os = "macos") {
        Backends::METAL
    } else {
        Backends::VULKAN
    };
    let mut retained = std::collections::BTreeMap::new();
    let summary = capture_frames(
        &mut program,
        &mut noon::RustHostCallbackTable::new(),
        ExportFrameOptions {
            frame_rate: FrameRate::new(4, 1).unwrap(),
            start_frame: 0,
            stop: ExportStop::SourceEnd,
            max_frames: 35,
            max_transitions_per_sample: 64,
            final_hold_seconds: 0.0,
        },
        options,
        |frame| {
            retained.insert(
                (frame.observation.requested_time * 4.0).round() as u32,
                frame.rgba.to_vec(),
            );
            Ok::<_, std::convert::Infallible>(())
        },
    )
    .unwrap();
    assert_eq!(summary.sampling.frames, 34);
    assert_eq!(
        retained[&17], retained[&19],
        "zero radius and removed attachment are exact"
    );
    assert_eq!(
        retained[&21], retained[&33],
        "dissolve-in restores the same glass card exactly"
    );
    let changed = retained[&1]
        .iter()
        .zip(&retained[&17])
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        changed > 1000,
        "nonzero backdrop must change the checker: {changed}"
    );
    // At full dissolve-out, only the literal opaque prefix remains. This is an
    // independent source oracle, not a comparison against another effect render.
    let mut maximum = 0;
    let mut channels = 0;
    for y in 0..180 {
        for x in 0..320 {
            // The normal image/path rasterizer's exact silhouette edge is qualified
            // elsewhere; leave its one-pixel AA band out of this literal witness.
            if [39, 40, 279, 280].contains(&x) || [29, 30, 149, 150].contains(&y) {
                continue;
            }
            let expected = if (40..280).contains(&x) && (30..150).contains(&y) {
                if ((x - 40) / 15 + (y - 30) / 15) % 2 == 0 {
                    [28, 82, 148, 255]
                } else {
                    [240, 166, 70, 255]
                }
            } else {
                [8, 16, 31, 255]
            };
            for (c, value) in expected.into_iter().enumerate() {
                let error = retained[&27][(y * 320 + x) * 4 + c].abs_diff(value);
                maximum = maximum.max(error);
                assert!(error <= 2, "full-out prefix at {x},{y}/{c}: error {error}");
                channels += 1;
            }
        }
    }
    if let Some(path) = std::env::var_os("NOON_GLASS_ARTIFACTS") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        for (at, rgba) in retained {
            std::fs::write(path.join(format!("frame-{at}.rgba")), rgba).unwrap();
        }
    }
    eprintln!("GLASS_PROGRAMME_PASS frames=34 independent_channels={channels} maximum_byte_error={maximum} changed_channels={changed} neutral_removal_exact=true visibility_restore_exact=true");
}
