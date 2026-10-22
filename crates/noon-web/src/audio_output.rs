//! Browser device output for an already admitted finite Rust audio plan.
//!
//! Clip timing and gain envelopes are supplied by the existing Runtime. This
//! adapter owns disposable browser nodes and PCM copies, never authored time.

mod transition;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ViewerAudioControls {
    volume: f64,
    muted: bool,
}

impl Default for ViewerAudioControls {
    fn default() -> Self {
        Self {
            volume: 1.0,
            muted: false,
        }
    }
}

impl ViewerAudioControls {
    pub(crate) fn set_volume(&mut self, volume: f64) -> Result<(), &'static str> {
        if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
            return Err("viewer audio volume must be finite and between zero and one");
        }
        self.volume = volume;
        Ok(())
    }

    pub(crate) fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    pub(crate) const fn volume(self) -> f64 {
        self.volume
    }

    pub(crate) const fn muted(self) -> bool {
        self.muted
    }

    pub(crate) fn device_gain(self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.volume as f32
        }
    }
}

/// Web Audio gain parameters use single precision even though authored gains
/// use doubles. Refuse an unrepresentable value before connecting any output.
fn device_gain(gain: f64) -> Result<f32, &'static str> {
    let converted = gain as f32;
    if !gain.is_finite() || gain < 0.0 || !converted.is_finite() {
        return Err("authored audio gain is not representable by Web Audio");
    }
    Ok(converted)
}

fn device_time(origin: f64, offset: f64) -> Result<f64, &'static str> {
    let value = origin + offset;
    if !origin.is_finite() || !offset.is_finite() || offset < 0.0 || !value.is_finite() {
        return Err("audio output time is not representable by Web Audio");
    }
    Ok(value)
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::{
        cell::{Cell, RefCell},
        collections::BTreeMap,
        rc::Rc,
    };

    use noon::ExecutionSession;
    use noon_core::AudioResourceHandle;
    use wasm_bindgen::{closure::Closure, JsCast, JsValue};
    use web_sys::{
        AudioBuffer, AudioBufferSourceNode, AudioContext, AudioContextState, AudioParam,
        AudioScheduledSourceNode, GainNode,
    };

    use super::{
        device_gain, device_time,
        transition::{GainRamp, ATTACK_SECONDS, RELEASE_SECONDS, VIEWER_RAMP_SECONDS},
        ViewerAudioControls,
    };

    struct DeviceVoice {
        source: AudioBufferSourceNode,
        gain: GainNode,
    }

    impl Drop for DeviceVoice {
        fn drop(&mut self) {
            let _ = web_sys::AudioScheduledSourceNode::stop(self.source.as_ref());
            let _ = self.source.disconnect();
            let _ = self.gain.disconnect();
        }
    }

    /// A disconnected/muted graph is prepared as a whole before any source may
    /// contribute sound. Dropping a partial preparation retires every node.
    struct DeviceGraph {
        voices: Vec<DeviceVoice>,
        bus: GainNode,
        ramp: GainRamp,
        last_voice: usize,
        natural_end: f64,
        ended: Option<Closure<dyn FnMut(web_sys::Event)>>,
    }

    impl DeviceGraph {
        /// The latest-ending native source retires this one disposable graph.
        /// A weak slot reference and the browser node's own identity avoid a
        /// reference cycle, an extra generation, or a cleanup timer.
        fn observe_retirement(&mut self, retiring: &Rc<RefCell<Option<Self>>>) {
            let slot = Rc::downgrade(retiring);
            let ended = Closure::new(move |event: web_sys::Event| {
                let Some(slot) = slot.upgrade() else {
                    return;
                };
                let Some(target) = event.target() else {
                    return;
                };
                let is_current = slot.borrow().as_ref().is_some_and(|graph| {
                    graph.voices[graph.last_voice]
                        .source
                        .unchecked_ref::<web_sys::EventTarget>()
                        == &target
                });
                if is_current {
                    // wasm-bindgen retains an invocation until its callback
                    // returns, so dropping its registered owner here is safe.
                    let graph = slot.borrow_mut().take();
                    drop(graph);
                }
            });
            AudioScheduledSourceNode::set_onended(
                self.voices[self.last_voice].source.as_ref(),
                Some(ended.as_ref().unchecked_ref()),
            );
            self.ended = Some(ended);
        }
    }

    impl Drop for DeviceGraph {
        fn drop(&mut self) {
            if self.ended.is_some() {
                AudioScheduledSourceNode::set_onended(
                    self.voices[self.last_voice].source.as_ref(),
                    None,
                );
            }
            self.bus.gain().set_value(0.0);
            let _ = self.bus.disconnect();
        }
    }

    struct PreparedVoice {
        buffer: AudioBuffer,
        start_delay: f64,
        duration: f64,
        source_offset: f64,
        loop_region: Option<(f64, f64)>,
        gain_points: Vec<(f64, f32)>,
    }

    pub(crate) struct WebAudioOutput {
        context: AudioContext,
        master: GainNode,
        buffers: BTreeMap<AudioResourceHandle, AudioBuffer>,
        graph: Option<DeviceGraph>,
        retiring: Rc<RefCell<Option<DeviceGraph>>>,
        controls: ViewerAudioControls,
        master_ramp: Rc<Cell<GainRamp>>,
        interrupted: Rc<Cell<bool>>,
        unlock_pending: Rc<Cell<bool>>,
        unlock_promise: Option<js_sys::Promise>,
        state_change: Closure<dyn FnMut(web_sys::Event)>,
    }

    impl WebAudioOutput {
        pub(crate) fn new(controls: ViewerAudioControls) -> Result<Self, JsValue> {
            if web_sys::window().is_none() {
                return Err(JsValue::from_str(
                    "direct audio output requires a browser Window execution context",
                ));
            }
            let context = AudioContext::new()?;
            let master = match context.create_gain() {
                Ok(master) => master,
                Err(error) => {
                    let _ = context.close();
                    return Err(error);
                }
            };
            master.gain().set_value(controls.device_gain());
            if let Err(error) = master.connect_with_audio_node(&context.destination()) {
                let _ = context.close();
                return Err(error);
            }
            let interrupted = Rc::new(Cell::new(false));
            let master_ramp = Rc::new(Cell::new(GainRamp::constant(controls.device_gain())));
            let state_change = {
                let context = context.clone();
                let master = master.clone();
                let interrupted = interrupted.clone();
                let master_ramp = master_ramp.clone();
                Closure::new(move |_: web_sys::Event| {
                    if context.state() != AudioContextState::Running {
                        // Never restart queued sound merely because the browser
                        // later resumes its device clock. The next explicit
                        // unlock and transport anchor must rebuild the output.
                        silence_parameter(&master.gain(), context.current_time());
                        master_ramp.set(GainRamp::constant(0.0));
                        interrupted.set(true);
                    }
                })
            };
            context.set_onstatechange(Some(state_change.as_ref().unchecked_ref()));
            Ok(Self {
                context,
                master,
                buffers: BTreeMap::new(),
                graph: None,
                retiring: Rc::new(RefCell::new(None)),
                controls,
                master_ramp,
                interrupted,
                unlock_pending: Rc::new(Cell::new(false)),
                unlock_promise: None,
                state_change,
            })
        }

        /// Reuse one outstanding native device operation. Its completion owns
        /// no renderer, playback command, or capability to revive a retired run.
        pub(crate) fn unlock(&mut self) -> Result<js_sys::Promise, JsValue> {
            self.stop();
            self.interrupted.set(false);
            let gain = self.controls.device_gain();
            let now = self.context.current_time();
            self.master.gain().cancel_scheduled_values(now)?;
            self.master.gain().set_value_at_time(gain, now)?;
            self.master_ramp.set(GainRamp::constant(gain));
            if self.unlock_pending.get() {
                return Ok(self.unlock_promise.as_ref().unwrap().clone());
            }
            let promise = self.context.resume()?;
            self.unlock_pending.set(true);
            self.unlock_promise = Some(promise.clone());
            let pending = self.unlock_pending.clone();
            let completion = wasm_bindgen_futures::JsFuture::from(promise.clone());
            wasm_bindgen_futures::spawn_local(async move {
                // Observe rejection even if the host retires before attaching
                // its handler. Only the original caller decides what to do.
                let _ = completion.await;
                pending.set(false);
            });
            Ok(promise)
        }

        pub(crate) fn is_ready(&self) -> bool {
            self.context.state() == AudioContextState::Running && !self.interrupted.get()
        }

        pub(crate) fn is_scheduled(&self) -> bool {
            self.graph.is_some()
        }

        pub(crate) fn voice_count(&self) -> usize {
            self.retire_completed(self.context.current_time());
            self.graph.as_ref().map_or(0, |graph| graph.voices.len())
                + self
                    .retiring
                    .borrow()
                    .as_ref()
                    .map_or(0, |graph| graph.voices.len())
        }

        pub(crate) fn set_controls(
            &mut self,
            controls: ViewerAudioControls,
        ) -> Result<(), JsValue> {
            self.controls = controls;
            let now = self.context.current_time();
            if self.interrupted.get() {
                silence_parameter(&self.master.gain(), now);
                self.master_ramp.set(GainRamp::constant(0.0));
                return Ok(());
            }
            let ramp = self
                .master_ramp
                .get()
                .retarget(controls.device_gain(), now, VIEWER_RAMP_SECONDS)
                .map_err(js_error)?;
            apply_ramp(&self.master.gain(), ramp)?;
            self.master_ramp.set(ramp);
            Ok(())
        }

        /// Lifecycle/failure cancellation is immediate, including a release
        /// already scheduled on the device. It never leaves a delayed tail.
        pub(crate) fn stop(&mut self) {
            self.graph = None;
            self.retiring.borrow_mut().take();
        }

        /// Ordinary transport changes release one audible graph. A newer graph
        /// remains at zero until that release ends; rapid changes can therefore
        /// replace it without truncating an audible tail or accumulating graphs.
        pub(crate) fn release(&mut self) -> Result<(), JsValue> {
            if !self.is_ready() {
                self.stop();
                return Ok(());
            }
            let result = self.release_at(self.context.current_time());
            if result.is_err() {
                self.stop();
            }
            result
        }

        fn retire_completed(&self, now: f64) {
            let complete = self
                .retiring
                .borrow()
                .as_ref()
                .is_some_and(|graph| now >= graph.ramp.end());
            if complete {
                let graph = self.retiring.borrow_mut().take();
                drop(graph);
            }
        }

        fn release_at(&mut self, now: f64) -> Result<(), JsValue> {
            self.retire_completed(now);
            let Some(mut graph) = self.graph.take() else {
                return Ok(());
            };
            if graph.ramp.value_at(now) == 0.0 || graph.natural_end <= now {
                return Ok(());
            }
            if self.retiring.borrow().is_some() {
                return Err(JsValue::from_str(
                    "audio transition cannot retain two audible release graphs",
                ));
            }
            let duration = RELEASE_SECONDS.min(graph.natural_end - now);
            let ramp = graph.ramp.retarget(0.0, now, duration).map_err(js_error)?;
            apply_ramp(&graph.bus.gain(), ramp)?;
            graph.ramp = ramp;
            graph.observe_retirement(&self.retiring);
            for voice in &graph.voices {
                AudioScheduledSourceNode::stop_with_when(voice.source.as_ref(), ramp.end())?;
            }
            *self.retiring.borrow_mut() = Some(graph);
            Ok(())
        }

        pub(crate) fn preload(&mut self, session: &ExecutionSession) -> Result<(), JsValue> {
            self.prepare(session).map(|_| ())
        }

        fn prepare(&mut self, session: &ExecutionSession) -> Result<Vec<PreparedVoice>, JsValue> {
            let mut prepared = Vec::new();
            for clip in session.audio_playback().map_err(js_error)? {
                let length = u32::try_from(clip.pcm.frames()).map_err(|_| {
                    JsValue::from_str("audio PCM frame count exceeds the Web Audio buffer limit")
                })?;
                // Validate every authored automation value before source nodes
                // are created or started. A later invalid clip cannot cause a
                // valid earlier clip to emit a partial batch.
                let gain_points = clip
                    .gain_points
                    .points()
                    .iter()
                    .map(|point| {
                        Ok((
                            device_time(0.0, point.offset).map_err(js_error)?,
                            device_gain(point.gain).map_err(js_error)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, JsValue>>()?;
                device_time(clip.start_delay, clip.duration).map_err(js_error)?;
                if let std::collections::btree_map::Entry::Vacant(entry) =
                    self.buffers.entry(clip.resource)
                {
                    let buffer = self.context.create_buffer(
                        u32::from(clip.pcm.channels()),
                        length,
                        clip.pcm.sample_rate() as f32,
                    )?;
                    let channels = usize::from(clip.pcm.channels());
                    let mut channel = vec![0.0; clip.pcm.frames()];
                    for index in 0..channels {
                        for (destination, source) in channel
                            .iter_mut()
                            .zip(clip.pcm.samples().chunks_exact(channels))
                        {
                            *destination = source[index];
                        }
                        buffer.copy_to_channel(&channel, index as i32)?;
                    }
                    entry.insert(buffer);
                }
                prepared.push(PreparedVoice {
                    buffer: self.buffers[&clip.resource].clone(),
                    start_delay: clip.start_delay,
                    duration: clip.duration,
                    source_offset: clip.source_offset,
                    loop_region: clip.loop_region.map(|region| (region.start, region.end)),
                    gain_points,
                });
            }
            Ok(prepared)
        }

        /// Materialize the Runtime's finite schedule at the same accepted host
        /// anchor as visual playback. Browser time is only a device offset.
        pub(crate) fn start(&mut self, session: &ExecutionSession) -> Result<(), JsValue> {
            if !self.is_ready() {
                return Err(JsValue::from_str(
                    "enable audio from a user gesture before resuming audio playback",
                ));
            }
            let prepared = self.prepare(session)?;
            let bus = self.context.create_gain()?;
            bus.gain().set_value(0.0);
            bus.connect_with_audio_node(&self.master)?;
            let mut graph = DeviceGraph {
                voices: Vec::with_capacity(prepared.len()),
                bus,
                ramp: GainRamp::constant(0.0),
                last_voice: 0,
                natural_end: 0.0,
                ended: None,
            };
            for voice in &prepared {
                let source = self.context.create_buffer_source()?;
                let gain = self.context.create_gain()?;
                graph.voices.push(DeviceVoice { source, gain });
                let device = graph.voices.last().unwrap();
                device.source.set_buffer(Some(&voice.buffer));
                if let Some((start, end)) = voice.loop_region {
                    device.source.set_loop(true);
                    device.source.set_loop_start(start);
                    device.source.set_loop_end(end);
                }
                device.source.connect_with_audio_node(&device.gain)?;
                device.gain.connect_with_audio_node(&graph.bus)?;
            }
            let origin = self.context.current_time();
            // Check the complete device-time conversion before starting even a
            // muted source. Extremely large finite authored times can overflow
            // or collapse when added to a device clock.
            for (index, voice) in prepared.iter().enumerate() {
                let start = device_time(origin, voice.start_delay).map_err(js_error)?;
                let end = device_time(start, voice.duration).map_err(js_error)?;
                if end <= start {
                    return Err(JsValue::from_str(
                        "audio interval is too short for the current Web Audio clock",
                    ));
                }
                for &(offset, _) in &voice.gain_points {
                    device_time(start, offset).map_err(js_error)?;
                }
                if end > graph.natural_end {
                    graph.natural_end = end;
                    graph.last_voice = index;
                }
            }
            for (voice, device) in prepared.iter().zip(&graph.voices) {
                let start = origin + voice.start_delay;
                for (index, &(offset, gain)) in voice.gain_points.iter().enumerate() {
                    if index == 0 {
                        device.gain.gain().set_value_at_time(gain, start + offset)?;
                    } else {
                        device
                            .gain
                            .gain()
                            .linear_ramp_to_value_at_time(gain, start + offset)?;
                    }
                }
            }
            for (voice, device) in prepared.iter().zip(&graph.voices) {
                device
                    .source
                    .start_with_when_and_grain_offset_and_grain_duration(
                        origin + voice.start_delay,
                        voice.source_offset,
                        voice.duration,
                    )?;
            }
            let now = self.context.current_time();
            self.release_at(now)?;
            let attack_start = self
                .retiring
                .borrow()
                .as_ref()
                .map_or(now, |retiring| retiring.ramp.end().max(now));
            // Sources already run at their exact authored offsets. Only their
            // disposable bus is gated during the old graph's bounded release;
            // opening it later cannot shift the audio/visual anchor.
            let ramp =
                GainRamp::linear(0.0, 1.0, attack_start, ATTACK_SECONDS).map_err(js_error)?;
            apply_ramp(&graph.bus.gain(), ramp)?;
            graph.ramp = ramp;
            self.graph = Some(graph);
            Ok(())
        }
    }

    impl Drop for WebAudioOutput {
        fn drop(&mut self) {
            self.context.set_onstatechange(None);
            // Keep the closure alive until its browser registration is removed.
            let _ = &self.state_change;
            self.stop();
            silence_parameter(&self.master.gain(), self.context.current_time());
            let _ = self.master.disconnect();
            let _ = self.context.close();
        }
    }

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }

    fn apply_ramp(parameter: &AudioParam, ramp: GainRamp) -> Result<(), JsValue> {
        parameter.cancel_scheduled_values(ramp.start())?;
        // Cancelling a running ramp removes its future endpoint. Close that
        // linear segment at the observed value before adding the new one; a
        // setValueAtTime alone would replace its preceding curve with a step.
        parameter.linear_ramp_to_value_at_time(ramp.from(), ramp.start())?;
        parameter.linear_ramp_to_value_at_time(ramp.target(), ramp.end())?;
        Ok(())
    }

    fn silence_parameter(parameter: &AudioParam, now: f64) {
        let _ = parameter.cancel_scheduled_values(now);
        let _ = parameter.set_value_at_time(0.0, now);
        parameter.set_value(0.0);
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) use wasm::WebAudioOutput;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_mute_retains_volume_and_rejects_invalid_changes_atomically() {
        let mut controls = ViewerAudioControls::default();
        controls.set_volume(0.25).unwrap();
        controls.set_muted(true);
        assert!(controls.muted());
        assert_eq!(controls.device_gain(), 0.0);
        for invalid in [-1.0, 1.01, f64::INFINITY, f64::NAN] {
            assert!(controls.set_volume(invalid).is_err());
            assert_eq!(controls.volume(), 0.25);
            assert!(controls.muted());
        }
        controls.set_muted(false);
        assert_eq!(controls.device_gain(), 0.25);
    }

    #[test]
    fn browser_gain_admission_rejects_double_values_that_overflow_audio_param() {
        assert_eq!(device_gain(0.0), Ok(0.0));
        assert_eq!(device_gain(f64::from(f32::MAX)), Ok(f32::MAX));
        assert!(device_gain(f64::MAX).is_err());
        assert!(device_gain(f64::NAN).is_err());
        assert!(device_gain(-1.0).is_err());
    }

    #[test]
    fn browser_time_conversion_rejects_unrepresentable_offsets() {
        assert_eq!(device_time(10.0, 0.25), Ok(10.25));
        assert!(device_time(f64::MAX, f64::MAX).is_err());
        assert!(device_time(1.0, -0.5).is_err());
        assert!(device_time(f64::INFINITY, 0.0).is_err());
    }
}
