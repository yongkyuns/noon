//! Finite audio and section observations on the existing worker player.

use super::*;

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

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = finiteAudioDuration))]
    pub fn finite_audio_duration(&self) -> Option<f64> {
        self.finite_audio_end
    }

    #[cfg(any(target_arch = "wasm32", test))]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = audioResourceBundleBytes))]
    pub fn audio_resource_bundle_bytes(&self) -> Result<Vec<u8>, String> {
        Self::require_worker_execution_profile(&self.session)?;
        crate::worker_audio::encode(&self.session, self.encoder.session())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = audioAnchorJson))]
    pub fn audio_anchor_json(&self, sampled_epoch_ms: f64) -> Result<String, String> {
        Self::require_worker_execution_profile(&self.session)?;
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
