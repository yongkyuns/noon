#![cfg(feature = "png")]

use std::path::{Path, PathBuf};
use std::time::Duration;

use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
use noon_export::png::{
    export_png_sequence, PngError, PngExportError, PngSequenceOptions,
};
use noon_export::{Backends, CaptureOptions};

fn frames(rate: u32) -> ExportFrameOptions {
    ExportFrameOptions {
        frame_rate: FrameRate::new(rate, 1).unwrap(),
        start_frame: 0,
        stop: ExportStop::SourceEnd,
        max_frames: 1_000,
        max_transitions_per_sample: 64,
        final_hold_seconds: 0.0,
    }
}

fn software(width: u32, height: u32) -> CaptureOptions {
    let mut capture = CaptureOptions::new(width, height);
    capture.backends = Backends::VULKAN;
    capture.force_fallback_adapter = true;
    capture.gpu_wait_timeout = Duration::from_secs(30);
    capture
}

fn following() -> (
    noon::LiveProgram<noon::example_scenes::following_graph_camera::FollowingGraphCamera>,
    noon::RustHostCallbackTable,
) {
    noon::example_scenes::following_graph_camera::program().unwrap()
}

fn root() -> (PathBuf, bool) {
    if let Some(path) = std::env::var_os("NOON_PNG_PROOF_DIR") {
        let path = PathBuf::from(path);
        if path.exists() {
            std::fs::remove_dir_all(&path).unwrap();
        }
        std::fs::create_dir_all(&path).unwrap();
        return (path, false);
    }
    let path = std::env::temp_dir().join(format!("noon-png-proof-{}", std::process::id()));
    if path.exists() {
        std::fs::remove_dir_all(&path).unwrap();
    }
    std::fs::create_dir_all(&path).unwrap();
    (path, true)
}

fn png_files(directory: &Path) -> Vec<PathBuf> {
    let mut files = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "png"))
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn scratch_paths(root: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().contains(".noon-png-"))
        })
        .collect()
}

#[test]
#[ignore = "requires software Vulkan; run by Native Host Smoke without DISPLAY"]
fn native_png_sequence_is_complete_transactional_and_recoverable() {
    let (root, cleanup) = root();

    let output = root.join("sequence");
    let (mut program, mut callbacks) = following();
    let result = export_png_sequence(
        &mut program,
        &mut callbacks,
        frames(3),
        software(65, 33),
        PngSequenceOptions::new(&output),
    )
    .unwrap();
    assert_eq!(result.capture.sampling.frames, 9);
    assert_eq!(result.directory, output);

    let files = png_files(&result.directory);
    assert_eq!(files.len(), 9);
    assert_eq!(files[0].file_name().unwrap(), "frame_000000.png");
    assert_eq!(files[8].file_name().unwrap(), "frame_000008.png");
    for file in [&files[0], &files[8]] {
        let bytes = std::fs::read(file).unwrap();
        let image =
            image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).unwrap();
        assert_eq!((image.width(), image.height()), (65, 33));
    }

    let preserved = root.join("preserved");
    std::fs::create_dir(&preserved).unwrap();
    std::fs::write(preserved.join("old"), b"keep").unwrap();
    let (mut preserved_program, mut preserved_callbacks) = following();
    let error = export_png_sequence(
        &mut preserved_program,
        &mut preserved_callbacks,
        frames(1),
        software(32, 32),
        PngSequenceOptions::new(&preserved),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        PngExportError::Output(PngError::DestinationExists(_))
    ));
    assert_eq!(std::fs::read(preserved.join("old")).unwrap(), b"keep");
    assert_eq!(preserved_program.session().frame().time, 0.0);

    let replaced = root.join("replaced");
    std::fs::create_dir(&replaced).unwrap();
    std::fs::write(replaced.join("old"), b"old").unwrap();
    let (mut replaced_program, mut replaced_callbacks) = following();
    let mut replace_options = PngSequenceOptions::new(&replaced);
    replace_options.overwrite = true;
    let replaced_summary = export_png_sequence(
        &mut replaced_program,
        &mut replaced_callbacks,
        frames(1),
        software(32, 32),
        replace_options,
    )
    .unwrap();
    assert_eq!(replaced_summary.capture.sampling.frames, 3);
    assert!(!replaced.join("old").exists());
    assert_eq!(png_files(&replaced).len(), 3);

    let cancelled = root.join("cancelled");
    let (mut cancelled_program, mut cancelled_callbacks) = following();
    let cancelled_capture = software(32, 32);
    cancelled_capture.cancellation.cancel();
    let cancelled_error = export_png_sequence(
        &mut cancelled_program,
        &mut cancelled_callbacks,
        frames(1),
        cancelled_capture,
        PngSequenceOptions::new(&cancelled),
    )
    .unwrap_err();
    assert!(matches!(
        cancelled_error,
        PngExportError::Capture(noon_export::CaptureRunError::Cancelled)
    ));
    assert!(!cancelled.exists());
    assert_eq!(cancelled_program.session().frame().time, 0.0);

    let recovery = root.join("recovery");
    let (mut recovery_program, mut recovery_callbacks) = following();
    let recovery_summary = export_png_sequence(
        &mut recovery_program,
        &mut recovery_callbacks,
        frames(1),
        software(17, 9),
        PngSequenceOptions::new(&recovery),
    )
    .unwrap();
    assert_eq!(recovery_summary.capture.sampling.frames, 3);
    assert_eq!(png_files(&recovery).len(), 3);
    assert!(scratch_paths(&root).is_empty());

    if cleanup {
        std::fs::remove_dir_all(root).unwrap();
    }
}
