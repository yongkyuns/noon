//! Canvas lifecycle for one prepared reader, not an animation scheduler.
//! Runtime deadlines, evaluation, input and resources stay with PrefixReader.
//! These two flags record only user pause and missing-prefix backpressure.
use noon::{PrefixBatch, PrefixBatchStats, PrefixError, PrefixReader};
use noon_runtime::RuntimeWakeState;

pub(crate) struct PreparedCanvasPlayback {
    pub(crate) reader: PrefixReader,
    playing: bool,
    buffering: bool,
}

impl PreparedCanvasPlayback {
    pub(crate) fn new(reader: PrefixReader) -> Result<Self, &'static str> {
        // Source/reader admission is not an audible-output contract. Reject at
        // the common host boundary before a device or transport is constructed,
        // rather than silently displaying an audio scene without its soundtrack.
        if reader.has_audio() {
            return Err("prepared audio output requires displayed-time synchronization (#2358)");
        }
        Ok(Self::for_worker(reader))
    }
    /// The worker host supplies a bounded, picture-qualified Window audio path.
    /// Direct canvases still use `new` until they implement their own output.
    pub(crate) fn for_worker(reader: PrefixReader) -> Self {
        let playing = !reader.has_audio();
        Self {
            reader,
            playing,
            buffering: false,
        }
    }
    pub(crate) fn playing(&self) -> bool {
        self.playing
    }
    pub(crate) fn buffering(&self) -> bool {
        self.buffering
    }
    /// Pure demand derived from the reader, never a JavaScript timeline clock.
    pub(crate) fn preparation_target(&self, ahead: f64) -> Result<f64, &'static str> {
        let target = self.reader.frame().time + ahead;
        if !ahead.is_finite() || ahead <= 0.0 || !target.is_finite() {
            return Err("preparation look-ahead must be finite, positive and representable");
        }
        Ok(target)
    }
    /// Interpreter continuations use the same reader-local demand as the typed
    /// Rust producer. This is permission to resume source, not a guessed endpoint.
    pub(crate) fn needs_preparation(&self, ahead: f64) -> Result<bool, &'static str> {
        let target = self.preparation_target(ahead)?;
        Ok(self.reader.end_time().is_none() && (self.buffering || self.reader.frontier() < target))
    }
    pub(crate) fn set_playing(&mut self, playing: bool) -> bool {
        let changed = self.playing != playing;
        self.playing = playing;
        changed
    }
    pub(crate) fn wake_state(&self) -> RuntimeWakeState {
        let wake = self.reader.wake_state();
        if self.playing && !self.buffering {
            wake
        } else {
            wake.without_timeline_wake()
        }
    }
    pub(crate) fn seek(&mut self, time: f64) -> Result<(), PrefixError> {
        self.reader.seek(time)?;
        self.buffering = false;
        Ok(())
    }
    /// A browser timer may deliver a later wall sample than the timestamp
    /// attached to the next RAF callback. Both are host clock observations, not
    /// explicit history seeks: ignore a stale sample without replaying a frame.
    /// The strict advance() contract below still rejects backwards requests,
    /// and malformed times are never converted into valid samples.
    pub(crate) fn advance_clock_sample(&mut self, requested: f64) -> Result<(), PrefixError> {
        if !requested.is_finite() {
            return self.advance(requested);
        }
        if requested < self.reader.frame().time {
            return Ok(());
        }
        self.advance(requested)
    }
    pub(crate) fn advance(&mut self, requested: f64) -> Result<(), PrefixError> {
        // Preserve the reader's typed error and rejection atomicity; min(end)
        // must not turn NaN/infinity or a backwards timestamp into valid time.
        if !requested.is_finite() || requested < self.reader.frame().time {
            self.reader.advance_to(requested)?;
            return Ok(());
        }
        if !self.playing || self.buffering {
            return Ok(());
        }
        if let Some(end) = self.reader.end_time() {
            self.reader.advance_to(requested.min(end))?;
        } else if requested >= self.reader.frontier() {
            // No epsilon frame, speculative seek or synthetic cancellation.
            // A missing suffix preserves the last actually evaluated sample.
            self.buffering = true;
        } else {
            self.reader.advance_to(requested)?;
        }
        Ok(())
    }
    pub(crate) fn append(&mut self, batch: PrefixBatch) -> Result<PrefixBatchStats, PrefixError> {
        let frontier = self.reader.frontier();
        let stats = self.reader.append(batch)?;
        if self.reader.frontier() > frontier || self.reader.end_time().is_some() {
            self.buffering = false;
        }
        Ok(stats)
    }
}

#[cfg(test)]
mod tests;
