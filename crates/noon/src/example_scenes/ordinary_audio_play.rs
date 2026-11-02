//! Exact typed Rust pair of `tests/fixtures/fourier_audio/audio_ordinary_play.py`.
//! Native and direct Rust/WASM execute this same ordinary continuation and score.

use std::{cell::Cell, rc::Rc};

use super::background_audio::{music, SAMPLE_RATE};
use crate::{
    AnimationOptions, AudioClipOptions, AudioResource, ContinuationStep, LiveContinuation,
    LiveSession, LiveSessionError, Mobject, Scene, DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET,
};

/// Read-only evidence that preparation ran once and export owns no continuation.
#[derive(Clone, Default)]
pub struct SourceProgress {
    calls: Rc<Cell<u32>>,
    drops: Rc<Cell<u32>>,
}
impl SourceProgress {
    pub fn calls(&self) -> u32 {
        self.calls.get()
    }
    pub fn drops(&self) -> u32 {
        self.drops.get()
    }
}

pub struct Source {
    circle: Mobject,
    progress: SourceProgress,
}
impl Drop for Source {
    fn drop(&mut self) {
        self.progress.drops.set(self.progress.drops.get() + 1);
    }
}
impl LiveContinuation for Source {
    type Error = LiveSessionError;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        let step = self.progress.calls.get();
        self.progress.calls.set(step + 1);
        match step {
            0 => live
                .declare_and_activate_create(&self.circle, AnimationOptions::new().run_time(1.0))
                .map(ContinuationStep::Await),
            1 => live.wait_segment(2.0).map(ContinuationStep::Await),
            2 => Ok(ContinuationStep::Finished),
            _ => panic!("completed example source cannot run during export"),
        }
    }
}

/// Fixed 3-second Small Discovery soundtrack, Create(Circle(0.5)) for 1 second,
/// then an ordinary 2-second wait. This builder does not run the continuation.
pub fn scene() -> Result<(Scene, Source, SourceProgress), String> {
    let mut scene = Scene::new();
    let pcm = music::render_music(SAMPLE_RATE).map_err(|error| error.to_string())?;
    let resource = AudioResource::from_interleaved(
        pcm.sample_rate(),
        music::CHANNELS as u16,
        pcm.into_samples(),
        DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET,
    )
    .map_err(|error| error.to_string())?;
    scene
        .audio(
            resource,
            AudioClipOptions {
                duration: Some(3.0),
                gain: 0.5,
                fade_in: 0.5,
                fade_out: 0.5,
                ..AudioClipOptions::default()
            },
        )
        .map_err(|error| error.to_string())?;
    let circle = scene.circle(0.5).map_err(|error| error.to_string())?;
    let progress = SourceProgress::default();
    Ok((
        scene,
        Source {
            circle,
            progress: progress.clone(),
        },
        progress,
    ))
}
