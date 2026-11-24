//! File staging for the shared immutable PCM mixer, not another audio timeline.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use noon::integration::scene_execution::ExecutionSession;
use noon::integration::{
    AudioMixFormat, ExportAudioGrid, ExportFrameSummary, MAX_AUDIO_MIX_BLOCK_FRAMES,
};

use super::check_publication_cancellation;
use crate::CaptureCancellation;

pub(super) const SAMPLE_RATE: u32 = 48_000;
pub(super) const CHANNELS: usize = 2;
pub(super) const PCM_FILENAME: &str = "audio.f32le";

/// One private PCM file for the existing encoder, whether mixed in this process
/// or received from an explicit export boundary. This is output staging only.
pub(super) trait StagedSoundtrack {
    fn frames(&self) -> u64;
    fn stage(&mut self, directory: &Path, cancellation: &CaptureCancellation) -> io::Result<()>;
}

pub(super) fn soundtrack_grid(sampling: ExportFrameSummary) -> io::Result<ExportAudioGrid> {
    let grid = ExportAudioGrid::new(sampling, AudioMixFormat::STEREO_48_KHZ).map_err(invalid)?;
    super::terminal_duration::validate_soundtrack_grid(
        sampling.frame_rate,
        sampling.frames,
        grid.frames(),
    )?;
    Ok(grid)
}

/// Borrow the same immutable plan used for visual capture. No PCM copy or
/// mutable playhead is retained. The full video grid is padded with silence;
/// an explicit authored stop never leaks later audio into its final video frame.
pub(super) struct Soundtrack<'a> {
    session: &'a ExecutionSession,
    scene_start: f64,
    pub frames: u64,
    mixed_frames: u64,
}

impl<'a> Soundtrack<'a> {
    pub fn new(session: &'a ExecutionSession, sampling: ExportFrameSummary) -> io::Result<Self> {
        let grid = soundtrack_grid(sampling)?;
        let frames = grid.frames();
        // Check origin/index range before files are made. Each bounded block
        // also checks adjacent sample-time precision before its PCM is written.
        session
            .mix_audio_block(
                sampling.start_time,
                frames,
                AudioMixFormat::STEREO_48_KHZ,
                &mut [],
            )
            .map_err(invalid)?;
        Ok(Self {
            session,
            scene_start: sampling.start_time,
            frames,
            mixed_frames: grid.mixed_frames(),
        })
    }

    /// PCM is staged once before starting FFmpeg, avoiding a second live pipe
    /// and its deadlock/backpressure protocol. Memory stays bounded by a block;
    /// private disk usage is eight bytes per 48 kHz stereo frame.
    pub fn stage(&self, directory: &Path, cancellation: &CaptureCancellation) -> io::Result<()> {
        stage_pcm(directory, self.frames, cancellation, |first, output| {
            self.mix(first, output)
        })
    }

    #[cfg(test)]
    fn write_pcm(
        &self,
        file: &mut impl Write,
        cancellation: &CaptureCancellation,
    ) -> io::Result<()> {
        write_pcm(file, self.frames, cancellation, |first, output| {
            self.mix(first, output)
        })
    }

    fn mix(&self, first: u64, output: &mut [f32]) -> io::Result<()> {
        let mixed = self
            .mixed_frames
            .saturating_sub(first)
            .min((output.len() / CHANNELS) as u64) as usize
            * CHANNELS;
        output[mixed..].fill(0.0);
        if mixed == 0 {
            return Ok(());
        }
        self.session
            .mix_audio_block(
                self.scene_start,
                first,
                AudioMixFormat::STEREO_48_KHZ,
                &mut output[..mixed],
            )
            .map_err(invalid)
    }
}

pub(super) fn stage_pcm(
    directory: &Path,
    frames: u64,
    cancellation: &CaptureCancellation,
    mix: impl FnMut(u64, &mut [f32]) -> io::Result<()>,
) -> io::Result<()> {
    check_publication_cancellation(cancellation)?;
    let mut file = BufWriter::new(File::create(directory.join(PCM_FILENAME))?);
    write_pcm(&mut file, frames, cancellation, mix)
}

fn write_pcm(
    file: &mut impl Write,
    frames: u64,
    cancellation: &CaptureCancellation,
    mut mix: impl FnMut(u64, &mut [f32]) -> io::Result<()>,
) -> io::Result<()> {
    let mut samples = vec![0.0_f32; MAX_AUDIO_MIX_BLOCK_FRAMES * CHANNELS];
    let mut bytes = vec![0_u8; samples.len() * 4];
    let mut first = 0;
    while first < frames {
        check_publication_cancellation(cancellation)?;
        let count = (frames - first).min(MAX_AUDIO_MIX_BLOCK_FRAMES as u64) as usize;
        let output = &mut samples[..count * CHANNELS];
        mix(first, output)?;
        let encoded = &mut bytes[..output.len() * 4];
        for (sample, bytes) in output.iter().zip(encoded.as_chunks_mut::<4>().0) {
            bytes.copy_from_slice(&sample.to_le_bytes());
        }
        file.write_all(encoded)?;
        first += count as u64;
    }
    file.flush()?;
    check_publication_cancellation(cancellation)
}

impl StagedSoundtrack for Soundtrack<'_> {
    fn frames(&self) -> u64 {
        self.frames
    }

    fn stage(&mut self, directory: &Path, cancellation: &CaptureCancellation) -> io::Result<()> {
        Soundtrack::stage(self, directory, cancellation)
    }
}

pub(super) fn stage_probe(directory: &Path) -> io::Result<()> {
    // One AAC packet is enough to probe the actual installed audio encoder.
    std::fs::write(directory.join(PCM_FILENAME), [0_u8; 1024 * CHANNELS * 4])
}

fn invalid(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, error)
}

#[cfg(test)]
mod tests;
