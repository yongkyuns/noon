//! Native MP4 input uses the same system FFmpeg installation as file export.

use std::{
    fs::File,
    io::{self, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStderr, ChildStdout, Command, Stdio},
    sync::{Arc, Mutex},
};

use noon_core::{AudioResource, VideoFrameIndex, VideoFrameSource, VideoFrameWindow};
use serde::Deserialize;

mod cache;
pub use cache::NativeVideoCache;

const MAX_INPUT_BYTES: u64 = 128 * 1024 * 1024;
const MAX_PROBE_BYTES: usize = 8 * 1024 * 1024;
const MAX_DURATION: f64 = 60.0;
const AUDIO_RATE: u32 = 48_000;
const AUDIO_BYTES: usize = AUDIO_RATE as usize * 2 * 4 * 61;

/// A prepared local MP4 asset. Owns a private copy so later edits to the source
/// path cannot change frames selected during replay or export.
#[derive(Debug)]
pub struct NativeVideoAsset {
    path: PathBuf,
    _storage: tempfile::TempDir,
    width: u32,
    height: u32,
    video_stream: u32,
    time_base: f64,
    packets: Box<[Packet]>,
    frames: VideoFrameIndex,
    audio: Option<(AudioResource, f64)>,
}

impl NativeVideoAsset {
    /// Inspect a bounded H.264 MP4 and prepare its optional AAC soundtrack.
    /// The video track is indexed by packet timestamps without decoding it.
    pub fn open(source: impl AsRef<Path>) -> io::Result<Self> {
        let source = File::open(source)?;
        if !source.metadata()?.is_file() {
            return Err(invalid("video input must be a local file"));
        }
        let storage = tempfile::tempdir()?;
        let path = storage.path().join("input.mp4");
        let mut snapshot = File::create(&path)?;
        let length = io::copy(&mut source.take(MAX_INPUT_BYTES + 1), &mut snapshot)?;
        if length > MAX_INPUT_BYTES {
            return Err(invalid("video input exceeds 128 MiB"));
        }
        snapshot.flush()?;
        drop(snapshot);

        let mut probe = Command::new("ffprobe");
        probe.args([
            "-v",
            "error",
            "-f",
            "mov",
            "-protocol_whitelist",
            "file,pipe",
        ]);
        probe.args([
            "-show_entries",
            "stream=index,codec_type,codec_name,time_base,start_pts,duration_ts,width,height:packet=stream_index,pts,flags",
            "-of",
            "json",
            "-i",
        ]);
        let bytes = read_output(probe.arg(&path), MAX_PROBE_BYTES, "index MP4 tracks")?;
        let Probe { streams, packets } =
            serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        let (video, audio) = match streams.as_slice() {
            [video] if video.codec_type == "video" => (video, None),
            [first, second] if first.codec_type == "video" && second.codec_type == "audio" => {
                (first, Some(second))
            }
            [first, second] if first.codec_type == "audio" && second.codec_type == "video" => {
                (second, Some(first))
            }
            _ => {
                return Err(invalid(
                    "expected one H.264 video track and an optional AAC track",
                ))
            }
        };
        let (Some(width), Some(height)) = (video.width, video.height) else {
            return Err(invalid("video stream has no dimensions"));
        };
        if video.codec_name != "h264"
            || width == 0
            || height == 0
            || width.max(height) > 1920
            || u64::from(width) * u64::from(height) > 1920 * 1080
        {
            return Err(invalid(
                "expected H.264 video within a 1920×1080 pixel budget",
            ));
        }
        let time_base = video.seconds_per_tick()?;
        let start = video
            .start_pts
            .ok_or_else(|| invalid("missing video start timestamp"))?;
        let duration = video
            .duration_ts
            .ok_or_else(|| invalid("missing video track duration"))?;
        let mut packets: Vec<_> = packets
            .into_iter()
            .filter(|packet| packet.stream_index == video.index)
            .filter(|packet| packet.pts.is_some_and(|pts| pts >= start))
            .filter(|packet| !packet.flags.contains('D'))
            .map(|packet| Packet {
                pts: packet.pts.expect("presentation timestamp was checked"),
                key: packet.flags.contains('K'),
            })
            .collect();
        packets.sort_unstable_by_key(|packet| packet.pts);
        let first = packets
            .first()
            .ok_or_else(|| invalid("video contains no presentable frames"))?;
        let end = start
            .checked_add(duration)
            .ok_or_else(|| invalid("video track duration overflows"))?;
        let duration = (end as f64 - first.pts as f64) * time_base;
        if duration > MAX_DURATION {
            return Err(invalid("video duration exceeds 60 seconds"));
        }
        let timestamps = packets
            .iter()
            .map(|packet| (packet.pts as f64 - first.pts as f64) * time_base)
            .collect::<Vec<_>>();
        let frames = VideoFrameIndex::new(timestamps, duration).map_err(invalid)?;
        let audio = audio
            .map(|audio| decode_audio(&path, audio, first.pts as f64 * time_base))
            .transpose()?;
        Ok(Self {
            path,
            _storage: storage,
            width,
            height,
            video_stream: video.index,
            time_base,
            packets: packets.into_boxed_slice(),
            frames,
            audio,
        })
    }

    pub const fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn frame_index(&self) -> &VideoFrameIndex {
        &self.frames
    }

    /// Decoded stereo PCM and its offset from the first video frame.
    pub fn soundtrack(&self) -> Option<(&AudioResource, f64)> {
        self.audio.as_ref().map(|(pcm, offset)| (pcm, *offset))
    }

    /// Decode sequentially from a requested presentation frame. Call on a
    /// decoder worker. Dropping the reader cancels and reaps its process.
    pub fn decode_from(&self, frame: usize) -> io::Result<NativeVideoFrames<'_>> {
        let target = self
            .packets
            .get(frame)
            .ok_or_else(|| invalid("video frame is outside the asset"))?;
        let mut command = ffmpeg();
        if let Some(key) = self.packets[..=frame]
            .iter()
            .rev()
            .find(|packet| packet.key && packet.pts > 0)
        {
            let key_time = key.pts as f64 * self.time_base;
            // FFmpeg seeks with microsecond precision. Round toward the
            // preceding keyframe so sub-microsecond rounding cannot skip it.
            let seek = (key_time * 1_000_000.0).floor() / 1_000_000.0;
            command.args([
                "-seek_timestamp",
                "1",
                "-noaccurate_seek",
                "-ss",
                &format!("{seek:.6}"),
            ]);
        }
        command.args(["-threads", "2", "-noautorotate"]);
        input(&mut command, &self.path);
        command.args([
            "-map",
            &format!("0:{}", self.video_stream),
            "-an",
            "-vf",
            &format!("trim=start_pts={},showinfo=checksum=0", target.pts),
            "-fps_mode",
            "passthrough",
            "-threads:v",
            "1",
            "-pix_fmt",
            "rgba",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
        let mut process = Process::start(&mut command, Stdio::piped(), "decode video")?;
        let pixels = process.0.stdout.take().expect("video pixel pipe");
        let timestamps = BufReader::new(process.0.stderr.take().expect("video timestamp pipe"));
        Ok(NativeVideoFrames {
            process: Arc::new(Mutex::new(process)),
            pixels,
            timestamps,
            asset: self,
            next: frame,
        })
    }
}

impl VideoFrameSource for NativeVideoAsset {
    fn dimensions(&self) -> (u32, u32) {
        self.dimensions()
    }

    fn frame_index(&self) -> &VideoFrameIndex {
        self.frame_index()
    }

    fn open_window(
        self: Arc<Self>,
        wake: Box<dyn Fn() + Send>,
    ) -> io::Result<Box<dyn VideoFrameWindow>> {
        Ok(Box::new(NativeVideoCache::new(self, wake)?))
    }
}

fn ffmpeg() -> Command {
    let mut command = Command::new("ffmpeg");
    command.args([
        "-hide_banner",
        "-nostdin",
        "-nostats",
        "-loglevel",
        "info",
        "-xerror",
        "-copyts",
    ]);
    command
}

fn input(command: &mut Command, path: &Path) {
    command
        .args(["-f", "mov", "-protocol_whitelist", "file,pipe", "-i"])
        .arg(path);
}

fn decode_audio(
    path: &Path,
    stream: &Stream,
    video_start: f64,
) -> io::Result<(AudioResource, f64)> {
    if stream.codec_name != "aac" {
        return Err(invalid("the optional MP4 audio track must be AAC"));
    }
    let start = stream
        .start_pts
        .ok_or_else(|| invalid("missing audio start timestamp"))?;
    let ticks = (start as f64 * stream.seconds_per_tick()? * f64::from(AUDIO_RATE)).round();
    let mut command = ffmpeg();
    input(&mut command, path);
    command.args([
        "-map",
        &format!("0:{}", stream.index),
        "-vn",
        "-af",
        &format!("aresample={AUDIO_RATE}:async=1:first_pts={ticks:.0}"),
        "-ac",
        "2",
        "-f",
        "f32le",
        "pipe:1",
    ]);
    let bytes = read_output(&mut command, AUDIO_BYTES, "decode AAC soundtrack")?;
    let (samples, remainder) = bytes.as_chunks::<4>();
    if !remainder.is_empty() {
        return Err(invalid("the AAC decoder returned an incomplete PCM sample"));
    }
    let samples: Vec<_> = samples.iter().copied().map(f32::from_le_bytes).collect();
    let pcm =
        AudioResource::from_interleaved(AUDIO_RATE, 2, samples, AUDIO_BYTES).map_err(invalid)?;
    Ok((pcm, ticks / f64::from(AUDIO_RATE) - video_start))
}

/// One sequential decode from a verified timestamp. Holds no scene clock.
pub struct NativeVideoFrames<'a> {
    process: Arc<Mutex<Process>>,
    pixels: ChildStdout,
    timestamps: BufReader<ChildStderr>,
    asset: &'a NativeVideoAsset,
    next: usize,
}

impl NativeVideoFrames<'_> {
    pub fn read_frame(&mut self) -> io::Result<Option<Vec<u8>>> {
        let Some(expected) = self.asset.packets.get(self.next) else {
            return Ok(None);
        };
        if read_pts(&mut self.timestamps)? != expected.pts {
            return Err(invalid(
                "decoded frame timestamp disagrees with the MP4 index",
            ));
        }
        let (width, height) = self.asset.dimensions();
        let mut pixels = vec![0; width as usize * height as usize * 4];
        self.pixels.read_exact(&mut pixels).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("could not read decoded RGBA frame: {error}"),
            )
        })?;
        self.next += 1;
        Ok(Some(pixels))
    }
}

fn read_pts(reader: &mut impl BufRead) -> io::Result<i64> {
    let mut line = String::new();
    while reader.read_line(&mut line)? != 0 {
        if line.starts_with("[Parsed_showinfo_") && line.contains(" pts:") {
            return line
                .split_once(" pts:")
                .and_then(|(_, rest)| rest.split_ascii_whitespace().next())
                .and_then(|pts| pts.parse().ok())
                .ok_or_else(|| invalid("FFmpeg returned an invalid video timestamp"));
        }
        line.clear();
    }
    Err(invalid("FFmpeg ended before the indexed video timestamp"))
}

struct Process(Child);

impl Process {
    fn start(command: &mut Command, stderr: Stdio, operation: &str) -> io::Result<Self> {
        command
            .stdin(Stdio::null())
            .stderr(stderr)
            .stdout(Stdio::piped())
            .spawn()
            .map(Self)
            .map_err(|error| {
                io::Error::new(error.kind(), format!("could not {operation}: {error}"))
            })
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn read_output(command: &mut Command, limit: usize, operation: &str) -> io::Result<Vec<u8>> {
    let mut process = Process::start(command, Stdio::null(), operation)?;
    let mut bytes = Vec::new();
    process
        .0
        .stdout
        .take()
        .expect("child output pipe")
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(invalid(format!(
            "could not {operation}: output exceeds the input profile budget"
        )));
    }
    let status = process.0.wait()?;
    if !status.success() {
        return Err(invalid(format!(
            "could not {operation}: FFmpeg tool returned {status}"
        )));
    }
    Ok(bytes)
}

fn invalid(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[derive(Deserialize)]
struct Probe {
    streams: Vec<Stream>,
    packets: Vec<ProbePacket>,
}

#[derive(Deserialize)]
struct Stream {
    index: u32,
    codec_type: String,
    codec_name: String,
    time_base: String,
    start_pts: Option<i64>,
    duration_ts: Option<i64>,
    width: Option<u32>,
    height: Option<u32>,
}

impl Stream {
    fn seconds_per_tick(&self) -> io::Result<f64> {
        self.time_base
            .split_once('/')
            .and_then(|(n, d)| Some((n.parse::<u32>().ok()?, d.parse::<u32>().ok()?)))
            .filter(|&(n, d)| n != 0 && d != 0)
            .map(|(n, d)| f64::from(n) / f64::from(d))
            .ok_or_else(|| invalid("invalid MP4 stream time base"))
    }
}

#[derive(Deserialize)]
struct ProbePacket {
    stream_index: u32,
    pts: Option<i64>,
    flags: String,
}

#[derive(Debug)]
struct Packet {
    pts: i64,
    key: bool,
}
