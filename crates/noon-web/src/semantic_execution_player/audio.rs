//! Finite audio and section observations on the existing worker player.

use super::*;

#[derive(Clone, Copy)]
pub(super) struct PendingSceneControl {
    pub(super) sequence: u64,
    pub(super) publication: PublicationContext,
    pub(super) action: noon::PlaybackControl,
}

#[cfg(test)]
mod tests;

impl PendingSceneControl {
    fn action_name(self) -> &'static str {
        match self.action {
            noon::PlaybackControl::TogglePlayback => "toggle_playback",
            noon::PlaybackControl::Restart => "restart",
            noon::PlaybackControl::PreviousSection => "previous_section",
            noon::PlaybackControl::NextSection => "next_section",
            noon::PlaybackControl::ToggleAudioMuted => "toggle_audio_muted",
        }
    }
}

impl SemanticExecutionPlayer {
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn set_section_navigation(&mut self, navigation: noon::SceneSectionNavigation) {
        self.section_navigation = navigation;
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
impl SemanticExecutionPlayer {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = hasAudio))]
    pub fn has_audio_wasm(&self) -> bool {
        self.session.has_audio()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = finitePlaybackDuration))]
    pub fn finite_playback_duration(&self) -> Option<f64> {
        self.finite_transport_end
    }

    /// Platform completion metadata for one successfully committed click.
    /// The decimal sequence preserves the full Rust input identity across the
    /// genuine worker boundary. It cannot manufacture a target or action.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = pendingSceneControlJson))]
    pub fn pending_scene_control_json(&self) -> Option<String> {
        self.pending_scene_control.map(|request| {
            serde_json::json!({
                "sequence": request.sequence.to_string(),
                "action": request.action_name(),
            })
            .to_string()
        })
    }

    /// Consume the same receipt only after the input transaction has returned.
    /// A stale device-unlock completion cannot take a newer request, revive a
    /// changed publication, or extend immutable playback into live execution.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = takeSceneControl))]
    pub fn take_scene_control(&mut self, sequence: u64) -> Result<Option<String>, String> {
        let Some(request) = self.pending_scene_control else {
            return Ok(None);
        };
        if request.sequence != sequence {
            return Ok(None);
        }
        self.pending_scene_control = None;
        if request.publication != self.session.publication_context() {
            return Ok(None);
        }
        if !self.session.supports_immutable_playback() || self.finite_export.is_some() {
            return Err(
                "scene playback controls require admitted finite immutable execution (#2263)"
                    .into(),
            );
        }
        Ok(Some(request.action_name().to_owned()))
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = cancelSceneControl))]
    pub fn cancel_scene_control(&mut self) {
        self.pending_scene_control = None;
    }

    #[cfg(any(target_arch = "wasm32", test))]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = audioResourceBundleBytes))]
    pub fn audio_resource_bundle_bytes(&self) -> Result<Vec<u8>, String> {
        crate::worker_audio::encode(&self.session, self.encoder.session())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = audioAnchorJson))]
    pub fn audio_anchor_json(&self, sampled_epoch_ms: f64) -> Result<String, String> {
        if !sampled_epoch_ms.is_finite() {
            return Err("audio anchor requires a finite host timestamp".into());
        }
        Ok(serde_json::json!({
            "session": self.encoder.session(), "time": self.session.frame().time,
            "playing": self.clock.is_playing(), "sampledEpochMs": sampled_epoch_ms,
        })
        .to_string())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = sectionsJson))]
    pub fn sections_json(&self) -> String {
        serde_json::Value::Array(self.section_navigation.sections().iter().enumerate().map(|(index, section)| {
            serde_json::json!({ "index": index, "name": section.name, "time": section.time })
        }).collect()).to_string()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = seekSectionDeltaJson))]
    pub fn seek_section_delta_json(&mut self, previous: bool) -> Result<Option<String>, String> {
        let direction = if previous {
            noon::SectionNavigation::Previous
        } else {
            noon::SectionNavigation::Next
        };
        let time = self
            .section_navigation
            .target(self.time(), direction)?
            .map(|(_, section)| section.time);
        match time {
            Some(time) => self.seek_delta_json(time),
            None => Ok(None),
        }
    }
}
