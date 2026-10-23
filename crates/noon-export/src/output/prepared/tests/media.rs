//! Real codec qualification of the explicit media boundary, without a GPU.

use super::*;
use noon::{AudioClipOptions, AudioResource, Scene};
use std::process::Command;

fn command(command: &mut Command) -> Vec<u8> {
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    result.stdout
}

fn probe(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&command(
        Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_streams",
                "-show_format",
                "-of",
                "json",
            ])
            .arg(path),
    ))
    .unwrap()
}

fn stream<'a>(probe: &'a serde_json::Value, kind: &str) -> &'a serde_json::Value {
    probe["streams"]
        .as_array()
        .unwrap()
        .iter()
        .find(|stream| stream["codec_type"] == kind)
        .unwrap()
}

fn duration(value: &serde_json::Value) -> f64 {
    value["duration"].as_str().unwrap().parse().unwrap()
}

fn decode_pcm(path: &Path) -> Vec<f32> {
    let bytes = command(
        Command::new("ffmpeg")
            .args(["-v", "error", "-nostdin", "-i"])
            .arg(path)
            .args([
                "-map",
                "0:a:0",
                "-c:a",
                "pcm_f32le",
                "-f",
                "f32le",
                "pipe:1",
            ]),
    );
    assert_eq!(bytes.len() % 8, 0);
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| f32::from_le_bytes(*bytes))
        .collect()
}

#[test]
#[ignore = "requires FFmpeg and ffprobe; selected by native output gate"]
fn prepared_movie_preserves_cropped_shared_pcm_pixels_and_exact_partial_endpoint() {
    let root = Temp::new();
    let mut options = options();
    options.start_frame = 15;
    let mut manifest = manifest(options, 2.03, true);
    manifest.width = 64;
    manifest.height = 48;
    let sampling = write_media(&root.0, &manifest);
    assert_eq!(sampling.frames, 46);
    assert_eq!(sampling.start_time, 0.5005);
    let mut rgba = Vec::new();
    for frame in 0..sampling.frames {
        rgba.extend_from_slice(&[40 + frame as u8 * 3, 60, 130, 255].repeat(64 * 48));
    }
    fs::write(root.0.join(FRAMES_FILENAME), &rgba).unwrap();

    // The fixture PCM comes from the same shared Rust mixer as native scene
    // export. Both clips retain their absolute scene phase through the crop.
    let mut scene = Scene::new();
    let samples = (0..12_000)
        .flat_map(|index| {
            std::array::from_fn::<_, 2, _>(|channel| {
                let hz = [440.0, 660.0][channel];
                let phase = std::f64::consts::TAU * hz * index as f64 / 48_000.0;
                (0.2 * phase.sin()) as f32
            })
        })
        .collect::<Vec<_>>();
    let resource = AudioResource::from_interleaved(48_000, 2, samples, 128 * 1024).unwrap();
    for (start_time, duration) in [(0.25, 0.5), (1.25, 0.78)] {
        scene
            .audio(
                resource.clone(),
                AudioClipOptions {
                    start_time,
                    source_start_frame: 600,
                    source_end_frame: Some(6_600),
                    duration: Some(duration),
                    looped: true,
                    fade_in: 0.02,
                    fade_out: 0.02,
                    ..AudioClipOptions::default()
                },
            )
            .unwrap();
    }
    let session = scene.execution_session().unwrap();
    let publication = session.publication_context();
    audio::Soundtrack::new(&session, sampling)
        .unwrap()
        .stage(&root.0, &CaptureCancellation::default())
        .unwrap();
    assert_eq!(session.frame().time, 0.0);
    assert_eq!(session.publication_context(), publication);
    let expected_pcm = fs::read(root.0.join(PCM_FILENAME))
        .unwrap()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| f32::from_le_bytes(*bytes))
        .collect::<Vec<_>>();

    let path = root.0.join("movie.mp4");
    let result = export_prepared_capture(
        &root.0,
        OutputOptions::mp4(&path),
        PreparedCaptureLimits::default(),
        CaptureCancellation::default(),
    )
    .unwrap();
    assert_eq!(result.sampling, sampling);
    assert_eq!(result.audio_frames, Some(73_674));
    assert_no_staging(&root.0);
    let evidence = probe(&path);
    let video = stream(&evidence, "video");
    let audio = stream(&evidence, "audio");
    assert_eq!(evidence["streams"].as_array().unwrap().len(), 2);
    assert_eq!(video["codec_name"], "h264");
    assert_eq!(video["nb_frames"], "46");
    assert_eq!(video["time_base"], "1/30000");
    assert_eq!(video["start_time"], "0.000000");
    assert_eq!(video["duration_ts"], 45_885);
    assert_eq!(audio["codec_name"], "aac");
    assert_eq!(audio["channels"], 2);
    assert_eq!(audio["sample_rate"], "48000");
    assert_eq!(audio["start_time"], "0.000000");
    assert_eq!(audio["duration_ts"], 73_416);
    for entry in [&evidence["format"], video, audio] {
        assert!((duration(entry) - 1.5295).abs() < 1e-6, "{entry}");
    }
    let decoded = decode_pcm(&path);
    // A decoder may expose AAC's final padded block; timing is independently
    // checked above. Correlate only the exact presentation interval.
    let presented = 73_416 * 2;
    assert!(decoded.len() >= presented);
    let mut cross = 0.0;
    let mut expected_power = 0.0;
    let mut actual_power = 0.0;
    for (&actual, &expected) in decoded[..presented].iter().zip(&expected_pcm) {
        assert!(actual.is_finite());
        cross += f64::from(actual) * f64::from(expected);
        expected_power += f64::from(expected).powi(2);
        actual_power += f64::from(actual).powi(2);
    }
    assert!(cross / (expected_power * actual_power).sqrt() > 0.97);
    assert!((0.85..1.15).contains(&(actual_power / expected_power).sqrt()));
    let gap = &decoded[16_800 * 2..31_200 * 2];
    let gap_rms = (gap
        .iter()
        .map(|value| f64::from(*value).powi(2))
        .sum::<f64>()
        / gap.len() as f64)
        .sqrt();
    assert!(gap_rms < 0.008, "silent gap RMS {gap_rms}");

    let pixels = command(
        Command::new("ffmpeg")
            .args(["-v", "error", "-nostdin", "-i"])
            .arg(&path)
            .args([
                "-map",
                "0:v:0",
                "-fps_mode",
                "passthrough",
                "-pix_fmt",
                "rgba",
                "-f",
                "rawvideo",
                "pipe:1",
            ]),
    );
    assert_eq!(pixels.len(), rgba.len());
    for (encoded, expected) in pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(rgba.as_chunks::<4>().0)
    {
        assert_eq!(encoded[3], 255);
        for channel in 0..3 {
            assert!((i16::from(encoded[channel]) - i16::from(expected[channel])).abs() <= 8);
        }
    }
}

#[test]
#[ignore = "requires FFmpeg and ffprobe; selected by native output gate"]
fn prepared_video_only_hold_reuses_terminal_publication_without_an_audio_track() {
    let root = Temp::new();
    let mut options = options();
    options.start_frame = 0;
    options.frame_rate = FrameRate::new(20, 1).unwrap();
    options.final_hold_seconds = 0.15;
    let manifest = manifest(options, 0.2, false);
    write_media(&root.0, &manifest);
    let path = root.0.join("hold.mp4");
    let result = export_prepared_capture(
        &root.0,
        OutputOptions::mp4(&path),
        PreparedCaptureLimits::default(),
        CaptureCancellation::default(),
    )
    .unwrap();
    assert_eq!(result.sampling.frames, 7);
    assert_eq!(result.audio_frames, None);
    let evidence = probe(&path);
    assert_eq!(evidence["streams"].as_array().unwrap().len(), 1);
    assert_eq!(stream(&evidence, "video")["nb_frames"], "7");
    assert!((duration(&evidence["format"]) - 0.35).abs() < 1e-6);
    assert_no_staging(&root.0);
}

#[test]
#[ignore = "requires FFmpeg codec probe; selected by native output gate"]
fn malformed_prepared_media_cleans_staging_and_never_replaces_existing_movie() {
    let root = Temp::new();
    let manifest = manifest(options(), 0.2, true);
    let path = root.0.join("keep.mp4");
    fs::write(&path, b"original movie").unwrap();
    let mut output = OutputOptions::mp4(&path);
    output.overwrite = true;
    for corrupt_audio in [true, false] {
        write_media(&root.0, &manifest);
        let filename = if corrupt_audio {
            PCM_FILENAME
        } else {
            FRAMES_FILENAME
        };
        let mut bytes = fs::read(root.0.join(filename)).unwrap();
        if corrupt_audio {
            bytes[..4].copy_from_slice(&f32::NAN.to_le_bytes());
        } else {
            bytes[3] = 128;
        }
        fs::write(root.0.join(filename), bytes).unwrap();
        let error = export_prepared_capture(
            &root.0,
            output.clone(),
            PreparedCaptureLimits::default(),
            CaptureCancellation::default(),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(fs::read(&path).unwrap(), b"original movie");
        assert_no_staging(&root.0);
    }
}
