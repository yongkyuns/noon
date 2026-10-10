#![cfg(not(target_arch = "wasm32"))]
use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
use noon_export::output::{export_file, OutputOptions};
use noon_export::{capture_frames, CaptureOptions};
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};

fn options(p: u32, q: u32) -> ExportFrameOptions {
    ExportFrameOptions {
        frame_rate: FrameRate::new(p, q).unwrap(),
        start_frame: 0,
        stop: ExportStop::SourceEnd,
        max_frames: 1_000,
        max_transitions_per_sample: 64,
        final_hold_seconds: 0.0,
    }
}

fn capture_options() -> CaptureOptions {
    let mut options = CaptureOptions::new(320, 180);
    options.backends = noon_export::Backends::VULKAN;
    options.force_fallback_adapter = true;
    options
}

#[test]
#[ignore = "requires software Vulkan and FFmpeg; native output gate verifies decoded results"]
fn export_camera_to_mp4_and_png_at_integer_and_fractional_rates() {
    let root = std::env::var_os("NOON_OUTPUT_PROOF_DIR")
        .map(std::path::PathBuf::from)
        .expect("set NOON_OUTPUT_PROOF_DIR for retained output evidence");
    fs::create_dir_all(&root).unwrap();
    let mut cases = BufWriter::new(File::create(root.join("cases.tsv")).unwrap());
    writeln!(cases, "name\tp\tq\twidth\theight\tframes\tpng").unwrap();
    for (name, p, q, frames, png) in [
        ("camera-30", 30, 1, 90, true),
        ("camera-2997", 30_000, 1_001, 90, false),
        ("camera-5994", 60_000, 1_001, 180, false),
    ] {
        let (mut program, mut callbacks) =
            noon::example_scenes::following_graph_camera::program().unwrap();
        let result = export_file(
            &mut program,
            &mut callbacks,
            options(p, q),
            capture_options(),
            OutputOptions::mp4(root.join(format!("{name}.mp4"))),
        )
        .unwrap();
        assert_eq!(result.capture.sampling.frames, frames);
        assert_eq!(result.capture.sampling.source_end, Some(3.0));
        eprintln!("{name}: {} frames on {:?}", frames, result.capture.adapter);
        let (mut reference, mut callbacks) =
            noon::example_scenes::following_graph_camera::program().unwrap();
        let mut raw = BufWriter::new(File::create(root.join(format!("{name}.rgba"))).unwrap());
        let summary = capture_frames(
            &mut reference,
            &mut callbacks,
            options(p, q),
            capture_options(),
            |frame| raw.write_all(frame.rgba),
        )
        .unwrap();
        raw.flush().unwrap();
        assert_eq!(summary.sampling.frames, frames);
        if png {
            let (mut source, mut callbacks) =
                noon::example_scenes::following_graph_camera::program().unwrap();
            let images = export_file(
                &mut source,
                &mut callbacks,
                options(p, q),
                capture_options(),
                OutputOptions::png_sequence(root.join(format!("{name}-png"))),
            )
            .unwrap();
            assert_eq!(images.capture.sampling.frames, frames);
        }
        writeln!(cases, "{name}\t{p}\t{q}\t320\t180\t{frames}\t{png}").unwrap();
    }
    cases.flush().unwrap();
}

#[test]
#[ignore = "requires software Vulkan and FFmpeg; native output gate verifies decoded results"]
fn cropped_export_mp4_uses_source_end_relative_to_crop_start() {
    let root = std::env::var_os("NOON_OUTPUT_PROOF_DIR")
        .map(std::path::PathBuf::from)
        .expect("set NOON_OUTPUT_PROOF_DIR for cropped output evidence");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("cropped-camera-2997.mp4");
    let mut frame_options = options(30_000, 1_001);
    frame_options.start_frame = 20;
    let (mut program, mut callbacks) =
        noon::example_scenes::following_graph_camera::program().unwrap();
    let result = export_file(
        &mut program,
        &mut callbacks,
        frame_options,
        capture_options(),
        OutputOptions::mp4(&path),
    )
    .unwrap();
    assert_eq!(result.capture.sampling.frames, 70);
    assert_eq!(result.capture.sampling.source_end, Some(3.0));
    let expected_start = 20.0 * 1_001.0 / 30_000.0;
    assert!((result.capture.sampling.start_time - expected_start).abs() < 1e-12);

    let output = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_packets",
            "-show_entries",
            "stream=duration_ts,nb_frames:packet=pts,duration",
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success());
    let probe = String::from_utf8(output.stdout).unwrap();
    assert!(probe.contains("duration_ts=69980"), "{probe}");
    assert!(probe.contains("nb_frames=70"), "{probe}");
    assert!(probe.contains("pts=69069"), "{probe}");
    assert!(probe.contains("duration=911"), "{probe}");

    let (mut raw_program, mut raw_callbacks) =
        noon::example_scenes::following_graph_camera::program().unwrap();
    let raw = capture_frames(
        &mut raw_program,
        &mut raw_callbacks,
        frame_options,
        capture_options(),
        |frame| {
            assert_eq!(frame.frame.source_sample.index(), frame.frame.pts + 20);
            let clip_time = frame.frame.source_sample.authored_time() - expected_start;
            let expected_pts = frame.frame.pts as f64 * 1_001.0 / 30_000.0;
            assert!((clip_time - expected_pts).abs() < 1e-12);
            Ok::<_, io::Error>(())
        },
    )
    .unwrap();
    assert_eq!(raw.sampling.frames, 70);
}

#[test]
#[ignore = "requires software Vulkan and FFmpeg"]
fn source_failure_never_replaces_an_existing_video() {
    struct Fails(bool);
    impl noon::LiveContinuation for Fails {
        type Error = io::Error;
        fn resume(
            &mut self,
            live: &mut noon::LiveSession<'_>,
        ) -> Result<noon::ContinuationStep, io::Error> {
            if self.0 {
                return Err(io::Error::other("intentional source failure"));
            }
            self.0 = true;
            live.wait_segment(0.1)
                .map(noon::ContinuationStep::Await)
                .map_err(io::Error::other)
        }
    }
    let root =
        std::env::temp_dir().join(format!("noon-output-source-failure-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let path = root.join("old.mp4");
    fs::write(&path, b"existing user video").unwrap();
    let mut scene = noon::Scene::new();
    let object = scene.square(1.0).unwrap();
    scene.add(&object).unwrap();
    let mut program = scene.into_live_program(Fails(false)).unwrap();
    let mut callbacks = noon::RustHostCallbackTable::new();
    let mut output = OutputOptions::mp4(&path);
    output.overwrite = true;
    let error = export_file(
        &mut program,
        &mut callbacks,
        options(30, 1),
        capture_options(),
        output,
    )
    .unwrap_err();
    assert!(error.to_string().contains("intentional source failure"));
    assert_eq!(fs::read(&path).unwrap(), b"existing user video");
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    fs::remove_dir_all(root).unwrap();
}
