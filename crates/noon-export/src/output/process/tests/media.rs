//! Real codecs must preserve the serial reference's frames and rational PTS.
//! This is transport qualification with synthetic pixels, not an engine benchmark.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use noon::integration::FrameRate;

use super::super::Encoder;
use super::Fixture;
use crate::output::{encoder_command, OutputOptions};
use crate::CaptureCancellation;

const FRAMES: u64 = 17;

fn frame_pixels(width: u32, height: u32, index: u64) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            pixels.extend_from_slice(&[
                ((u64::from(x / 8) * 23 + index * 7) % 256) as u8,
                ((u64::from(y / 8) * 31 + index * 11) % 256) as u8,
                ((u64::from(x / 16) * 17 + index * 13) % 256) as u8,
                255,
            ]);
        }
    }
    pixels
}

fn encode(
    directory: &Path,
    rate: FrameRate,
    width: u32,
    height: u32,
    png: bool,
    pipelined: bool,
) {
    fs::create_dir(directory).unwrap();
    let options = if png {
        OutputOptions::png_sequence(directory)
    } else {
        OutputOptions::mp4(directory.join("video.mp4"))
    };
    let mut command = encoder_command(&options, rate, width, height, directory);
    let mut encoder = Encoder::new(
        &mut command,
        width as usize * height as usize * 4,
        CaptureCancellation::default(),
        Duration::from_secs(30),
    )
    .unwrap();
    for index in 0..FRAMES {
        let mut pixels = frame_pixels(width, height, index);
        if pipelined {
            encoder.enqueue(&pixels).unwrap();
        } else {
            encoder.write(&pixels).unwrap();
        }
        // The input must remain owned even when the producer immediately reuses
        // or drops its own memory before FFmpeg has completed its pipe read.
        pixels.fill(0);
    }
    // Deliberately do not call drain: finalization must account for all queued
    // writes, close stdin, and wait for the real codec and muxer to finish.
    encoder.finish().unwrap();
}

fn run(command: &mut Command) -> Vec<u8> {
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "media command {command:?} failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    result.stdout
}

fn decoded_video(path: &Path) -> Vec<u8> {
    run(Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-xerror", "-i"])
        .arg(path)
        .args([
            "-vf",
            "scale=in_range=limited:out_range=full:in_color_matrix=bt709,format=rgba",
            "-fps_mode",
            "passthrough",
            "-f",
            "rawvideo",
            "pipe:1",
        ]))
}

fn verify_timing(path: &Path, rate: FrameRate) -> Vec<u64> {
    let bytes = run(Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-count_frames",
            "-show_entries",
            "frame=pts:stream=r_frame_rate,avg_frame_rate,time_base,start_pts,duration_ts,nb_read_frames",
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(path));
    let output = String::from_utf8(bytes).unwrap();
    let mut pts = Vec::new();
    let mut stream = BTreeMap::new();
    for line in output.lines().filter(|line| !line.is_empty()) {
        let (key, value) = line.split_once('=').expect("key=value probe output");
        if key == "pts" {
            pts.push(value.parse::<u64>().unwrap());
        } else {
            assert!(stream.insert(key, value).is_none(), "duplicate stream field");
        }
    }
    let p = rate.numerator();
    let q = rate.denominator();
    let fraction = format!("{p}/{q}");
    let time_base = format!("1/{p}");
    let count = FRAMES.to_string();
    let duration = (FRAMES * u64::from(q)).to_string();
    for (key, expected) in [
        ("r_frame_rate", fraction.as_str()),
        ("avg_frame_rate", fraction.as_str()),
        ("time_base", time_base.as_str()),
        ("start_pts", "0"),
        ("duration_ts", duration.as_str()),
        ("nb_read_frames", count.as_str()),
    ] {
        assert_eq!(stream.get(key).copied(), Some(expected), "{key}");
    }
    assert_eq!(
        pts,
        (0..FRAMES).map(|i| i * u64::from(q)).collect::<Vec<_>>()
    );
    pts
}

#[test]
#[ignore = "requires FFmpeg and ffprobe; selected by native output gate"]
fn pipelined_mp4_matches_serial_decoded_frames_and_every_timestamp() {
    let fixture = Fixture::new();
    let (width, height) = (64, 32);
    for (p, q) in [(30, 1), (30_000, 1_001), (60_000, 1_001)] {
        let rate = FrameRate::new(p, q).unwrap();
        let serial = fixture.0.join(format!("serial-{p}"));
        let pipeline = fixture.0.join(format!("pipeline-{p}"));
        encode(&serial, rate, width, height, false, false);
        encode(&pipeline, rate, width, height, false, true);
        let serial_video = serial.join("video.mp4");
        let pipeline_video = pipeline.join("video.mp4");
        assert_eq!(
            verify_timing(&serial_video, rate),
            verify_timing(&pipeline_video, rate)
        );
        let expected = decoded_video(&serial_video);
        let actual = decoded_video(&pipeline_video);
        let frame_bytes = width as usize * height as usize * 4;
        assert_eq!(expected.len(), FRAMES as usize * frame_bytes);
        assert_eq!(actual.len(), expected.len());
        assert!(actual == expected, "pipeline changed decoded {p}/{q} pixels");
        // Ensure the oracle really distinguishes successive frames: a frozen or
        // repeated-frame fixture must not certify a pipeline's order.
        for index in 1..FRAMES as usize {
            let previous = &expected[(index - 1) * frame_bytes..index * frame_bytes];
            let current = &expected[index * frame_bytes..(index + 1) * frame_bytes];
            assert!(previous != current, "oracle repeated frame {index}");
        }
    }
}

fn decoded_pngs(directory: &Path, rate: FrameRate) -> Vec<u8> {
    run(Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-xerror", "-framerate"])
        .arg(format!("{}/{}", rate.numerator(), rate.denominator()))
        .args(["-start_number", "0", "-i"])
        .arg(directory.join("frame-%010d.png"))
        .args([
            "-fps_mode",
            "passthrough",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "pipe:1",
        ]))
}

#[test]
#[ignore = "requires FFmpeg; selected by native output gate"]
fn pipelined_png_matches_serial_and_input_bytes_at_odd_dimensions() {
    let fixture = Fixture::new();
    let (width, height) = (65, 33);
    let rate = FrameRate::new(60_000, 1_001).unwrap();
    let serial = fixture.0.join("serial-png");
    let pipeline = fixture.0.join("pipeline-png");
    encode(&serial, rate, width, height, true, false);
    encode(&pipeline, rate, width, height, true, true);
    let expected: Vec<u8> = (0..FRAMES)
        .flat_map(|i| frame_pixels(width, height, i))
        .collect();
    for directory in [&serial, &pipeline] {
        let mut names: Vec<_> = fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        let expected_names: Vec<_> = (0..FRAMES)
            .map(|index| format!("frame-{index:010}.png"))
            .collect();
        assert_eq!(names, expected_names);
        let actual = decoded_pngs(directory, rate);
        assert!(actual == expected, "PNG transport changed the input pixels");
    }
}
