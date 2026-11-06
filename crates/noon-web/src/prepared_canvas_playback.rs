//! Canvas lifecycle for one prepared reader, not an animation scheduler.
//! Runtime deadlines, evaluation, input and resources stay with PrefixReader.
//! This owner records transport intent, prefix backpressure and terminal loop policy.
use noon::{PrefixBatch, PrefixBatchStats, PrefixError, PrefixReader};
use noon_runtime::{RuntimeWakeState, TimelineWakeState};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreparedPlaybackObservation {
    pub(crate) time: f64,
    pub(crate) frontier: f64,
    pub(crate) end: Option<f64>,
    pub(crate) seek_range: Option<PreparedSeekRange>,
    pub(crate) playing: bool,
    pub(crate) buffering: bool,
    pub(crate) preparing: bool,
    pub(crate) completed: bool,
    pub(crate) advancing: bool,
    pub(crate) loop_duration: Option<f64>,
    pub(crate) loop_count: u64,
    pub(crate) loop_error: Option<&'static str>,
    pub(crate) interactions: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreparedSeekRange {
    pub(crate) start: f64,
    pub(crate) end: f64,
}

pub(crate) struct PreparedCanvasPlayback {
    pub(crate) reader: PrefixReader,
    playing: bool,
    buffering: bool,
    preparing: bool,
    loop_duration: Option<f64>,
    loop_error: Option<&'static str>,
    loop_count: u64,
    endpoint_seek_pending: bool,
    last_clock_sample: f64,
    reanchor_clock: bool,
}

impl PreparedCanvasPlayback {
    pub(crate) fn new(reader: PrefixReader) -> Self {
        let ended_at_start = reader
            .end_time()
            .is_some_and(|end| reader.frame().time >= end);
        let start_time = reader.frame().time;
        Self {
            reader,
            playing: !ended_at_start,
            buffering: false,
            preparing: false,
            loop_duration: None,
            loop_error: None,
            loop_count: 0,
            endpoint_seek_pending: false,
            last_clock_sample: start_time,
            reanchor_clock: false,
        }
    }
    pub(crate) fn playing(&self) -> bool {
        self.playing
    }
    pub(crate) fn buffering(&self) -> bool {
        self.buffering
    }
    pub(crate) fn preparing(&self) -> bool {
        self.preparing
    }
    pub(crate) fn completed(&self) -> bool {
        match self.reader.end_time() {
            Some(0.0) => true,
            Some(end) => self.loop_duration.is_none() && self.reader.frame().time >= end,
            None => false,
        }
    }
    pub(crate) fn advancing(&self) -> bool {
        self.playing
            && !self.preparing
            && !self.buffering
            && !(self.reader.end_time().is_some_and(|end| end == 0.0))
            && (self.loop_duration.is_some()
                || self
                    .reader
                    .end_time()
                    .is_none_or(|end| self.reader.frame().time < end))
    }
    pub(crate) fn loop_duration(&self) -> Option<f64> {
        if self.reader.end_time() == Some(0.0) {
            None
        } else {
            self.loop_duration
        }
    }
    pub(crate) fn observation(&self) -> PreparedPlaybackObservation {
        let seek_range = self
            .reader
            .selectable_time_bounds()
            .expect("prepared reader keeps its runtime and retained history scope")
            .map(|(start, end)| PreparedSeekRange { start, end });
        PreparedPlaybackObservation {
            time: self.reader.frame().time,
            frontier: self.reader.frontier(),
            end: self.reader.end_time(),
            seek_range,
            playing: self.playing,
            buffering: self.buffering,
            preparing: self.preparing,
            completed: self.completed(),
            advancing: self.advancing(),
            loop_duration: self.loop_duration(),
            loop_count: self.loop_count,
            loop_error: self.loop_error,
            interactions: self.reader.interactions_active(),
        }
    }
    fn loop_position(requested: f64, duration: f64) -> f64 {
        crate::clock::loop_phase(requested, duration)
    }
    fn completed_cycles(time: f64, duration: f64) -> u64 {
        crate::clock::completed_loop_cycles(time, duration)
    }
    pub(crate) fn time_from_clock(&self, requested: f64) -> f64 {
        let Some(end) = self.reader.end_time() else {
            return requested.min(self.reader.frontier());
        };
        let Some(duration) = self.loop_duration.filter(|_| end > 0.0) else {
            return requested.min(end);
        };
        if self.endpoint_seek_pending && requested == self.reader.frame().time {
            return requested;
        }
        let phase = Self::loop_position(requested, duration);
        phase.min(end)
    }
    /// Configure the existing gallery loop policy. It becomes active only after
    /// this reader adopts a closed terminal range; an open prefix never wraps.
    pub(crate) fn set_loop_duration(&mut self, duration: Option<f64>) -> Result<(), &'static str> {
        if let Some(duration) = duration {
            if !duration.is_finite() || duration <= 0.0 {
                return Err("playback loop duration must be finite and positive");
            }
            if self.reader.end_time().is_some_and(|end| duration < end) {
                return Err("playback loop duration cannot be shorter than the completed range");
            }
        }
        self.loop_duration = duration;
        self.loop_error = None;
        self.last_clock_sample = self.reader.frame().time;
        Ok(())
    }
    /// A completed wrap changes the local scene-time origin. Browser hosts
    /// consume this once to re-anchor their existing wall-time conversion.
    pub(crate) fn take_clock_reanchor(&mut self) -> bool {
        std::mem::take(&mut self.reanchor_clock)
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
        Ok(self.reader.end_time().is_none()
            && (self.preparing || self.buffering || self.reader.frontier() < target))
    }
    pub(crate) fn begin_preparation(&mut self) {
        self.preparing = true;
        self.last_clock_sample = self.reader.frame().time;
    }
    pub(crate) fn release_preparation(&mut self) -> bool {
        let was_preparing = std::mem::take(&mut self.preparing);
        if was_preparing {
            self.last_clock_sample = self.reader.frame().time;
        }
        was_preparing
    }
    pub(crate) fn stop_preparation(&mut self) -> bool {
        let was_preparing = self.release_preparation();
        if was_preparing {
            self.set_playing(false);
        }
        was_preparing
    }
    pub(crate) fn set_playing(&mut self, playing: bool) -> bool {
        let playing = playing
            && !(self.reader.end_time().is_some_and(|end| end == 0.0))
            && !(self.reader.end_time().is_some_and(|end| {
                self.reader.frame().time >= end && self.loop_duration.is_none()
            }));
        let changed = self.playing != playing;
        if changed && playing {
            self.last_clock_sample = self.reader.frame().time;
        }
        self.playing = playing;
        changed
    }
    pub(crate) fn wake_state(&self) -> RuntimeWakeState {
        let wake = self.reader.wake_state();
        if self.preparing {
            return wake.without_timeline_wake();
        }
        if self.playing && !self.buffering {
            if self.reader.end_time().is_some_and(|end| end > 0.0) {
                if let Some(duration) = self.loop_duration {
                    return wake.with_additional_timeline(TimelineWakeState::Deadline(duration));
                }
            }
            wake
        } else {
            wake.without_timeline_wake()
        }
    }
    pub(crate) fn seek(&mut self, time: f64) -> Result<(), PrefixError> {
        if self.preparing {
            if !time.is_finite() || time < 0.0 {
                self.reader.seek(time)?;
            }
            return Err(PrefixError::NotReady);
        }
        self.reader.seek(time)?;
        self.buffering = false;
        self.last_clock_sample = time;
        self.endpoint_seek_pending = self.loop_duration.is_some()
            && self
                .reader
                .end_time()
                .is_some_and(|end| end > 0.0 && time == end);
        if self.reader.end_time().is_some_and(|end| end == 0.0)
            || (self.loop_duration.is_none()
                && self.reader.end_time().is_some_and(|end| time == end))
        {
            self.playing = false;
        }
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
        if self.preparing {
            return Ok(());
        }
        if let Some(end) = self.reader.end_time() {
            if let Some(duration) = self.loop_duration {
                let endpoint_hold = self.endpoint_seek_pending
                    && requested == self.reader.frame().time
                    && self.reader.end_time() == Some(requested);
                let target = if endpoint_hold {
                    requested
                } else {
                    Self::loop_position(requested, duration)
                };
                let mut wraps = if endpoint_hold {
                    0
                } else {
                    Self::completed_cycles(requested, duration)
                        .saturating_sub(Self::completed_cycles(self.last_clock_sample, duration))
                };
                if endpoint_hold {
                    self.endpoint_seek_pending = true;
                } else if requested > self.reader.frame().time {
                    if self.endpoint_seek_pending {
                        wraps = wraps.saturating_add(1);
                    }
                    self.endpoint_seek_pending = false;
                }
                let target = target.min(end);
                if target < self.reader.frame().time {
                    self.reader.seek(target)?;
                } else {
                    self.reader.advance_to(target)?;
                }
                self.loop_count = self.loop_count.saturating_add(wraps);
                self.reanchor_clock |= wraps > 0;
                self.last_clock_sample = if wraps > 0 { target } else { requested };
            } else {
                self.reader.advance_to(requested.min(end))?;
                self.last_clock_sample = requested.min(end);
                if requested >= end {
                    self.playing = false;
                }
            }
        } else if requested >= self.reader.frontier() {
            // No epsilon frame, speculative seek or synthetic cancellation.
            // A missing suffix preserves the last actually evaluated sample.
            self.buffering = true;
        } else {
            self.reader.advance_to(requested)?;
            self.last_clock_sample = requested;
        }
        Ok(())
    }
    pub(crate) fn append(&mut self, batch: PrefixBatch) -> Result<PrefixBatchStats, PrefixError> {
        let frontier = self.reader.frontier();
        let was_buffering = self.buffering;
        let was_preparing = self.preparing;
        let terminal = batch.is_terminal();
        let invalid_loop_range = batch.is_terminal()
            && self
                .loop_duration
                .is_some_and(|duration| duration < batch.frontier());
        let stats = self.reader.append(batch)?;
        if invalid_loop_range {
            self.loop_duration = None;
            self.loop_error = Some("configured loop duration is shorter than the completed range");
        }
        if self.reader.frontier() > frontier || self.reader.end_time().is_some() {
            self.buffering = false;
        }
        if was_preparing && terminal {
            self.release_preparation();
        }
        if was_buffering {
            self.last_clock_sample = self.reader.frame().time;
        }
        Ok(stats)
    }
}

#[cfg(test)]
mod tests;
