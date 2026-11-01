//! Immutable audio resources across the existing Python-worker/Window boundary.
//!
//! This is a disposable device mirror, not a scene or player. Timing projection
//! uses the same Rust AudioPlaybackClip query as direct output. A command carries
//! one accepted worker sample and its wall timestamp; no clock lives here.

use std::collections::{BTreeMap, BTreeSet};

use noon::ExecutionSession;
use noon_core::{
    AudioClipDefinition, AudioClipOptions, AudioResource, AudioResourceHandle, AudioResourceId,
    SemanticNodeId, DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET,
};
use noon_runtime::AudioPlaybackClip;
use serde::{Deserialize, Serialize};

const VERSION: u32 = 2;
const MAX_METADATA_BYTES: usize = 1024 * 1024;
const MAX_CLIPS: usize = 4096;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourceWire {
    handle: [u64; 3],
    sample_rate: u32,
    channels: u16,
    samples: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClipWire {
    identity: [u32; 2],
    resource: usize,
    start_time: f64,
    source_start_frame: usize,
    source_end_frame: usize,
    duration: f64,
    looped: bool,
    gain: f64,
    fade_in: f64,
    fade_out: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BundleWire {
    version: u32,
    session: u32,
    requires_horizon: bool,
    resources: Vec<ResourceWire>,
    clips: Vec<ClipWire>,
}

/// Encode each retained PCM resource once at an explicit worker lease boundary.
pub(crate) fn encode(
    session: &ExecutionSession,
    transport_session: u32,
) -> Result<Vec<u8>, String> {
    encode_closure(
        session
            .audio_playback_at(0.0)
            .map_err(|error| error.to_string())?
            .map(|clip| (clip.clip, clip.definition, clip.pcm)),
        transport_session,
        false,
    )
}

#[cfg(feature = "prepared-prefix")]
pub(crate) fn encode_prepared(
    reader: &noon::PrefixReader,
    transport_session: u32,
) -> Result<Vec<u8>, String> {
    encode_closure(reader.audio_resource_closure(), transport_session, true)
}

fn encode_closure<'a>(
    closure: impl Iterator<Item = (SemanticNodeId, &'a AudioClipDefinition, &'a AudioResource)>,
    transport_session: u32,
    requires_horizon: bool,
) -> Result<Vec<u8>, String> {
    let mut indices = BTreeMap::new();
    let mut resources = Vec::new();
    let mut payloads = Vec::new();
    let mut clips = Vec::new();
    let mut pcm_bytes = 0_usize;
    for (identity, definition, pcm) in closure {
        if clips.len() == MAX_CLIPS {
            return Err("worker audio exceeds the finite clip limit".into());
        }
        let resource = definition.resource();
        let index = if let Some(&index) = indices.get(&resource) {
            index
        } else {
            pcm_bytes = pcm_bytes
                .checked_add(pcm.pcm_bytes())
                .ok_or("worker audio resource size overflow")?;
            if pcm_bytes > DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET {
                return Err("worker audio exceeds its PCM budget".into());
            }
            let index = resources.len();
            indices.insert(resource, index);
            resources.push(ResourceWire {
                handle: [resource.arena, resource.id.get(), resource.version],
                sample_rate: pcm.sample_rate(),
                channels: pcm.channels(),
                samples: pcm.samples().len(),
            });
            payloads.push(pcm.samples());
            index
        };
        clips.push(ClipWire {
            identity: [identity.slot(), identity.generation()],
            resource: index,
            start_time: definition.start_time(),
            source_start_frame: definition.source_start_frame(),
            source_end_frame: definition.source_end_frame(),
            duration: definition.duration(),
            looped: definition.is_looped(),
            gain: definition.gain(),
            fade_in: definition.fade_in(),
            fade_out: definition.fade_out(),
        });
    }
    let metadata = serde_json::to_vec(&BundleWire {
        version: VERSION,
        session: transport_session,
        requires_horizon,
        resources,
        clips,
    })
    .map_err(|error| error.to_string())?;
    if metadata.len() > MAX_METADATA_BYTES {
        return Err("worker audio metadata exceeds its limit".into());
    }
    let mut bytes = Vec::with_capacity(4 + metadata.len() + pcm_bytes);
    bytes.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&metadata);
    for payload in payloads {
        for sample in payload {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
    }
    Ok(bytes)
}

pub(crate) struct WorkerAudioResources {
    session: u32,
    requires_horizon: bool,
    resources: BTreeMap<AudioResourceHandle, AudioResource>,
    clips: Vec<(SemanticNodeId, AudioClipDefinition)>,
}

impl WorkerAudioResources {
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 4
            || bytes.len() > 4 + MAX_METADATA_BYTES + DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET
        {
            return Err("invalid worker audio bundle size".into());
        }
        let metadata_length = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        if metadata_length > MAX_METADATA_BYTES || metadata_length > bytes.len() - 4 {
            return Err("invalid worker audio metadata length".into());
        }
        let wire: BundleWire = serde_json::from_slice(&bytes[4..4 + metadata_length])
            .map_err(|error| format!("invalid worker audio metadata: {error}"))?;
        if wire.version != VERSION
            || wire.resources.len() > MAX_CLIPS
            || wire.clips.len() > MAX_CLIPS
        {
            return Err("unsupported worker audio bundle".into());
        }
        let mut resources = BTreeMap::new();
        let mut handles = Vec::new();
        let mut cursor = 4 + metadata_length;
        let mut total = 0_usize;
        for resource in wire.resources {
            let count = resource
                .samples
                .checked_mul(4)
                .ok_or("audio resource size overflow")?;
            total = total
                .checked_add(count)
                .ok_or("audio resource size overflow")?;
            if total > DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET || count > bytes.len() - cursor {
                return Err("worker PCM payload is truncated or exceeds its budget".into());
            }
            let handle = AudioResourceHandle {
                arena: resource.handle[0],
                id: AudioResourceId::new(resource.handle[1]),
                version: resource.handle[2],
            };
            if resources.contains_key(&handle) {
                return Err("duplicate worker audio resource".into());
            }
            let samples = bytes[cursor..cursor + count]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|word| f32::from_le_bytes(*word))
                .collect::<Vec<_>>();
            let pcm = AudioResource::from_interleaved(
                resource.sample_rate,
                resource.channels,
                samples,
                DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET,
            )
            .map_err(|error| error.to_string())?;
            resources.insert(handle, pcm);
            handles.push(handle);
            cursor += count;
        }
        if cursor != bytes.len() {
            return Err("worker audio bundle has trailing PCM bytes".into());
        }
        let mut identities = BTreeSet::new();
        let mut clips = Vec::with_capacity(wire.clips.len());
        for clip in wire.clips {
            let identity = SemanticNodeId::new(clip.identity[0], clip.identity[1]);
            if !identities.insert(identity) {
                return Err("duplicate worker audio clip".into());
            }
            let resource = *handles
                .get(clip.resource)
                .ok_or("missing worker audio resource")?;
            let definition = AudioClipDefinition::new(
                resource,
                &resources[&resource],
                AudioClipOptions {
                    start_time: clip.start_time,
                    source_start_frame: clip.source_start_frame,
                    source_end_frame: Some(clip.source_end_frame),
                    duration: Some(clip.duration),
                    looped: clip.looped,
                    gain: clip.gain,
                    fade_in: clip.fade_in,
                    fade_out: clip.fade_out,
                },
            )
            .map_err(|error| error.to_string())?;
            clips.push((identity, definition));
        }
        Ok(Self {
            session: wire.session,
            requires_horizon: wire.requires_horizon,
            resources,
            clips,
        })
    }

    pub(crate) fn session(&self) -> u32 {
        self.session
    }

    pub(crate) fn at_time(&self, time: f64) -> Result<Vec<AudioPlaybackClip<'_>>, String> {
        let mut clips = Vec::new();
        for (identity, definition) in &self.clips {
            if let Some(clip) = AudioPlaybackClip::at_time(
                *identity,
                definition,
                &self.resources[&definition.resource()],
                time,
            )
            .map_err(|error| error.to_string())?
            {
                clips.push(clip);
            }
        }
        Ok(clips)
    }

    /// The same runtime cutoff used by a prepared reader, after the Window has
    /// received an accepted reader anchor. PCM retained here grants no horizon.
    #[cfg(feature = "prepared-prefix")]
    pub(crate) fn at_time_before(
        &self,
        time: f64,
        available_until: f64,
    ) -> Result<Vec<AudioPlaybackClip<'_>>, String> {
        if !time.is_finite() || time < 0.0 || !available_until.is_finite() || available_until < 0.0
        {
            return Err("invalid prepared audio time or availability".into());
        }
        let mut clips = Vec::new();
        for (identity, definition) in &self.clips {
            if let Some(clip) = AudioPlaybackClip::at_time_before(
                *identity,
                definition,
                &self.resources[&definition.resource()],
                time,
                available_until,
            )
            .map_err(|error| error.to_string())?
            {
                clips.push(clip);
            }
        }
        Ok(clips)
    }

    #[cfg(feature = "prepared-prefix")]
    fn anchored_before(
        &self,
        session: u32,
        time: f64,
        sampled_epoch_ms: f64,
        received_epoch_ms: f64,
        available_until: f64,
    ) -> Result<Vec<AudioPlaybackClip<'_>>, String> {
        if !self.requires_horizon
            || session != self.session
            || !time.is_finite()
            || time < 0.0
            || !sampled_epoch_ms.is_finite()
            || sampled_epoch_ms < 0.0
            || !received_epoch_ms.is_finite()
            || received_epoch_ms < sampled_epoch_ms
            || !available_until.is_finite()
            || available_until < time
        {
            return Err("stale or invalid prepared audio anchor".into());
        }
        let time = time + (received_epoch_ms - sampled_epoch_ms) / 1000.0;
        self.at_time_before(time, available_until)
    }
}

#[cfg(all(target_arch = "wasm32", feature = "renderer"))]
mod wasm {
    use super::*;
    use crate::audio_output::{ViewerAudioControls, WebAudioOutput};
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    pub struct WasmWorkerAudioOutput {
        resources: WorkerAudioResources,
        output: WebAudioOutput,
        controls: ViewerAudioControls,
    }

    #[wasm_bindgen]
    impl WasmWorkerAudioOutput {
        #[wasm_bindgen(constructor)]
        pub fn new(bytes: &[u8]) -> Result<Self, JsValue> {
            let resources = WorkerAudioResources::decode(bytes).map_err(js_error)?;
            let controls = ViewerAudioControls::default();
            let mut output = WebAudioOutput::new(controls)?;
            output.preload_clips(resources.at_time(0.0).map_err(js_error)?)?;
            Ok(Self {
                resources,
                output,
                controls,
            })
        }

        pub fn unlock(&mut self) -> Result<js_sys::Promise, JsValue> {
            self.output.unlock()
        }
        pub fn ready(&self) -> bool {
            self.output.is_ready()
        }
        #[wasm_bindgen(js_name = requiresHorizon)]
        pub fn requires_horizon(&self) -> bool {
            self.resources.requires_horizon
        }
        #[wasm_bindgen(js_name = audioContext)]
        pub fn audio_context(&self) -> web_sys::AudioContext {
            self.output.context()
        }

        /// Age an accepted worker clock sample by transport latency, then use
        /// the shared stateless clip query. Never observe or drive visual time.
        pub fn start(
            &mut self,
            session: u32,
            time: f64,
            sampled_epoch_ms: f64,
            received_epoch_ms: f64,
        ) -> Result<(), JsValue> {
            if self.resources.requires_horizon
                || session != self.resources.session()
                || !time.is_finite()
                || time < 0.0
                || !sampled_epoch_ms.is_finite()
                || !received_epoch_ms.is_finite()
                || received_epoch_ms < sampled_epoch_ms
            {
                return Err(js_error("stale or invalid worker audio anchor"));
            }
            let time = time + (received_epoch_ms - sampled_epoch_ms) / 1000.0;
            self.output
                .start_clips(self.resources.at_time(time).map_err(js_error)?)
        }

        /// Start only within the reader availability carried by this anchor.
        /// Latency can consume the whole remaining interval; that must be silent
        /// rather than scheduling the unchanged immutable soundtrack afterward.
        #[cfg(feature = "prepared-prefix")]
        #[wasm_bindgen(js_name = startBounded)]
        pub fn start_bounded(
            &mut self,
            session: u32,
            time: f64,
            sampled_epoch_ms: f64,
            received_epoch_ms: f64,
            available_until: f64,
        ) -> Result<(), JsValue> {
            let clips = self
                .resources
                .anchored_before(
                    session,
                    time,
                    sampled_epoch_ms,
                    received_epoch_ms,
                    available_until,
                )
                .map_err(js_error)?;
            if clips.is_empty() {
                self.output.stop();
                return Ok(());
            }
            self.output.start_clips(clips)
        }

        pub fn pause(&mut self) -> Result<(), JsValue> {
            self.output.release()
        }
        pub fn stop(&mut self) {
            self.output.stop();
        }
        #[wasm_bindgen(js_name = setVolume)]
        pub fn set_volume(&mut self, volume: f64) -> Result<(), JsValue> {
            let mut controls = self.controls;
            controls.set_volume(volume).map_err(js_error)?;
            self.output.set_controls(controls)?;
            self.controls = controls;
            Ok(())
        }
        #[wasm_bindgen(js_name = setMuted)]
        pub fn set_muted(&mut self, muted: bool) -> Result<(), JsValue> {
            let mut controls = self.controls;
            controls.set_muted(muted);
            self.output.set_controls(controls)?;
            self.controls = controls;
            Ok(())
        }
    }
    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }
}

#[cfg(all(target_arch = "wasm32", feature = "renderer"))]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip_keeps_trimmed_loop_phase_fades_and_silent_gaps() {
        let mut scene = noon::Scene::new();
        scene
            .audio(
                AudioResource::from_interleaved(8000, 1, vec![0.125; 8000], 32000).unwrap(),
                AudioClipOptions {
                    start_time: 1.0,
                    source_start_frame: 2000,
                    source_end_frame: Some(6000),
                    duration: Some(3.0),
                    looped: true,
                    gain: 0.4,
                    fade_in: 0.5,
                    fade_out: 0.5,
                },
            )
            .unwrap();
        let session = scene.execution_session().unwrap();
        let bytes = encode(&session, 19).unwrap();
        let resources = WorkerAudioResources::decode(&bytes).unwrap();
        assert_eq!(resources.session(), 19);
        assert!(!resources.requires_horizon);
        for time in [0.0, 1.0, 1.25, 1.6, 2.75, 3.75, 4.0, 90.0] {
            let original = session.audio_playback_at(time).unwrap().collect::<Vec<_>>();
            let mirrored = resources.at_time(time).unwrap();
            assert_eq!(original.len(), mirrored.len());
            for (original, mirrored) in original.iter().zip(&mirrored) {
                assert_eq!(original.clip, mirrored.clip);
                assert_eq!(original.resource, mirrored.resource);
                assert_eq!(original.start_delay, mirrored.start_delay);
                assert_eq!(original.duration, mirrored.duration);
                assert_eq!(original.source_offset, mirrored.source_offset);
                assert_eq!(original.loop_region, mirrored.loop_region);
                assert_eq!(original.gain_points, mirrored.gain_points);
                assert_eq!(original.pcm, mirrored.pcm);
            }
        }
        assert!(WorkerAudioResources::decode(&bytes[..bytes.len() - 1]).is_err());
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(WorkerAudioResources::decode(&trailing).is_err());
        let mut invalid = bytes;
        let last = invalid.len() - 4;
        invalid[last..].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(WorkerAudioResources::decode(&invalid).is_err());
    }

    #[cfg(feature = "prepared-prefix")]
    #[test]
    fn prepared_mirror_keeps_delayed_pcm_but_matches_only_adopted_reader_instructions() {
        let mut scene = noon::Scene::new();
        scene
            .audio(
                AudioResource::from_interleaved(8000, 1, vec![0.125; 16_000], 64_000).unwrap(),
                AudioClipOptions {
                    start_time: 2.0,
                    source_start_frame: 4000,
                    source_end_frame: Some(12_000),
                    duration: Some(8.0),
                    looped: true,
                    gain: 0.5,
                    fade_in: 2.0,
                    fade_out: 2.0,
                },
            )
            .unwrap();
        let mut source = scene.execution_session().unwrap();
        source
            .begin_replay_retention(noon_runtime::ReplayLimits::default())
            .unwrap();
        let segment = source.wait_segment(1.0).unwrap();
        source.advance_to(1.0).unwrap();
        let (mut publisher, mut reader) = source.create_prepared_prefix(segment).unwrap();
        reader.seek(0.5).unwrap();
        let bytes = encode_prepared(&reader, 29).unwrap();
        let resources = WorkerAudioResources::decode(&bytes).unwrap();
        assert!(resources.requires_horizon);
        assert_eq!(resources.at_time(0.0).unwrap().len(), 1);
        assert!(resources.at_time_before(0.5, 1.0).unwrap().is_empty());
        let segment = source.wait_segment(4.0).unwrap();
        source.advance_to(5.0).unwrap();
        source.seal_replay().unwrap();
        let pending = source
            .finish_prepared_prefix(segment, &mut publisher)
            .unwrap();
        assert!(reader.audio_playback().unwrap().next().is_none());
        reader.append(pending).unwrap();
        for time in [0.5, 2.0, 2.5, 4.75, 5.0, 2.25] {
            reader.seek(time).unwrap();
            let original = reader.audio_playback().unwrap().collect::<Vec<_>>();
            let mirrored = resources.at_time_before(time, reader.frontier()).unwrap();
            assert_eq!(original.len(), mirrored.len());
            for (original, mirrored) in original.iter().zip(&mirrored) {
                assert_eq!(original.clip, mirrored.clip);
                assert_eq!(original.source_offset, mirrored.source_offset);
                assert_eq!(original.start_delay, mirrored.start_delay);
                assert_eq!(original.duration, mirrored.duration);
                assert_eq!(original.gain_points, mirrored.gain_points);
                assert_eq!(original.loop_region, mirrored.loop_region);
                assert_eq!(original.pcm, mirrored.pcm);
                assert!(time + mirrored.start_delay + mirrored.duration <= reader.frontier());
            }
        }
        let aged = resources
            .anchored_before(29, 2.5, 1000.0, 1250.0, 3.0)
            .unwrap();
        assert_eq!(aged.len(), 1);
        assert_eq!(aged[0].source_offset, 1.25);
        assert_eq!(aged[0].duration, 0.25);
        assert_eq!(aged[0].gain_points.points().last().unwrap().gain, 0.25);
        assert!(resources
            .anchored_before(29, 2.5, 1000.0, 1500.0, 3.0)
            .unwrap()
            .is_empty());
        assert!(resources
            .anchored_before(29, 2.5, 1000.0, 9000.0, 3.0)
            .unwrap()
            .is_empty());
        assert!(resources
            .anchored_before(30, 2.5, 1000.0, 1250.0, 3.0)
            .is_err());
        assert!(resources
            .anchored_before(29, 2.5, 1000.0, 999.0, 3.0)
            .is_err());
        for invalid in [f64::NAN, f64::INFINITY, -1.0] {
            assert!(resources
                .anchored_before(29, invalid, 1000.0, 1250.0, 3.0)
                .is_err());
            assert!(resources
                .anchored_before(29, 2.5, invalid, 1250.0, 3.0)
                .is_err());
            assert!(resources
                .anchored_before(29, 2.5, 1000.0, 1250.0, invalid)
                .is_err());
        }
    }
}
