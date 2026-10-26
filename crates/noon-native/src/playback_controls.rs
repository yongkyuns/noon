//! Native viewer commands applied after the ordinary shared input transaction.
//!
//! The session owns seek admission and evaluation; SceneSectionNavigation owns
//! cue selection. This shell retains only viewer intent and the existing native
//! wall-clock anchor. It does not replay input or create an authored timeline.

use std::time::Instant;

use noon::SectionNavigation;
use noon_core::PlaybackControl;
use winit::event::ElementState;
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::{NativeApp, NativeHostError};

impl NativeApp {
    fn ensure_playback_controls(&self) -> Result<f64, NativeHostError> {
        self.execution
            .finite_transport_end()
            .ok_or(NativeHostError::PlaybackControlsUnavailable)
    }

    fn observed_playback_time(&mut self, now: Instant) -> f64 {
        self.realtime_clock_for_timeline(self.execution.timeline(), now)
            .map_or(self.execution.frame_time(), |clock| {
                self.finite_transport_end
                    .map_or(clock.scene_time_at(now), |end| {
                        clock.scene_time_at(now).min(end)
                    })
            })
            .max(self.execution.frame_time())
    }

    pub(super) fn pause_playback(&mut self, now: Instant) -> Result<(), NativeHostError> {
        self.ensure_playback_controls()?;
        if self.playback_paused {
            return Ok(());
        }
        let target = self.observed_playback_time(now);
        // A sleeping static interval still has elapsed authored time. Observe it
        // through the existing runtime before dropping the common wall anchor.
        self.execution.advance_to(target)?;
        self.playback_paused = true;
        self.realtime_clock = None;
        #[cfg(feature = "audio")]
        self.release_audio();
        self.update_playback_title();
        Ok(())
    }

    pub(super) fn resume_playback(&mut self) -> Result<(), NativeHostError> {
        let end = self.ensure_playback_controls()?;
        if self.execution.frame_time() >= end {
            self.seek_playback(0.0)?;
        } else if !self.playback_paused {
            return Ok(());
        }
        self.playback_paused = false;
        self.realtime_clock = None;
        self.playback_picture_pending = true;
        self.force_full_redraw = true;
        #[cfg(feature = "audio")]
        if self.gpu.as_ref().is_some_and(|gpu| gpu.drawable) {
            self.prepare_audio()?;
        }
        self.update_playback_title();
        Ok(())
    }

    pub(super) fn seek_playback(&mut self, time: f64) -> Result<(), NativeHostError> {
        self.ensure_playback_controls()?;
        // Validate/evaluate first. An invalid target must not silence a healthy
        // stream or destroy its anchor. Successful seek retires transient input
        // in the same session and preserves viewer play/pause intent.
        self.execution.seek_to(time)?;
        #[cfg(feature = "audio")]
        self.release_audio();
        self.realtime_clock = None;
        self.playback_picture_pending = true;
        self.force_full_redraw = true;
        self.rebind_pointer_view()?;
        #[cfg(feature = "audio")]
        if !self.playback_paused && self.gpu.as_ref().is_some_and(|gpu| gpu.drawable) {
            self.prepare_audio()?;
        }
        self.update_playback_title();
        Ok(())
    }

    fn seek_playback_section(
        &mut self,
        direction: SectionNavigation,
        now: Instant,
    ) -> Result<(), NativeHostError> {
        self.ensure_playback_controls()?;
        let observed = self.observed_playback_time(now);
        let target = self
            .config
            .section_navigation
            .target(observed, direction)
            .map_err(NativeHostError::Program)?
            .map(|(_, section)| section.time);
        if let Some(time) = target {
            self.seek_playback(time)?;
        }
        Ok(())
    }

    pub(super) fn dispatch_playback_control(
        &mut self,
        control: PlaybackControl,
        now: Instant,
    ) -> Result<(), NativeHostError> {
        let end = self.ensure_playback_controls()?;
        match control {
            PlaybackControl::TogglePlayback => {
                if self.playback_paused || self.execution.frame_time() >= end {
                    self.resume_playback()
                } else {
                    self.pause_playback(now)
                }
            }
            PlaybackControl::PreviousSection => {
                self.seek_playback_section(SectionNavigation::Previous, now)
            }
            PlaybackControl::NextSection => {
                self.seek_playback_section(SectionNavigation::Next, now)
            }
            PlaybackControl::Restart => self.seek_playback(0.0),
            PlaybackControl::ToggleAudioMuted => self.set_viewer_muted(!self.viewer_muted),
        }
    }

    pub(super) fn set_viewer_volume(&mut self, volume: f64) -> Result<(), NativeHostError> {
        if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
            return Err(NativeHostError::Audio(
                "viewer audio volume must be finite and between zero and one".into(),
            ));
        }
        self.viewer_volume = volume;
        self.publish_viewer_gain()?;
        self.update_playback_title();
        Ok(())
    }

    pub(super) fn set_viewer_muted(&mut self, muted: bool) -> Result<(), NativeHostError> {
        self.viewer_muted = muted;
        self.publish_viewer_gain()?;
        self.update_playback_title();
        Ok(())
    }

    #[cfg(feature = "audio")]
    pub(super) fn viewer_gain(&self) -> f64 {
        if self.viewer_muted {
            0.0
        } else {
            self.viewer_volume
        }
    }

    fn publish_viewer_gain(&self) -> Result<(), NativeHostError> {
        #[cfg(feature = "audio")]
        for output in [&self.audio_output, &self.releasing_audio_output]
            .into_iter()
            .flatten()
        {
            output.set_gain(self.viewer_gain())?;
        }
        Ok(())
    }

    pub(super) fn update_playback_title(&self) {
        if self.config.playback_controls || self.execution.session().has_playback_controls() {
            if let Some(window) = &self.window {
                let state = if self.playback_paused {
                    "Paused"
                } else if self
                    .finite_transport_end
                    .is_some_and(|end| self.execution.frame_time() >= end)
                {
                    "Finished"
                } else {
                    "Playing"
                };
                let audio = if self.viewer_muted {
                    "Muted".to_owned()
                } else {
                    format!("Volume {:.0}%", self.viewer_volume * 100.0)
                };
                window.set_title(&format!("{} — {state} · {audio}", self.config.title));
            }
        }
    }

    pub(super) fn dispatch_playback_key(
        &mut self,
        key: PhysicalKey,
        state: ElementState,
        repeat: bool,
    ) -> Result<bool, NativeHostError> {
        if !self.config.playback_controls || !self.pointer.modifiers.is_empty() {
            return Ok(false);
        }
        let PhysicalKey::Code(key) = key else {
            return Ok(false);
        };
        if !matches!(
            key,
            KeyCode::Space
                | KeyCode::ArrowLeft
                | KeyCode::ArrowRight
                | KeyCode::ArrowUp
                | KeyCode::ArrowDown
                | KeyCode::PageUp
                | KeyCode::PageDown
                | KeyCode::Home
                | KeyCode::KeyM
        ) {
            return Ok(false);
        }
        // A held key emits one viewer command. The corresponding release stays
        // in this collector lane and never invents an authored native key edge.
        if state != ElementState::Pressed || repeat {
            return Ok(true);
        }
        let now = Instant::now();
        match key {
            KeyCode::Space => {
                self.dispatch_playback_control(PlaybackControl::TogglePlayback, now)?
            }
            KeyCode::Home => self.dispatch_playback_control(PlaybackControl::Restart, now)?,
            KeyCode::PageUp => {
                self.dispatch_playback_control(PlaybackControl::PreviousSection, now)?;
            }
            KeyCode::PageDown => {
                self.dispatch_playback_control(PlaybackControl::NextSection, now)?;
            }
            KeyCode::KeyM => {
                self.dispatch_playback_control(PlaybackControl::ToggleAudioMuted, now)?;
            }
            KeyCode::ArrowLeft | KeyCode::ArrowRight => {
                let end = self.ensure_playback_controls()?;
                let step = if key == KeyCode::ArrowLeft { -5.0 } else { 5.0 };
                let target = (self.observed_playback_time(now) + step).clamp(0.0, end);
                self.seek_playback(target)?;
            }
            KeyCode::ArrowUp | KeyCode::ArrowDown => {
                let step = if key == KeyCode::ArrowUp { 0.05 } else { -0.05 };
                self.set_viewer_volume((self.viewer_volume + step).clamp(0.0, 1.0))?;
            }
            _ => unreachable!("viewer key was classified above"),
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "audio", target_os = "linux"))]
mod device_tests;
