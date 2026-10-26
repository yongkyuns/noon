use std::fs;
use std::io::Write;
use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "noon-soundtrack-duration-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Fixture {
    _temp: Temp,
    path: PathBuf,
    rate: FrameRate,
    frames: u64,
    pcm: Vec<f32>,
}

impl Fixture {
    fn new(p: u32, q: u32, frames: u64, crop_start: f64, audio_first: bool) -> Self {
        let temp = Temp::new();
        let path = temp.0.join("soundtrack.mp4");
        let pcm_path = temp.0.join("soundtrack.f32");
        let count = (u128::from(frames) * u128::from(q) * u128::from(AUDIO_RATE))
            .div_ceil(u128::from(p)) as usize;
        let mut pcm = Vec::with_capacity(count * 2);
        for frame in 0..count {
            let time = frame as f64 / f64::from(AUDIO_RATE);
            // Smooth, nonperiodic phase makes a lost 1024-frame priming offset
            // distinguishable from both lossy coding error and a channel swap.
            let envelope = if (0.02..0.22).contains(&time) {
                (std::f64::consts::PI * (time - 0.02) / 0.2).sin().powi(2)
            } else {
                0.0
            };
            let authored = crop_start + time;
            let phase = std::f64::consts::TAU * (733.0 * authored + 117.0 * authored * authored);
            pcm.push((0.25 * envelope * phase.sin()) as f32);
            pcm.push((0.18 * envelope * (phase * 1.37).sin()) as f32);
        }
        let bytes: Vec<_> = pcm.iter().flat_map(|sample| sample.to_le_bytes()).collect();
        fs::write(&pcm_path, bytes).unwrap();
        let mut command = Command::new("ffmpeg");
        command.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-y",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
            "4x4",
            "-framerate",
        ]);
        command.arg(format!("{p}/{q}")).args([
            "-i", "pipe:0", "-f", "f32le", "-ar", "48000", "-ac", "2", "-i",
        ]);
        command.arg(&pcm_path);
        if audio_first {
            command.args(["-map", "1:a:0", "-map", "0:v:0"]);
        } else {
            command.args(["-map", "0:v:0", "-map", "1:a:0"]);
        }
        command.args([
            "-sn",
            "-dn",
            "-fps_mode",
            "passthrough",
            "-vf",
            "format=yuv420p",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "18",
            "-bf",
            "0",
            "-enc_time_base:v",
        ]);
        command
            .arg(format!("{q}:{p}"))
            .arg("-video_track_timescale")
            .arg(p.to_string())
            .args([
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                "-movflags",
                "+faststart",
                "-f",
                "mp4",
            ])
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .expect("FFmpeg is required by this media test");
        let mut stdin = child.stdin.take().unwrap();
        for frame in 0..frames {
            let pixel = [30 + (frame % 100) as u8, 70, 150, 255];
            stdin.write_all(&pixel.repeat(16)).unwrap();
        }
        drop(stdin);
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Self {
            _temp: temp,
            path,
            rate: FrameRate::new(p, q).unwrap(),
            frames,
            pcm,
        }
    }

    fn pcm_frames(&self) -> u64 {
        (self.pcm.len() / 2) as u64
    }
}

fn media_command(program: &str, arguments: &[&str], path: &Path) -> Vec<u8> {
    let output = Command::new(program)
        .args(arguments)
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn packets(path: &Path, stream: &str) -> Vec<(i64, u64)> {
    let bytes = media_command(
        "ffprobe",
        &[
            "-v",
            "error",
            "-select_streams",
            stream,
            "-show_packets",
            "-show_entries",
            "packet=pts,duration",
            "-of",
            "csv=p=0",
        ],
        path,
    );
    String::from_utf8(bytes)
        .unwrap()
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let mut values = line.split(',');
            (
                values.next().unwrap().parse().unwrap(),
                values.next().unwrap().parse().unwrap(),
            )
        })
        .collect()
}

fn decode(path: &Path, audio: bool, ignore_edits: bool) -> Vec<u8> {
    let mut command = Command::new("ffmpeg");
    command.args(["-v", "error", "-nostdin"]);
    if ignore_edits {
        command.args(["-ignore_editlist", "1"]);
    }
    command.arg("-i").arg(path);
    if audio {
        command.args(["-map", "0:a:0", "-c:a", "pcm_f32le", "-f", "f32le"]);
    } else {
        command.args([
            "-map",
            "0:v:0",
            "-pix_fmt",
            "rgba",
            "-fps_mode",
            "passthrough",
            "-f",
            "rawvideo",
        ]);
    }
    let output = command.arg("pipe:1").output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn top_atom(bytes: &[u8], wanted: [u8; 4]) -> Atom {
    let mut cursor = 0;
    loop {
        let (kind, atom) = atom_at(bytes, cursor, bytes.len()).unwrap();
        if kind == wanted {
            return atom;
        }
        cursor = atom.end;
    }
}

fn assert_duration_and_samples(fixture: &Fixture, end: u32, before: &[u8], before_audio: &[u8]) {
    let after = fs::read(&fixture.path).unwrap();
    let old_mdat = top_atom(before, *b"mdat");
    let new_mdat = top_atom(&after, *b"mdat");
    assert_eq!(old_mdat.start, new_mdat.start);
    assert_eq!(before.len(), after.len());
    assert_eq!(
        &before[old_mdat.start..old_mdat.end],
        &after[new_mdat.start..new_mdat.end]
    );

    let moov = top_atom(&after, *b"moov");
    let mvhd = child(&after, moov, *b"mvhd").unwrap();
    let (video, audio) = tracks(&after, moov).unwrap();
    let movie_scale = u32_at(&after, mvhd.payload() + 12).unwrap();
    assert_eq!(movie_scale % fixture.rate.numerator(), 0);
    assert_eq!(movie_scale % AUDIO_RATE, 0);
    let video_movie_end =
        u64::from(end) * u64::from(movie_scale) / u64::from(fixture.rate.numerator());
    let audio_end = u64::from(end) * u64::from(AUDIO_RATE) / u64::from(fixture.rate.numerator());
    let audio_movie_end = audio_end * u64::from(movie_scale) / u64::from(AUDIO_RATE);
    assert!(audio_movie_end <= video_movie_end);
    assert!((video_movie_end - audio_movie_end) * u64::from(AUDIO_RATE) < u64::from(movie_scale));
    assert_eq!(
        u64::from(u32_at(&after, mvhd.payload() + 16).unwrap()),
        video_movie_end
    );
    for (track, duration) in [(video, video_movie_end), (audio, audio_movie_end)] {
        assert_eq!(
            u64::from(u32_at(&after, track.tkhd.payload() + 20).unwrap()),
            duration
        );
        assert_eq!(
            u64::from(u32_at(&after, track.elst.payload() + 8).unwrap()),
            duration
        );
    }
    let reported = media_command(
        "ffprobe",
        &[
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
        ],
        &fixture.path,
    );
    let reported: f64 = String::from_utf8(reported).unwrap().trim().parse().unwrap();
    let expected_seconds = f64::from(end) / f64::from(fixture.rate.numerator());
    assert!(
        (reported - expected_seconds).abs() < 1.0e-6,
        "reported movie duration {reported} must follow exact video end {expected_seconds}, not a rounded-up audio sample",
    );
    assert_eq!(
        u32_at(&after, audio.elst.payload() + 12).unwrap(),
        AAC_FRAME
    );
    assert_eq!(
        u32_at(&after, audio.mdhd.payload() + 12).unwrap(),
        AUDIO_RATE
    );
    assert_eq!(
        u32_at(&after, audio.mdhd.payload() + 16).unwrap(),
        fixture.pcm_frames() as u32 + AAC_FRAME,
    );
    let video_packets = packets(&fixture.path, "v:0");
    assert_eq!(video_packets.len() as u64, fixture.frames);
    for (index, &(pts, duration)) in video_packets.iter().enumerate() {
        let expected_pts = index as u64 * u64::from(fixture.rate.denominator());
        assert_eq!(pts, expected_pts as i64);
        assert_eq!(
            duration,
            if index + 1 == video_packets.len() {
                u64::from(end) - expected_pts
            } else {
                u64::from(fixture.rate.denominator())
            },
        );
    }
    let after_audio = decode(&fixture.path, true, false);
    if audio_end == 0 {
        // A movie shorter than one complete 48 kHz sample has an exact container
        // endpoint but no presented audio samples; it is not an audible clip.
        let duration = media_command(
            "ffprobe",
            &[
                "-v",
                "error",
                "-select_streams",
                "a:0",
                "-show_entries",
                "stream=duration_ts",
                "-of",
                "csv=p=0",
            ],
            &fixture.path,
        );
        assert_eq!(String::from_utf8(duration).unwrap().trim(), "0");
        assert!(after_audio.is_empty());
    } else {
        let audio_packets = packets(&fixture.path, "a:0");
        assert_eq!(
            audio_packets[0],
            (-i64::from(AAC_FRAME), u64::from(AAC_FRAME))
        );
        let &(last_pts, last_duration) = audio_packets.last().unwrap();
        assert_eq!(last_pts as u64 + last_duration, audio_end);
    }
    let accepted_bytes = audio_end as usize * 2 * size_of::<f32>();
    // AAC decodes whole 1024-sample frames. Compare the admitted presentation
    // range, not the padded raw-decoder byte count, against the original decode.
    assert!(after_audio.len() >= accepted_bytes);
    assert_eq!(
        &after_audio[..accepted_bytes],
        &before_audio[..accepted_bytes]
    );
    let mut squared_error = 0.0_f64;
    let mut signal_energy = 0.0_f64;
    for (bytes, &expected) in after_audio[..accepted_bytes]
        .as_chunks::<4>()
        .0
        .iter()
        .zip(&fixture.pcm)
    {
        let observed = f32::from_le_bytes(*bytes);
        squared_error += f64::from(observed - expected).powi(2);
        signal_energy += f64::from(expected).powi(2);
    }
    if signal_energy > 1.0 {
        assert!(
            squared_error < signal_energy * 0.0025,
            "AAC waveform must retain authored sample alignment: relative RMS {}",
            (squared_error / signal_energy).sqrt(),
        );
    }
}

#[test]
#[ignore = "requires FFmpeg/ffprobe; selected by native output gate"]
fn soundtrack_preserves_exact_video_grid_crop_priming_and_decoded_alignment() {
    for (p, q, frames, crop, requested, end, audio_first) in [
        (60_000, 1_001, 120, 0.0, Some(2.0), 120_000, false),
        (
            30_000,
            1_001,
            70,
            20.0 * 1_001.0 / 30_000.0,
            Some(69_980.0 / 30_000.0),
            69_980,
            true,
        ),
        // This crop ends at 23828.8 PCM samples. Rounding audio upward used to
        // extend ffprobe FORMAT.duration beyond the exact 14893/30000 movie end.
        (
            30_000,
            1_001,
            15,
            7.0 * 1_001.0 / 30_000.0,
            Some(0.73 - 7.0 * 1_001.0 / 30_000.0),
            14_893,
            false,
        ),
        (30_000, 1_001, 1, 0.0, Some(0.01), 300, false),
        // Exact 100 microseconds and a tiny natural end finer than video ticks.
        (60_000, 1_001, 1, 0.0, Some(0.0001), 6, false),
        (30_000, 1_001, 1, 0.0, Some(0.00015), 1_001, false),
        // Very short PCM input exercises movie-edit rounding around priming.
        (30_000, 1, 1, 0.0, None, 1, false),
        (60_000, 1, 1, 0.0, None, 1, false),
        (1_000_000, 1, 1, 0.0, None, 1, false),
        // Full-grid output/final holds still need exact movie/audio edit units.
        (60_000, 1_001, 119, 0.0, None, 119_119, true),
        // The existing video endpoint fallback remains the whole CFR sample.
        (24, 1, 8, 0.0, Some(0.3), 8, false),
    ] {
        let fixture = Fixture::new(p, q, frames, crop, audio_first);
        let before = fs::read(&fixture.path).unwrap();
        // The original millisecond edit can itself truncate a tiny soundtrack
        // to zero. Decode its immutable media and skip the known priming frame
        // to get the complete PCM oracle, independent of presentation rounding.
        let mut before_audio = decode(&fixture.path, true, true);
        let before_audio = before_audio.split_off(AAC_FRAME as usize * 2 * size_of::<f32>());
        let before_video = decode(&fixture.path, false, false);
        assert_eq!(
            finish(
                &fixture.path,
                fixture.rate,
                frames,
                requested,
                fixture.pcm_frames()
            )
            .unwrap(),
            u64::from(end) != frames * u64::from(q),
        );
        assert_duration_and_samples(&fixture, end, &before, &before_audio);
        assert_eq!(decode(&fixture.path, false, false), before_video);
    }
}

#[test]
#[ignore = "requires FFmpeg/ffprobe; selected by native output gate"]
fn unsupported_soundtrack_profiles_leave_the_entire_file_unchanged() {
    let fixture = Fixture::new(30_000, 1_001, 70, 0.0, false);
    let original = fs::read(&fixture.path).unwrap();
    let moov = top_atom(&original, *b"moov");
    let (_, audio) = tracks(&original, moov).unwrap();
    let hdlr = child(&original, audio.mdia, *b"hdlr").unwrap();
    for (offset, replacement) in [
        (audio.elst.payload() + 12, 0_u32.to_be_bytes()),
        (audio.mdhd.payload() + 12, 44_100_u32.to_be_bytes()),
        (audio.stts.payload() + 12, 960_u32.to_be_bytes()),
        (hdlr.payload() + 8, *b"vide"),
    ] {
        let mut unsupported = original.clone();
        unsupported[offset..offset + 4].copy_from_slice(&replacement);
        fs::write(&fixture.path, &unsupported).unwrap();
        assert!(finish(
            &fixture.path,
            fixture.rate,
            fixture.frames,
            Some(2.33),
            fixture.pcm_frames()
        )
        .is_err());
        assert_eq!(fs::read(&fixture.path).unwrap(), unsupported);
    }
    // AAC-LC with another sample frequency cannot borrow this timing profile.
    let config = original
        .windows(5)
        .position(|bytes| bytes == [0x11, 0x90, 0x56, 0xe5, 0])
        .unwrap();
    let mut unsupported = original.clone();
    unsupported[config] = 0x12;
    fs::write(&fixture.path, &unsupported).unwrap();
    assert!(finish(
        &fixture.path,
        fixture.rate,
        fixture.frames,
        None,
        fixture.pcm_frames()
    )
    .is_err());
    assert_eq!(fs::read(&fixture.path).unwrap(), unsupported);
    fs::write(&fixture.path, &original).unwrap();
    assert!(finish(
        &fixture.path,
        fixture.rate,
        fixture.frames,
        None,
        fixture.pcm_frames() - 1
    )
    .is_err());
    assert_eq!(fs::read(&fixture.path).unwrap(), original);
}

#[test]
fn descriptor_lengths_cannot_escape_their_parent() {
    for invalid in [
        &[][..],
        &[3],
        &[3, 0x80],
        &[3, 0x80, 0x80, 0x80, 0x80],
        &[3, 4, 1],
    ] {
        assert!(descriptor(invalid).is_err());
    }
    let (tag, content, remainder) = descriptor(&[5, 0x80, 0x80, 0x80, 2, 0x11, 0x90, 6]).unwrap();
    assert_eq!((tag, content, remainder), (5, &[0x11, 0x90][..], &[6][..]));
}

#[test]
fn known_movie_and_priming_limits_are_rejected_without_staging() {
    use super::super::validate_soundtrack_grid;

    let fractional = FrameRate::new(30_000, 1_001).unwrap();
    assert!(validate_soundtrack_grid(fractional, 15, 24_024).is_ok());
    assert!(validate_soundtrack_grid(fractional, 15, 24_023).is_err());
    assert!(validate_soundtrack_grid(fractional, 0, 0).is_err());
    // Video and PCM individually fit, but their common movie clock does not.
    assert!(validate_soundtrack_grid(fractional, 500_000, 800_800_000).is_ok());
    assert!(validate_soundtrack_grid(fractional, 600_000, 960_960_000).is_err());
    let large_coprime_rate = FrameRate::new(i32::MAX as u32, 1).unwrap();
    assert!(validate_soundtrack_grid(large_coprime_rate, 1, 1).is_err());
    // The PCM payload fits the media field, but its AAC priming does not.
    let one_sample_per_video_frame = FrameRate::new(AUDIO_RATE, 1).unwrap();
    assert!(validate_soundtrack_grid(
        one_sample_per_video_frame,
        u64::from(u32::MAX),
        u64::from(u32::MAX),
    )
    .is_err());
}
