//! Immutable audio values and handles at the optional language boundary.
//!
//! PCM validation and clip semantics belong to the shared Rust audio types.
//! These wrappers retain no playback clock, mixer, or mutable clip state.

use crate::authoring_error::AuthoringFailure;
use noon::{
    AudioClipDefinition, AudioResource, PreparedNarration, DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET,
};
use noon_core::{SemanticNodeId, SemanticStore};
use std::{cell::RefCell, rc::Rc};

pub(crate) fn pcm_resource(
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u16,
) -> Result<AudioResource, AuthoringFailure> {
    AudioResource::from_interleaved(
        sample_rate,
        channels,
        samples,
        DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET,
    )
    .map_err(noon::AuthoringError::from)
    .map_err(Into::into)
}

pub(crate) fn small_discovery() -> Result<AudioResource, AuthoringFailure> {
    use noon::example_scenes::background_audio::{music, SAMPLE_RATE};
    let pcm = music::render_music(SAMPLE_RATE)
        .map_err(|error| AuthoringFailure::unclassified("audio.music_preparation", &error))?;
    AudioResource::from_interleaved(
        pcm.sample_rate(),
        music::CHANNELS as u16,
        pcm.into_samples(),
        DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET,
    )
    .map_err(noon::AuthoringError::from)
    .map_err(Into::into)
}

pub(crate) fn prepared_narration(
    manifest: &str,
    pcm: &[u8],
) -> Result<PreparedNarration, AuthoringFailure> {
    PreparedNarration::from_prepared(manifest, pcm)
        .map_err(noon::AuthoringError::from)
        .map_err(Into::into)
}

/// An alias of one scene-global generational clip identity, never a clip copy.
#[derive(Clone)]
pub(crate) struct AuthoringAudioClipHandle {
    pub(crate) store: Rc<RefCell<SemanticStore>>,
    pub(crate) node: SemanticNodeId,
}

impl AuthoringAudioClipHandle {
    pub(crate) fn definition(&self) -> Result<AudioClipDefinition, AuthoringFailure> {
        self.store
            .borrow()
            .semantic_audio_clip_state(self.node)
            .map(|clip| *clip.definition())
            .map_err(Into::into)
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use crate::authoring_error::js_error;
    use noon::AudioClipOptions;
    use wasm_bindgen::prelude::*;

    /// Prepared immutable PCM. Reusing this value shares the existing Rust PCM.
    #[wasm_bindgen]
    pub struct WasmAudioSource {
        pub(crate) resource: AudioResource,
    }

    #[wasm_bindgen]
    impl WasmAudioSource {
        #[wasm_bindgen(js_name = fromInterleaved)]
        pub fn from_interleaved(
            samples: Vec<f32>,
            sample_rate: u32,
            channels: u16,
        ) -> Result<WasmAudioSource, JsValue> {
            pcm_resource(samples, sample_rate, channels)
                .map(|resource| Self { resource })
                .map_err(js_error)
        }

        /// Prepare the original CC0 composition once, outside playback.
        #[wasm_bindgen(js_name = smallDiscovery)]
        pub fn small_discovery() -> Result<WasmAudioSource, JsValue> {
            super::small_discovery()
                .map(|resource| Self { resource })
                .map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = sampleRate)]
        pub fn sample_rate(&self) -> u32 {
            self.resource.sample_rate()
        }

        #[wasm_bindgen(getter)]
        pub fn channels(&self) -> u16 {
            self.resource.channels()
        }

        #[wasm_bindgen(getter)]
        pub fn frames(&self) -> usize {
            self.resource.frames()
        }
    }

    /// Verified external media, imported once and shared by narration clips.
    #[wasm_bindgen]
    pub struct WasmPreparedNarration {
        pub(crate) narration: PreparedNarration,
    }

    #[wasm_bindgen]
    impl WasmPreparedNarration {
        #[wasm_bindgen(js_name = fromPrepared)]
        pub fn from_prepared(manifest: &str, pcm: Vec<u8>) -> Result<Self, JsValue> {
            prepared_narration(manifest, &pcm)
                .map(|narration| Self { narration })
                .map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = durationSeconds)]
        pub fn duration_seconds(&self) -> f64 {
            self.narration.duration_seconds()
        }

        #[wasm_bindgen(getter, js_name = spokenText)]
        pub fn spoken_text(&self) -> String {
            self.narration.spoken_text().to_owned()
        }

        #[wasm_bindgen(getter, js_name = captionText)]
        pub fn caption_text(&self) -> String {
            self.narration.caption_text().to_owned()
        }

        #[wasm_bindgen(getter, js_name = transcriptStatus)]
        pub fn transcript_status(&self) -> String {
            self.narration.transcript_status().as_str().to_owned()
        }

        #[wasm_bindgen(js_name = cueStart)]
        pub fn cue_start(&self, name: &str) -> Result<f64, JsValue> {
            self.narration
                .cue(name)
                .map(|cue| cue.start_seconds())
                .map_err(noon::AuthoringError::from)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = cueEnd)]
        pub fn cue_end(&self, name: &str) -> Result<f64, JsValue> {
            self.narration
                .cue(name)
                .map(|cue| cue.end_seconds())
                .map_err(noon::AuthoringError::from)
                .map_err(js_error)
        }
    }

    /// Recorded clip placement; waits validate its shared semantic identity.
    #[wasm_bindgen]
    pub struct WasmNarrationHandle {
        pub(crate) handle: noon::NarrationHandle,
    }

    #[wasm_bindgen]
    impl WasmNarrationHandle {
        #[wasm_bindgen(getter, js_name = startTime)]
        pub fn start_time(&self) -> f64 {
            self.handle.start_time()
        }

        #[wasm_bindgen(getter, js_name = endTime)]
        pub fn end_time(&self) -> f64 {
            self.handle.end_time()
        }

        #[wasm_bindgen(getter, js_name = durationSeconds)]
        pub fn duration_seconds(&self) -> f64 {
            self.handle.duration_seconds()
        }
    }

    /// Inert call arguments. Scene::audio validates them against the PCM before
    /// its ordinary atomic semantic transaction admits a clip or resource.
    #[wasm_bindgen]
    pub struct WasmAudioClipOptions {
        pub(crate) options: AudioClipOptions,
    }

    #[wasm_bindgen]
    impl WasmAudioClipOptions {
        #[wasm_bindgen(constructor)]
        #[allow(clippy::too_many_arguments)]
        pub fn new(
            start_time: f64,
            source_start_frame: usize,
            source_end_frame: Option<usize>,
            duration: Option<f64>,
            looped: bool,
            gain: f64,
            fade_in: f64,
            fade_out: f64,
        ) -> WasmAudioClipOptions {
            Self {
                options: AudioClipOptions {
                    start_time,
                    source_start_frame,
                    source_end_frame,
                    duration,
                    looped,
                    gain,
                    fade_in,
                    fade_out,
                },
            }
        }
    }

    #[wasm_bindgen]
    pub struct WasmAudioClipHandle {
        pub(crate) handle: AuthoringAudioClipHandle,
    }

    #[wasm_bindgen]
    impl WasmAudioClipHandle {
        #[wasm_bindgen(getter, js_name = startTime)]
        pub fn start_time(&self) -> Result<f64, JsValue> {
            self.handle
                .definition()
                .map(|clip| clip.start_time())
                .map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = endTime)]
        pub fn end_time(&self) -> Result<f64, JsValue> {
            self.handle
                .definition()
                .map(|clip| clip.end_time())
                .map_err(js_error)
        }

        #[wasm_bindgen(getter)]
        pub fn duration(&self) -> Result<f64, JsValue> {
            self.handle
                .definition()
                .map(|clip| clip.duration())
                .map_err(js_error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepared_import_preserves_the_shared_invalid_media_diagnostic() {
        let direct = PreparedNarration::from_prepared("{}", &[]).unwrap_err();
        let expected = AuthoringFailure::from(noon::AuthoringError::from(direct));
        let wrapped = prepared_narration("{}", &[]).unwrap_err();
        assert_eq!(wrapped.category, expected.category);
        assert_eq!(wrapped.code, expected.code);
        assert_eq!(wrapped.message, expected.message);
    }

    #[test]
    fn source_admission_uses_shared_format_and_finite_pcm_validation() {
        for (samples, rate, channels) in [
            (vec![0.0], 7_999, 1),
            (vec![0.0], 8_000, 3),
            (vec![0.0], 8_000, 2),
            (Vec::new(), 8_000, 1),
            (vec![f32::NAN], 8_000, 1),
            (vec![f32::INFINITY], 8_000, 1),
        ] {
            let error = pcm_resource(samples, rate, channels).unwrap_err();
            assert_eq!(error.code, "authoring.audio_resource");
            assert_eq!(error.category, "invalid_input");
        }
        let source = pcm_resource(vec![2.0, -2.0, 0.5, -0.5], 8_000, 2).unwrap();
        assert_eq!(source.frames(), 2);
        assert_eq!(source.samples(), &[2.0, -2.0, 0.5, -0.5]);
    }

    #[test]
    fn bundled_source_retains_original_music_format_and_is_shared_on_clone() {
        let source = small_discovery().unwrap();
        assert_eq!(source.sample_rate(), 24_000);
        assert_eq!(source.channels(), 2);
        assert_eq!(source.frames(), 80 * 24_000);
        assert!(source.samples().iter().any(|sample| sample.abs() > 0.01));
        assert_eq!(source.samples().as_ptr(), source.clone().samples().as_ptr());
    }
}
