use super::*;

impl SemanticExecutionPlayer {
    pub(super) fn video_tick_time(&mut self, requested: f64) -> Option<f64> {
        let session = self.source.session();
        let media = session.video_rasters();
        if media.is_empty() || !self.clock.is_playing() {
            return Some(requested);
        }
        if self.video.awaiting_picture {
            return None;
        }
        let target = self.video.pending.unwrap_or(requested);
        self.video
            .allow_sample(media, session.frame().time, target, target)
            .then_some(target)
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
impl SemanticExecutionPlayer {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = setVideoWindow))]
    pub fn set_video_window(
        &mut self,
        clip: &str,
        start: usize,
        end: usize,
    ) -> Result<bool, String> {
        self.video
            .update(self.source.session().video_rasters(), clip, start, end)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = videoBuffering))]
    pub fn video_buffering(&self) -> bool {
        self.video.pending.is_some()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = videoReadyUntil))]
    pub fn video_ready_until(&self) -> Option<f64> {
        self.video
            .boundary(self.source.session().video_rasters(), self.time())
            .or_else(|| self.source.session().finite_playback_end_time())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = videoWaiting))]
    pub fn video_waiting(&self) -> bool {
        self.video.waiting()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = takeVideoDemands))]
    pub fn take_video_demands(&mut self) -> Option<String> {
        self.video.take_demands()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = acknowledgeVideoPresentation))]
    pub fn acknowledge_video_presentation(&mut self, wall_time_ms: f64) -> Result<(), String> {
        if !wall_time_ms.is_finite() || wall_time_ms < 0.0 {
            return Err("video presentation requires finite non-negative wall time".into());
        }
        if std::mem::take(&mut self.video.awaiting_picture) {
            self.clock
                .seek(self.time())
                .map_err(|error| error.to_string())?;
            self.clock
                .scene_time(wall_time_ms)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}
