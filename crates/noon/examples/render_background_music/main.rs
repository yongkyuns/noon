//! Offline asset preparation for the original "A Small Discovery" music bed.
//!
//! Run from the repository root:
//! `cargo run -p noon --no-default-features --release --example render_background_music -- /tmp/a-small-discovery.wav`
//! An optional second argument selects the sample rate (default: 48,000 Hz).
//! See README.md beside this file for the scoped composition/audio dedication.

mod music;

use std::fs::File;
use std::io::{BufWriter, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("a-small-discovery.wav");
    let rate = args
        .get(2)
        .map(|s| s.parse::<u32>())
        .transpose()?
        .unwrap_or(48_000);
    let started = std::time::Instant::now();
    let pcm = music::render_music(rate)?;
    let elapsed = started.elapsed();
    let samples = pcm.samples();
    let data_size = (samples.len() * std::mem::size_of::<i16>()) as u32;
    let mut wav = BufWriter::new(File::create(path)?);
    wav.write_all(b"RIFF")?;
    wav.write_all(&(36 + data_size).to_le_bytes())?;
    wav.write_all(b"WAVEfmt ")?;
    wav.write_all(&16_u32.to_le_bytes())?;
    wav.write_all(&1_u16.to_le_bytes())?; // Linear PCM.
    wav.write_all(&(music::CHANNELS as u16).to_le_bytes())?;
    wav.write_all(&pcm.sample_rate().to_le_bytes())?;
    wav.write_all(&(pcm.sample_rate() * music::CHANNELS as u32 * 2).to_le_bytes())?;
    wav.write_all(&(music::CHANNELS as u16 * 2).to_le_bytes())?;
    wav.write_all(&16_u16.to_le_bytes())?;
    wav.write_all(b"data")?;
    wav.write_all(&data_size.to_le_bytes())?;
    for sample in samples {
        let quantized = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        wav.write_all(&quantized.to_le_bytes())?;
    }
    wav.flush()?;

    let peak = samples
        .iter()
        .fold(0.0_f64, |p, s| p.max((*s as f64).abs()));
    let rms =
        (samples.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / samples.len() as f64).sqrt();
    let mean = samples.iter().map(|s| *s as f64).sum::<f64>() / samples.len() as f64;
    let max_step = samples.windows(4).step_by(2).fold(0.0_f64, |step, s| {
        step.max((s[2] as f64 - s[0] as f64).abs())
            .max((s[3] as f64 - s[1] as f64).abs())
    });
    let seam_l = samples[0] as f64 - samples[samples.len() - 2] as f64;
    let seam_r = samples[1] as f64 - samples[samples.len() - 1] as f64;
    println!(
        "{} | {} Hz | {} frames | {} seconds",
        music::TITLE,
        pcm.sample_rate(),
        pcm.frames(),
        music::DURATION_SECONDS
    );
    println!("render_seconds={:.3}", elapsed.as_secs_f64());
    println!("peak={peak:.9} ({:.2} dBFS)", 20.0 * peak.log10());
    println!("rms={rms:.9} ({:.2} dBFS)", 20.0 * rms.log10());
    println!("mean={mean:.12}");
    println!("max_adjacent_step={max_step:.9}");
    println!("loop_seam_step_left={seam_l:.9} right={seam_r:.9}");
    println!("wav={path}");
    Ok(())
}
