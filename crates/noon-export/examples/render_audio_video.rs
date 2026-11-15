//! Render the shared background-music scene, or the existing input/output lesson
//! with three prepared narration passages, directly to an MP4.
//!
//! The original score and animation use the same authored timeline as the
//! browser example. Playback buttons are ordinary scene objects and remain
//! visible in the movie; an encoded movie does not make those buttons interactive.

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
    use noon_export::output::{export_session_file, OutputOptions};
    use noon_export::CaptureOptions;

    let mut args = std::env::args_os().skip(1);
    const USAGE: &str =
        "usage: render_audio_video OUTPUT.mp4 [--narration POSITIVE.json NEGATIVE.json ZERO.json]";
    let path = args.next().ok_or(USAGE)?;
    let remaining: Vec<_> = args.collect();
    let narrated = !remaining.is_empty();
    let mut session = if narrated {
        if remaining.len() != 4 || remaining[0] != "--narration" {
            return Err(USAGE.into());
        }
        #[cfg(all(
            feature = "narration-import",
            feature = "native-text",
            feature = "bundled-fonts"
        ))]
        {
            let passages = [
                load_prepared(remaining[1].as_ref())?,
                load_prepared(remaining[2].as_ref())?,
                load_prepared(remaining[3].as_ref())?,
            ];
            noon::example_scenes::background_music_lesson::scene_with_narration(&passages)?
                .execution_session()
                .map_err(std::io::Error::other)?
        }
        #[cfg(not(all(
            feature = "narration-import",
            feature = "native-text",
            feature = "bundled-fonts"
        )))]
        return Err(
            "the narrated lesson requires features narration-import,native-text,bundled-fonts"
                .into(),
        );
    } else {
        let (scene, animation) =
            noon::example_scenes::background_audio::scene().map_err(std::io::Error::other)?;
        scene
            .execution_session_with_animation_root(&animation)
            .map_err(std::io::Error::other)?
    };
    let summary = export_session_file(
        &mut session,
        ExportFrameOptions {
            frame_rate: FrameRate::new(30, 1)?,
            start_frame: 0,
            stop: ExportStop::SourceEnd,
            max_frames: if narrated { 10_000 } else { 1_000 },
            max_transitions_per_sample: 64,
            final_hold_seconds: if narrated { 0.75 } else { 0.0 },
        },
        CaptureOptions::new(1280, 720),
        OutputOptions::mp4(path),
    )
    .map_err(|error| std::io::Error::other(error.to_string()))?;
    eprintln!(
        "Finalized {} frames with the authored soundtrack: {}",
        summary.capture.sampling.frames,
        summary.path.display()
    );
    Ok(())
}

/// Filesystem lookup is confined to this native preparation example. The engine
/// importer receives bytes and performs the authoritative schema/PCM checks.
#[cfg(all(
    not(target_arch = "wasm32"),
    feature = "narration-import",
    feature = "native-text",
    feature = "bundled-fonts"
))]
fn load_prepared(
    path: &std::path::Path,
) -> Result<noon::PreparedNarration, Box<dyn std::error::Error>> {
    use std::{fs::File, io::Read};

    fn read_bounded(path: &std::path::Path, limit: u64) -> std::io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > limit {
            return Err(std::io::Error::other(
                "prepared narration file exceeds its limit",
            ));
        }
        Ok(bytes)
    }

    let encoded = read_bounded(path, 1024 * 1024)?;
    let manifest = std::str::from_utf8(&encoded)?;
    let metadata: serde_json::Value = serde_json::from_str(manifest)?;
    let id = metadata["audio"]["asset_id"]
        .as_str()
        .filter(|id| {
            id.len() == 64
                && id
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        })
        .ok_or("invalid prepared audio identity")?;
    let expected = format!("../audio/{id}/audio.f32le");
    if metadata["audio"]["path"].as_str() != Some(expected.as_str()) {
        return Err("prepared audio must use its canonical cache-relative location".into());
    }
    let directory = path.parent().ok_or("manifest has no parent directory")?;
    let pcm = read_bounded(&directory.join(expected), 64 * 1024 * 1024)?;
    Ok(noon::PreparedNarration::from_prepared(manifest, &pcm)?)
}

#[cfg(target_arch = "wasm32")]
fn main() {}
