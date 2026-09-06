//! Transport adapter for an already-lowered semantic session; never parses authoring JSON.
#[cfg(any(target_arch = "wasm32", test))]
use noon::TimelineWakeState;
use noon::{
    CallbackAdvance, CallbackPhaseToken, CallbackReadRequest, CallbackReadValue,
    EffectivePropertyBatch, EffectiveSemanticPropertyWrite, ExecutionSession, RuntimeIdentity,
};
use noon_core::{
    ExecutionRevision, FrameEpoch, PublicationContext, Rect, SceneRevision, SemanticNodeId, Style,
    Transform2D,
};
#[cfg(any(target_arch = "wasm32", test))]
use noon_core::{
    NativeEventOccurrence, NativeEventSource, NativeInputValue, NativeStateSource, ReactiveValue,
    Vec2,
};
use serde::{Deserialize, Serialize};

#[cfg(any(target_arch = "wasm32", test))]
use crate::{
    BrowserExecutionCadence, BrowserExecutionWakeClock, BrowserExecutionWakePlan, BrowserHostWake,
};
use crate::{
    PlaybackClock, RendererObservationRequest, RetainedExecutionDeltaEncoder,
    RetainedExecutionDeltaEnvelope, RetainedResourceBundle,
};

/// A browser-host wake observation derived from one player-owned execution session.
///
/// The browser receives only this derived scheduling directive. Runtime/segment identity,
/// authored time, event cursors, and interpolation remain in the shared session.
#[cfg(any(target_arch = "wasm32", test))]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
#[derive(Debug)]
pub struct WasmExecutionWake {
    present_now: bool,
    cadence: BrowserExecutionCadence,
    timer_after_milliseconds: Option<f64>,
}

/// One callback-aware step while driving the current continuation segment.
///
/// A required phase carries the existing cross-context callback payload and
/// leaves the public frame and presentation clock pinned. The host commits that
/// exact token through `commitCallbackPhaseJson`, then retries with the same wall
/// timestamp. Only a ready step can report the segment endpoint.
#[cfg(any(target_arch = "wasm32", test))]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
#[derive(Debug)]
pub struct WasmLiveSegmentDrive {
    callback_phase_json: Option<String>,
    reached_endpoint: bool,
}

#[cfg(any(target_arch = "wasm32", test))]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
impl WasmLiveSegmentDrive {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(getter, js_name = callbackPhaseJson))]
    pub fn callback_phase_json(&self) -> Option<String> {
        self.callback_phase_json.clone()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(getter, js_name = reachedEndpoint))]
    pub fn reached_endpoint(&self) -> bool {
        self.reached_endpoint
    }
}

#[cfg(any(target_arch = "wasm32", test))]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
impl WasmExecutionWake {
    fn from_plan(plan: BrowserExecutionWakePlan, timer_after_milliseconds: Option<f64>) -> Self {
        Self {
            present_now: plan.present_now(),
            cadence: plan.cadence(),
            timer_after_milliseconds,
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(getter, js_name = presentNow))]
    pub fn present_now(&self) -> bool {
        self.present_now
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(getter, js_name = cadence))]
    pub fn cadence(&self) -> String {
        match self.cadence {
            BrowserExecutionCadence::AnimationFrame => "animation_frame",
            BrowserExecutionCadence::TimerAtSceneTime(_) => "timer",
            BrowserExecutionCadence::Idle => "idle",
        }
        .to_owned()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(getter, js_name = timerAfterMilliseconds))]
    pub fn timer_after_milliseconds(&self) -> Option<f64> {
        self.timer_after_milliseconds
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub struct SemanticExecutionPlayer {
    session: ExecutionSession,
    clock: PlaybackClock,
    encoder: RetainedExecutionDeltaEncoder,
    /// Immutable text/font/vector dependencies transferred once at the genuine
    /// authoring-worker to render-worker boundary.
    resource_bundle: Vec<u8>,
    snapshot_sent: bool,
    /// Exact phase metadata needed only to re-anchor presentation after the
    /// session atomically commits its own pending callback phase. The session
    /// remains the sole owner of callback progression and termination.
    pending_callback_phase: Option<(CallbackPhaseToken, f64)>,
    /// Present when the player came from canonical authoring. This is the one
    /// semantic store that produced `session`, not an execution mirror.
    #[cfg(any(target_arch = "wasm32", test))]
    semantics: Option<std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>>,
    #[cfg(any(target_arch = "wasm32", test))]
    semantic_root: Option<noon_core::SemanticNodeId>,
    /// Continuation metadata for this one session-owned runtime, never a
    /// frontend scheduler or animation state mirror.
    #[cfg(any(target_arch = "wasm32", test))]
    live_segment: Option<LiveSegmentReceipt>,
    /// Wall-to-authored conversion for one browser-host continuation lease. This derives
    /// targets from the current segment wake state; it is not a second timeline.
    #[cfg(any(target_arch = "wasm32", test))]
    live_wake_clock: BrowserExecutionWakeClock,
    /// Host-local occurrence order for the genuine browser control-port input
    /// boundary. Returning and re-leasing this player preserves the sequence.
    #[cfg(any(target_arch = "wasm32", test))]
    next_native_event_sequence: u64,
}

/// A host continuation receipt retains its endpoint after completion for renderer
/// recovery/scrubbing, while only Pending permits one begin/drive/complete lease.
#[cfg(any(target_arch = "wasm32", test))]
#[derive(Clone, Copy)]
enum LiveSegmentReceipt {
    Pending(noon::ExecutionSegment),
    Completed(noon::ExecutionSegment),
}

#[cfg(any(target_arch = "wasm32", test))]
impl LiveSegmentReceipt {
    fn segment(self) -> noon::ExecutionSegment {
        match self {
            Self::Pending(segment) | Self::Completed(segment) => segment,
        }
    }
}

impl SemanticExecutionPlayer {
    fn playback_clock(session: &ExecutionSession, duration: f64) -> Result<PlaybackClock, String> {
        if session.has_required_callbacks() {
            Ok(PlaybackClock::once())
        } else {
            PlaybackClock::looping(duration).map_err(|error| error.to_string())
        }
    }

    fn retain_callback_phase(
        &mut self,
        invocations: Vec<noon::RequiredCallbackInvocation>,
        overlay: noon::CallbackPhaseOverlay,
    ) -> Result<String, String> {
        let token = overlay.token();
        let phase_time = overlay.time();
        let json = match Self::callback_phase_json(&overlay, &invocations) {
            Ok(json) => json,
            Err(error) => {
                self.session
                    .fail_required_callback_phase(token)
                    .map_err(|termination| termination.to_string())?;
                return Err(error);
            }
        };
        self.pending_callback_phase = Some((token, phase_time));
        Ok(json)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn validate_live_loop_duration(&self, duration: f64) -> Result<(), String> {
        if !duration.is_finite() || duration <= 0.0 {
            return Err(format!("invalid playback loop duration {duration}"));
        }
        let Some(required) = self.live_handoff_duration() else {
            return Ok(());
        };
        if duration < required {
            return Err(format!(
                "playback duration {duration} is shorter than live handoff duration {required}"
            ));
        }
        Ok(())
    }

    /// Prepare a presentation clock for an already-validated runtime time.
    ///
    /// The caller builds this clone before a fallible runtime operation and only
    /// installs it after that operation succeeds. This keeps the presentation
    /// clock and published frame atomic without making either one a second time
    /// authority.
    #[cfg(any(target_arch = "wasm32", test))]
    fn live_clock_at(
        &self,
        time: f64,
        segment_end: f64,
        playing: bool,
    ) -> Result<PlaybackClock, String> {
        let mut clock = self.clock.clone();
        if let Some(duration) = clock.loop_duration() {
            let required = time.max(segment_end);
            if required > duration {
                clock
                    .set_loop_duration(required)
                    .map_err(|error| error.to_string())?;
            }
        }
        clock.seek(time).map_err(|error| error.to_string())?;
        clock.pause();
        if playing {
            clock.resume();
        }
        Ok(clock)
    }

    pub fn from_session(
        session: ExecutionSession,
        duration: f64,
        transport_session: u32,
    ) -> Result<Self, String> {
        let clock = Self::playback_clock(&session, duration)?;
        let resource_bundle = Self::resource_bundle_for(&session)?;
        Ok(Self {
            session,
            clock,
            encoder: RetainedExecutionDeltaEncoder::new(transport_session),
            resource_bundle,
            snapshot_sent: false,
            pending_callback_phase: None,
            #[cfg(any(target_arch = "wasm32", test))]
            semantics: None,
            #[cfg(any(target_arch = "wasm32", test))]
            semantic_root: None,
            #[cfg(any(target_arch = "wasm32", test))]
            live_segment: None,
            #[cfg(any(target_arch = "wasm32", test))]
            live_wake_clock: BrowserExecutionWakeClock::default(),
            #[cfg(any(target_arch = "wasm32", test))]
            next_native_event_sequence: 0,
        })
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn from_live_session(
        session: ExecutionSession,
        semantics: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        semantic_root: noon_core::SemanticNodeId,
        duration: f64,
        transport_session: u32,
    ) -> Result<Self, String> {
        let clock = Self::playback_clock(&session, duration)?;
        let resource_bundle = Self::resource_bundle_for(&session)?;
        Ok(Self {
            session,
            clock,
            encoder: RetainedExecutionDeltaEncoder::new(transport_session),
            resource_bundle,
            snapshot_sent: false,
            pending_callback_phase: None,
            semantics: Some(semantics),
            semantic_root: Some(semantic_root),
            live_segment: None,
            live_wake_clock: BrowserExecutionWakeClock::default(),
            next_native_event_sequence: 0,
        })
    }

    /// Change only derived transport framing while retaining the same runtime.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn rebind_transport(
        &mut self,
        duration: f64,
        transport_session: u32,
    ) -> Result<(), String> {
        if self.pending_callback_phase.is_some() {
            return Err(
                "cannot rebind transport while a required callback phase is pending".into(),
            );
        }
        self.validate_live_loop_duration(duration)?;
        let mut clock = self.clock.clone();
        if !self.session.has_required_callbacks() {
            clock
                .set_loop_duration(duration)
                .map_err(|error| error.to_string())?;
        }
        // Live publication may have installed sparse text/font dependencies after
        // this player was bootstrapped. Refresh only at the explicit cross-worker
        // handoff boundary so ordinary typed in-process property edits stay local.
        let resource_bundle = Self::resource_bundle_for(&self.session)?;
        self.clock = clock;
        self.resource_bundle = resource_bundle;
        self.encoder = RetainedExecutionDeltaEncoder::new(transport_session);
        self.snapshot_sent = false;
        // A transport recovery reuses this runtime but begins a new host lease.
        // Re-anchor the derived wall conversion at its next wake so elapsed wall
        // time while no endpoint owned the player cannot advance authored time.
        self.live_wake_clock = BrowserExecutionWakeClock::default();
        Ok(())
    }

    /// The authored duration needed to hand this live session to presentation.
    ///
    /// Retain the latest continuation endpoint across presentation scrubbing.
    /// An active continuation must keep that endpoint addressable before completion.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_handoff_duration(&self) -> Option<f64> {
        self.semantics.as_ref()?;
        Some(
            self.live_segment
                .map_or(self.session.frame().time, |segment| {
                    self.session.frame().time.max(segment.segment().end_time())
                }),
        )
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn has_required_callbacks(&self) -> bool {
        self.session.has_required_callbacks()
    }

    /// The authored scene revision represented by this runtime.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn scene_revision(&self) -> noon_core::SceneRevision {
        self.session.publication_context().scene_revision()
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_set_translation(
        &mut self,
        mobject: &noon::Mobject,
        x: f64,
        y: f64,
    ) -> Result<(), String> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .set_translation(mobject, x, y)
        .map(|_| ())
        .map_err(|error| error.to_string())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_fill(
        &mut self,
        mobject: &noon::Mobject,
        red: f64,
        green: f64,
        blue: f64,
        opacity: f64,
    ) -> Result<(), String> {
        self.with_live_session(|live| live.set_fill(mobject, red, green, blue, opacity))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_fill_color(
        &mut self,
        mobject: &noon::Mobject,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), String> {
        self.with_live_session(|live| live.set_fill_color(mobject, red, green, blue, alpha))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_disable_fill(&mut self, mobject: &noon::Mobject) -> Result<(), String> {
        self.with_live_session(|live| live.disable_fill(mobject))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_fill_opacity(
        &mut self,
        mobject: &noon::Mobject,
        opacity: f64,
    ) -> Result<(), String> {
        self.with_live_session(|live| live.set_fill_opacity(mobject, opacity))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_color(
        &mut self,
        mobject: &noon::Mobject,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), String> {
        self.with_live_session(|live| live.set_color(mobject, red, green, blue, alpha))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_stroke(
        &mut self,
        mobject: &noon::Mobject,
        red: f64,
        green: f64,
        blue: f64,
        opacity: f64,
    ) -> Result<(), String> {
        self.with_live_session(|live| live.set_stroke(mobject, red, green, blue, opacity))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_stroke_color(
        &mut self,
        mobject: &noon::Mobject,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), String> {
        self.with_live_session(|live| live.set_stroke_color(mobject, red, green, blue, alpha))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_disable_stroke(&mut self, mobject: &noon::Mobject) -> Result<(), String> {
        self.with_live_session(|live| live.disable_stroke(mobject))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_stroke_opacity(
        &mut self,
        mobject: &noon::Mobject,
        opacity: f64,
    ) -> Result<(), String> {
        self.with_live_session(|live| live.set_stroke_opacity(mobject, opacity))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_opacity(
        &mut self,
        mobject: &noon::Mobject,
        opacity: f64,
    ) -> Result<(), String> {
        self.with_live_session(|live| live.set_opacity(mobject, opacity))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_object_opacity(
        &mut self,
        mobject: &noon::Mobject,
        opacity: f64,
    ) -> Result<(), String> {
        self.with_live_session(|live| live.set_object_opacity(mobject, opacity))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    fn with_live_session<T>(
        &mut self,
        operation: impl FnOnce(&mut noon::LiveSession<'_>) -> Result<T, noon::LiveSessionError>,
    ) -> Result<T, String> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        operation(&mut noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        ))
        .map_err(|error| error.to_string())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_replace_content(
        &mut self,
        target: &noon::Mobject,
        source: &noon::Mobject,
    ) -> Result<(), String> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .replace_content(target, source)
        .map(|_| ())
        .map_err(|error| error.to_string())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_shift(
        &mut self,
        mobject: &noon::Mobject,
        x: f64,
        y: f64,
    ) -> Result<(), String> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .shift(mobject, x, y)
        .map(|_| ())
        .map_err(|error| error.to_string())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_scale(
        &mut self,
        mobject: &noon::Mobject,
        x: f64,
        y: f64,
    ) -> Result<(), String> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .set_scale(mobject, x, y)
        .map(|_| ())
        .map_err(|error| error.to_string())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_scale(
        &mut self,
        mobject: &noon::Mobject,
        x: f64,
        y: f64,
    ) -> Result<(), String> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .scale(mobject, x, y)
        .map(|_| ())
        .map_err(|error| error.to_string())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_rotation(
        &mut self,
        mobject: &noon::Mobject,
        angle: f64,
    ) -> Result<(), String> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .set_rotation(mobject, angle)
        .map(|_| ())
        .map_err(|error| error.to_string())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_rotate(
        &mut self,
        mobject: &noon::Mobject,
        angle: f64,
    ) -> Result<(), String> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .rotate(mobject, angle)
        .map(|_| ())
        .map_err(|error| error.to_string())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_effective(
        &mut self,
        mobject: &noon::Mobject,
    ) -> Result<noon::EffectiveMobjectState, String> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .effective(mobject)
        .map_err(|error| error.to_string())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_effective_layout(
        &mut self,
        mobject: &noon::Mobject,
    ) -> Result<noon::EffectiveMobjectLayout, String> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .effective_layout(mobject)
        .map_err(|error| error.to_string())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_add(&mut self, mobject: &noon::Mobject) -> Result<(), String> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .add(mobject)
        .map(|_| ())
        .map_err(|error| error.to_string())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_remove(&mut self, mobject: &noon::Mobject) -> Result<(), String> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .remove(mobject)
        .map(|_| ())
        .map_err(|error| error.to_string())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn require_completed_live_segment(&self) -> Result<(), String> {
        self.require_callback_progression_available()?;
        if self.has_pending_live_segment() {
            return Err("complete the current live segment before continuing".into());
        }
        Ok(())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn require_callback_progression_available(&self) -> Result<(), String> {
        if let Some(termination) = self.session.callback_termination() {
            return Err(format!(
                "required callback progression terminated: {:?}",
                termination.kind()
            ));
        }
        if self.pending_callback_phase.is_some() {
            return Err("a required callback phase is pending host completion".into());
        }
        Ok(())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_play_animation(
        &mut self,
        animation: &noon::DeclaredAnimation,
    ) -> Result<f64, String> {
        self.require_completed_live_segment()?;
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        let segment = noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .play_animation(animation)
        .map_err(|error| error.to_string())?;
        let end_time = segment.end_time();
        self.clock = self
            .live_clock_at(self.session.frame().time, end_time, true)
            .expect("validated execution segment must produce a valid presentation clock");
        self.live_segment = Some(LiveSegmentReceipt::Pending(segment));
        self.live_wake_clock = BrowserExecutionWakeClock::default();
        Ok(end_time)
    }

    /// Atomically declare and activate one ordinary affine transform in the
    /// live session, then retain its normal continuation segment.
    ///
    /// The declaration belongs to the shared semantic store.  This wrapper
    /// owns no target snapshot or timeline cursor; callers drive and complete
    /// the returned segment through the existing live methods below.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_declare_and_activate_transform_to(
        &mut self,
        source: &noon::Mobject,
        target: &noon::Mobject,
        options: noon_core::AnimationOptions,
    ) -> Result<f64, String> {
        self.require_completed_live_segment()?;
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        let segment = noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .declare_and_activate_transform_to(source, target, options)
        .map_err(|error| error.to_string())?;
        let end_time = segment.end_time();
        self.clock = self
            .live_clock_at(self.session.frame().time, end_time, true)
            .expect("validated execution segment must produce a valid presentation clock");
        self.live_segment = Some(LiveSegmentReceipt::Pending(segment));
        self.live_wake_clock = BrowserExecutionWakeClock::default();
        Ok(end_time)
    }

    /// Atomically declare and activate one shared basic fade, retaining its
    /// ordinary continuation segment in this one session-owned player.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_declare_and_activate_fade(
        &mut self,
        target: &noon::Mobject,
        direction: noon_core::SemanticFadeDirection,
        options: noon_core::AnimationOptions,
    ) -> Result<f64, String> {
        self.require_completed_live_segment()?;
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        let segment = noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .declare_and_activate_fade(target, direction, options)
        .map_err(|error| error.to_string())?;
        let end_time = segment.end_time();
        self.clock = self
            .live_clock_at(self.session.frame().time, end_time, true)
            .expect("validated execution segment must produce a valid presentation clock");
        self.live_segment = Some(LiveSegmentReceipt::Pending(segment));
        self.live_wake_clock = BrowserExecutionWakeClock::default();
        Ok(end_time)
    }

    /// Query root membership from the exact shared live session. This is a
    /// derived wrapper observation, never a frontend lifecycle authority.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_contains(&mut self, target: &noon::Mobject) -> Result<bool, String> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .contains(target)
        .map_err(|error| error.to_string())
    }

    /// Atomically declare and activate one flat prepared transform composition.
    ///
    /// The borrowed requests retain shared semantic handles only. Scheduling,
    /// effective capture, identity promotion, and runtime publication remain in
    /// the shared Rust compiler/session path.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_declare_and_activate_transform_composition(
        &mut self,
        kind: noon_core::SemanticAnimationCompositionKind,
        children: &[noon::TransformToRequest<'_>],
        composition_options: noon_core::AnimationOptions,
        play_options: noon_core::AnimationOptions,
    ) -> Result<f64, String> {
        self.require_completed_live_segment()?;
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        let segment = noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .declare_and_activate_transform_composition(
            kind,
            children,
            composition_options,
            play_options,
        )
        .map_err(|error| error.to_string())?;
        let end_time = segment.end_time();
        self.clock = self
            .live_clock_at(self.session.frame().time, end_time, true)
            .expect("validated execution segment must produce a valid presentation clock");
        self.live_segment = Some(LiveSegmentReceipt::Pending(segment));
        self.live_wake_clock = BrowserExecutionWakeClock::default();
        Ok(end_time)
    }

    /// Atomically append and activate one canonical scalar tracker interval,
    /// retaining its ordinary continuation segment in this player.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_declare_and_activate_value_tracker(
        &mut self,
        tracker: &noon::ValueTracker,
        target: f64,
        duration: f64,
        rate_func: noon_core::RateFunction,
    ) -> Result<f64, String> {
        self.require_completed_live_segment()?;
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        let segment = noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .declare_and_activate_value_tracker(tracker, target, duration, rate_func)
        .map_err(|error| error.to_string())?;
        let end_time = segment.end_time();
        self.clock = self
            .live_clock_at(self.session.frame().time, end_time, true)
            .expect("validated scalar segment must produce a valid presentation clock");
        self.live_segment = Some(LiveSegmentReceipt::Pending(segment));
        self.live_wake_clock = BrowserExecutionWakeClock::default();
        Ok(end_time)
    }

    /// Create and sparsely enroll one scalar tracker in this retained session.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_value_tracker(
        &mut self,
        initial: f64,
    ) -> Result<noon::ValueTracker, String> {
        self.require_completed_live_segment()?;
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .value_tracker(initial)
        .map_err(|error| error.to_string())
    }

    /// Create a detached target through the retained session so its semantic
    /// publication remains coherent with this runtime. Detached target rows do
    /// not create execution objects or frame work.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_target_editor(
        &mut self,
        source: &noon::Mobject,
    ) -> Result<noon::Mobject, String> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .target_editor(source)
        .map_err(|error| error.to_string())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_wait(&mut self, duration: f64) -> Result<f64, String> {
        self.require_completed_live_segment()?;
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        let segment = noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .wait_segment(duration)
        .map_err(|error| error.to_string())?;
        let end_time = segment.end_time();
        self.clock = self
            .live_clock_at(self.session.frame().time, end_time, true)
            .expect("validated execution segment must produce a valid presentation clock");
        self.live_segment = Some(LiveSegmentReceipt::Pending(segment));
        self.live_wake_clock = BrowserExecutionWakeClock::default();
        Ok(end_time)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_segment(&self) -> Result<noon::ExecutionSegment, String> {
        match self.live_segment {
            Some(LiveSegmentReceipt::Pending(segment)) => Ok(segment),
            _ => Err("play an animation or wait before driving a live segment".to_owned()),
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn has_pending_live_segment(&self) -> bool {
        matches!(self.live_segment, Some(LiveSegmentReceipt::Pending(_)))
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn reject_required_callback_segment(&self) -> Result<(), String> {
        if self.session.has_required_callbacks() {
            return Err(
                "ordinary endpoint-only execution cannot run required callbacks; use a continuation"
                    .into(),
            );
        }
        Ok(())
    }

    /// Observe browser wake mechanics for the active shared continuation segment.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_segment_wake(
        &mut self,
        wall_time_ms: f64,
    ) -> Result<WasmExecutionWake, String> {
        self.require_callback_progression_available()?;
        let segment = self.live_segment()?;
        let plan = BrowserExecutionWakePlan::from_pending_segment(&self.session, segment);
        let directive = self
            .live_wake_clock
            .directive(plan, wall_time_ms, self.session.frame().time)
            .ok_or("invalid browser wall timestamp or authored continuation time")?;
        let timer_after_milliseconds = match directive.wake() {
            BrowserHostWake::TimerAfterMilliseconds(delay) => Some(delay),
            BrowserHostWake::AnimationFrame | BrowserHostWake::Idle => None,
        };
        Ok(WasmExecutionWake::from_plan(plan, timer_after_milliseconds))
    }

    /// Project the generic player's runtime-owned wake state through its one
    /// browser playback clock.
    ///
    /// `wall_time_ms` comes from this authoring/engine context. Renderer-worker
    /// timestamps are admission signals only and may use a different time origin.
    /// Required callbacks pin the clock until their exact batch commits. A looping
    /// player adds the loop boundary only when the execution session retains actual
    /// timeline history, allowing a clean static scene to settle at O(0).
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn execution_wake(
        &mut self,
        wall_time_ms: f64,
    ) -> Result<WasmExecutionWake, String> {
        let callback_blocked =
            self.pending_callback_phase.is_some() || self.session.callback_termination().is_some();
        let mut wake = self.session.wake_state();
        if callback_blocked || !self.clock.is_playing() {
            wake = wake.without_timeline_wake();
        } else if let Some(loop_duration) = self.clock.loop_duration() {
            if self.session.has_replay_timeline_work() {
                wake = wake.with_additional_timeline(TimelineWakeState::Deadline(loop_duration));
            }
        }
        let plan = BrowserExecutionWakePlan::from_runtime(wake);
        let timer_after_milliseconds = if callback_blocked {
            None
        } else {
            match plan.cadence() {
                BrowserExecutionCadence::TimerAtSceneTime(deadline) => Some(
                    self.clock
                        .timer_delay_milliseconds(deadline, wall_time_ms, self.session.frame().time)
                        .map_err(|error| error.to_string())?,
                ),
                BrowserExecutionCadence::AnimationFrame | BrowserExecutionCadence::Idle => {
                    self.clock
                        .observe_wake_time(wall_time_ms)
                        .map_err(|error| error.to_string())?;
                    None
                }
            }
        };
        Ok(WasmExecutionWake::from_plan(plan, timer_after_milliseconds))
    }

    /// Begin the next browser wall-time interval after required host work.
    ///
    /// The endpoint calls this only after retrying the callback-bearing drive
    /// with its captured timestamp. The session's published time is therefore
    /// unchanged while this resets the derived wall-time conversion.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn reanchor_live_segment_wake(
        &mut self,
        wall_time_ms: f64,
    ) -> Result<WasmExecutionWake, String> {
        self.require_callback_progression_available()?;
        self.live_segment()?;
        self.live_wake_clock
            .reanchor(wall_time_ms, self.session.frame().time)
            .ok_or("invalid browser wall timestamp or authored continuation time")?;
        self.live_segment_wake(wall_time_ms)
    }

    /// Drive the current segment from one Rust-derived browser wall-time mapping.
    ///
    /// The session clamps this target to the segment boundary and owns all timeline work.
    /// If it reaches a required callback boundary, the returned phase must be committed
    /// before the host retries this operation with the same wall timestamp.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_drive_segment_from_wall_time(
        &mut self,
        wall_time_ms: f64,
    ) -> Result<WasmLiveSegmentDrive, String> {
        self.require_callback_progression_available()?;
        let segment = self.live_segment()?;
        let requested_time = self
            .live_wake_clock
            .scene_time_at(wall_time_ms)
            .ok_or("observe the live segment wake before driving it from wall time")?;
        self.live_drive_segment_to(segment, requested_time)
    }

    /// Drive the active continuation segment toward one externally supplied
    /// authored-time sample.
    ///
    /// The caller supplies an absolute sample from an external reference grid.
    /// Segment clamping, callback barriers, and runtime advancement remain owned
    /// by the execution session. Rejecting a backward sample before constructing
    /// the presentation clock or entering the session keeps the frame unchanged.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_drive_segment_to_authored_time(
        &mut self,
        requested_time: f64,
    ) -> Result<WasmLiveSegmentDrive, String> {
        self.require_callback_progression_available()?;
        let current = self.session.frame().time;
        if !requested_time.is_finite() || requested_time < current {
            return Err(format!(
                "external continuation sample requires time at or after {current}, got {requested_time}"
            ));
        }
        let segment = self.live_segment()?;
        self.live_drive_segment_to(segment, requested_time)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_drive_segment_to(
        &mut self,
        segment: noon::ExecutionSegment,
        requested_time: f64,
    ) -> Result<WasmLiveSegmentDrive, String> {
        let current_time = self.session.frame().time;
        let mut clock = self.live_clock_at(current_time, segment.end_time(), false)?;
        match self
            .session
            .advance_segment_to_callback_barrier(segment, requested_time)
            .map_err(|error| error.to_string())?
        {
            CallbackAdvance::Ready(_) => {
                clock.seek(self.session.frame().time).expect(
                    "published live time must remain within the preflighted segment extent",
                );
                self.clock = clock;
                Ok(WasmLiveSegmentDrive {
                    callback_phase_json: None,
                    reached_endpoint: self.session.frame().time >= segment.end_time(),
                })
            }
            CallbackAdvance::HostRequired {
                invocations,
                overlay,
            } => {
                let callback_phase_json = self.retain_callback_phase(invocations, overlay)?;
                Ok(WasmLiveSegmentDrive {
                    callback_phase_json: Some(callback_phase_json),
                    reached_endpoint: false,
                })
            }
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_advance_segment_to(&mut self, requested_time: f64) -> Result<bool, String> {
        self.reject_required_callback_segment()?;
        let segment = self.live_segment()?;
        let drive = self.live_drive_segment_to(segment, requested_time)?;
        debug_assert!(drive.callback_phase_json.is_none());
        // Preserve the established wrapper contract: this reports completion
        // reconciliation, not merely reaching an animation endpoint. The async
        // wall-time drive above deliberately exposes the latter so its owner can
        // call `completeLiveSegment` exactly once.
        Ok(self.session.segment_state(segment).is_complete())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_complete_segment(&mut self) -> Result<(), String> {
        let segment = self.live_segment()?;
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        let clock = self.live_clock_at(self.session.frame().time, segment.end_time(), false)?;
        noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .complete_segment(segment)
        .map_err(|error| error.to_string())?;
        self.clock = clock;
        self.live_segment = Some(LiveSegmentReceipt::Completed(segment));
        self.live_wake_clock = BrowserExecutionWakeClock::default();
        Ok(())
    }

    /// Evaluate scalar tracks through the one execution session, then align the
    /// hold presentation at that same absolute time for a later handoff.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_evaluate(&mut self, time: f64) -> Result<(), String> {
        let mut clock = self.clock.clone();
        clock.seek(time).map_err(|error| error.to_string())?;
        clock.pause();
        self.session
            .advance_to(time)
            .map_err(|error| error.to_string())?;
        self.clock = clock;
        Ok(())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_effective_signal(
        &self,
        tracker: &noon::ValueTracker,
    ) -> Result<f64, String> {
        match self
            .session
            .effective_signal_value(tracker.node_id())
            .ok_or("ValueTracker is not lowered into this execution session")?
        {
            ReactiveValue::Scalar(value) => Ok(f64::from(*value)),
            _ => Err("ValueTracker runtime signal is not scalar".into()),
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_set_signal(
        &mut self,
        tracker: &noon::ValueTracker,
        value: f64,
    ) -> Result<(), String> {
        if !value.is_finite() {
            return Err("ValueTracker value must be finite".into());
        }
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .set_value(tracker, value)
        .map_err(|error| error.to_string())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn set_native_state_input(
        &mut self,
        source: NativeStateSource,
        value: NativeInputValue,
    ) -> Result<(), String> {
        self.session
            .set_native_state_input(source, value)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn emit_native_event(&mut self, source: NativeEventSource) -> Result<(), String> {
        let sequence = self.next_native_event_sequence;
        let next = sequence
            .checked_add(1)
            .ok_or("native input event sequence exhausted")?;
        self.session
            .emit_native_event(NativeEventOccurrence::new(sequence, source))
            .map_err(|error| error.to_string())?;
        self.next_native_event_sequence = next;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn session_mut_for_test(&mut self) -> &mut ExecutionSession {
        &mut self.session
    }

    fn resource_bundle_for(session: &ExecutionSession) -> Result<Vec<u8>, String> {
        RetainedResourceBundle::capture(
            session
                .frame()
                .objects
                .iter()
                .filter_map(|object| object.text()),
            session.text_resources(),
            session.geometry_resources(),
            session.font_resources(),
        )
        .and_then(|bundle| bundle.encode_binary())
        .map_err(|error| error.to_string())
    }

    fn delta(&mut self, snapshot: bool) -> Result<Option<RetainedExecutionDeltaEnvelope>, String> {
        let camera = self.session.camera().map_err(|e| e.to_string())?;
        let changes = self.session.take_frame_changes();
        if snapshot || changes.is_all() || changes.is_structural() || !self.snapshot_sent {
            let delta = self
                .encoder
                .encode_snapshot_indices(
                    self.session.frame(),
                    camera,
                    (0..self.session.frame().objects.len()).filter(|index| {
                        self.session
                            .execution_slot_for_frame_index(*index)
                            .is_some()
                    }),
                )
                .map_err(|e| e.to_string())?;
            self.snapshot_sent = true;
            Ok(Some(delta))
        } else {
            self.encoder
                .encode_incremental(self.session.frame(), &changes, camera)
                .map_err(|e| e.to_string())
        }
    }

    fn encoded_delta(&mut self, snapshot: bool) -> Result<Option<String>, String> {
        self.delta(snapshot)?
            .map(|delta| serde_json::to_string(&delta).map_err(|e| e.to_string()))
            .transpose()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct CallbackNodeWire {
    slot: u32,
    generation: u32,
}

impl From<SemanticNodeId> for CallbackNodeWire {
    fn from(node: SemanticNodeId) -> Self {
        Self {
            slot: node.slot(),
            generation: node.generation(),
        }
    }
}

impl From<CallbackNodeWire> for SemanticNodeId {
    fn from(node: CallbackNodeWire) -> Self {
        Self::new(node.slot, node.generation)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct CallbackPublicationWire {
    scene_revision: String,
    execution_revision: String,
    frame_epoch: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct CallbackTokenWire {
    runtime: String,
    publication: CallbackPublicationWire,
    sequence: String,
}

impl From<CallbackPhaseToken> for CallbackTokenWire {
    fn from(token: CallbackPhaseToken) -> Self {
        let publication = token.publication();
        Self {
            runtime: token.runtime().get().to_string(),
            publication: CallbackPublicationWire {
                scene_revision: publication.scene_revision().get().to_string(),
                execution_revision: publication.execution_revision().get().to_string(),
                frame_epoch: publication.frame_epoch().get().to_string(),
            },
            sequence: token.sequence().get().to_string(),
        }
    }
}

impl TryFrom<CallbackTokenWire> for CallbackPhaseToken {
    type Error = String;

    fn try_from(token: CallbackTokenWire) -> Result<Self, Self::Error> {
        let parse = |label: &str, value: String| {
            value
                .parse::<u64>()
                .map_err(|error| format!("invalid callback {label} {value:?}: {error}"))
        };
        Ok(CallbackPhaseToken::new(
            RuntimeIdentity::new(parse("runtime identity", token.runtime)?),
            PublicationContext::new(
                SceneRevision::new(parse("scene revision", token.publication.scene_revision)?),
                ExecutionRevision::new(parse(
                    "execution revision",
                    token.publication.execution_revision,
                )?),
                FrameEpoch::new(parse("frame epoch", token.publication.frame_epoch)?),
            ),
            noon::CallbackSequence::new(parse("sequence", token.sequence)?),
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct CallbackPhaseObjectWire {
    node: CallbackNodeWire,
    transform: Transform2D,
    style: Style,
    appearance: f32,
    presence: bool,
    reveal: f32,
    morph: f32,
    bounds: Option<Rect>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct CallbackInvocationWire {
    callback_id: String,
    target: CallbackNodeWire,
    occurrence_index: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct CallbackPhaseWire {
    token: CallbackTokenWire,
    time: f64,
    delta_time: f64,
    objects: Vec<CallbackPhaseObjectWire>,
    invocations: Vec<CallbackInvocationWire>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct CallbackPhaseTokenEnvelope {
    token: CallbackTokenWire,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CallbackReadRequestWire {
    ScalarSignal { node: CallbackNodeWire },
    Object { node: CallbackNodeWire },
}

impl From<CallbackReadRequestWire> for CallbackReadRequest {
    fn from(value: CallbackReadRequestWire) -> Self {
        match value {
            CallbackReadRequestWire::ScalarSignal { node } => Self::ScalarSignal(node.into()),
            CallbackReadRequestWire::Object { node } => Self::Object(node.into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CallbackReadValueWire {
    Scalar { value: f32 },
    Object { object: CallbackPhaseObjectWire },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct CallbackTerminationWire {
    token: CallbackTokenWire,
    kind: &'static str,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct RendererObservationPublicationWire {
    delta: RetainedExecutionDeltaEnvelope,
    observation: RendererObservationRequest,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CallbackWriteWire {
    Transform {
        object: CallbackNodeWire,
        transform: Transform2D,
    },
    Style {
        object: CallbackNodeWire,
        style: Style,
    },
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct CallbackBatchWire {
    token: CallbackTokenWire,
    writes: Vec<CallbackWriteWire>,
}

#[cfg(any(target_arch = "wasm32", test))]
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct NativeStateInputWire {
    source: NativeStateSource,
    value: NativeInputValueWire,
}

#[cfg(any(target_arch = "wasm32", test))]
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum NativeInputValueWire {
    Scalar { value: f32 },
    Bool { value: bool },
    Vec2 { x: f32, y: f32 },
}

#[cfg(any(target_arch = "wasm32", test))]
impl From<NativeInputValueWire> for NativeInputValue {
    fn from(value: NativeInputValueWire) -> Self {
        match value {
            NativeInputValueWire::Scalar { value } => Self::Scalar(value),
            NativeInputValueWire::Bool { value } => Self::Bool(value),
            NativeInputValueWire::Vec2 { x, y } => Self::Vec2(Vec2::new(x, y)),
        }
    }
}

#[cfg(any(target_arch = "wasm32", test))]
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct NativeEventInputWire {
    source: NativeEventSource,
}

fn validate_callback_transform(transform: Transform2D) -> Result<(), String> {
    if [
        transform.translation.x,
        transform.translation.y,
        transform.rotation,
        transform.scale.x,
        transform.scale.y,
    ]
    .into_iter()
    .all(f32::is_finite)
    {
        Ok(())
    } else {
        Err("callback transform values must be finite".into())
    }
}

fn validate_callback_style(style: Style) -> Result<(), String> {
    let finite_color = |color: noon_core::Color| {
        [color.red, color.green, color.blue, color.alpha]
            .into_iter()
            .all(f32::is_finite)
    };
    if !style.stroke_width.is_finite()
        || !style.opacity.is_finite()
        || style.fill.is_some_and(|color| !finite_color(color))
        || style.stroke.is_some_and(|color| !finite_color(color))
    {
        return Err("callback style values must be finite".into());
    }
    Ok(())
}

fn decode_callback_batch(json: &str) -> Result<EffectivePropertyBatch, String> {
    let wire: CallbackBatchWire = serde_json::from_str(json)
        .map_err(|error| format!("invalid callback batch JSON: {error}"))?;
    let token = CallbackPhaseToken::try_from(wire.token)?;
    let writes = wire
        .writes
        .into_iter()
        .map(|write| match write {
            CallbackWriteWire::Transform { object, transform } => {
                validate_callback_transform(transform)?;
                Ok(EffectiveSemanticPropertyWrite::Transform {
                    object: object.into(),
                    transform,
                })
            }
            CallbackWriteWire::Style { object, style } => {
                validate_callback_style(style)?;
                Ok(EffectiveSemanticPropertyWrite::Style {
                    object: object.into(),
                    style,
                })
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(EffectivePropertyBatch::new(token, writes))
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
impl SemanticExecutionPlayer {
    fn callback_phase_json(
        overlay: &noon::CallbackPhaseOverlay,
        invocations: &[noon::RequiredCallbackInvocation],
    ) -> Result<String, String> {
        let phase = CallbackPhaseWire {
            token: overlay.token().into(),
            time: overlay.time(),
            delta_time: overlay.delta_time(),
            objects: overlay
                .objects()
                .map(|(node, properties)| CallbackPhaseObjectWire {
                    node: node.into(),
                    transform: properties.transform,
                    style: properties.style,
                    appearance: properties.appearance,
                    presence: properties.presence,
                    reveal: properties.reveal,
                    morph: properties.morph,
                    bounds: properties.bounds,
                })
                .collect(),
            invocations: invocations
                .iter()
                .copied()
                .map(|invocation| CallbackInvocationWire {
                    callback_id: invocation.callback_id().get().to_string(),
                    target: invocation.target().into(),
                    occurrence_index: invocation.occurrence_index(),
                })
                .collect(),
        };
        serde_json::to_string(&phase).map_err(|error| error.to_string())
    }

    fn advance_to_callback_phase(&mut self, time: f64) -> Result<Option<String>, String> {
        if self.pending_callback_phase.is_some() {
            return Err("a required callback phase is already pending".into());
        }
        match self
            .session
            .advance_to_callback_barrier(time)
            .map_err(|error| error.to_string())?
        {
            CallbackAdvance::Ready(_) => Ok(None),
            CallbackAdvance::HostRequired {
                invocations,
                overlay,
            } => self.retain_callback_phase(invocations, overlay).map(Some),
        }
    }

    fn callback_token_from_json(token_json: &str) -> Result<CallbackPhaseToken, String> {
        let token: CallbackTokenWire = serde_json::from_str(token_json)
            .map_err(|error| format!("invalid callback token JSON: {error}"))?;
        token.try_into()
    }

    fn phase_token_from_json(phase_json: &str) -> Result<CallbackPhaseToken, String> {
        let phase: CallbackPhaseTokenEnvelope = serde_json::from_str(phase_json)
            .map_err(|error| format!("invalid callback phase JSON: {error}"))?;
        phase.token.try_into()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = initialCallbackPhaseJson))]
    pub fn initial_callback_phase_json(&mut self) -> Result<Option<String>, String> {
        self.advance_to_callback_phase(self.session.frame().time)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = tickCallbackPhaseJson))]
    pub fn tick_callback_phase_json(
        &mut self,
        timestamp_ms: f64,
    ) -> Result<Option<String>, String> {
        let mut clock = self.clock.clone();
        let requested = clock
            .scene_time(timestamp_ms)
            .map_err(|error| error.to_string())?;
        let phase = self.advance_to_callback_phase(requested)?;
        if phase.is_none() {
            self.clock = clock;
        }
        Ok(phase)
    }

    /// Advance the canonical session to one exact forward authored-time barrier.
    ///
    /// Unlike browser-frame ticking, this takes the authored time directly. The
    /// execution session remains responsible for stopping at an intervening
    /// required callback activation; callers commit that one phase and may then
    /// request the remaining authored time again. No host playback cursor or
    /// timestamp conversion participates in this operation.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = advanceForwardToCallbackPhaseJson))]
    pub fn advance_forward_to_callback_phase_json(
        &mut self,
        time: f64,
    ) -> Result<Option<String>, String> {
        let current = self.session.frame().time;
        if !time.is_finite() || time < current {
            return Err(format!(
                "forward callback advance requires time at or after {current}, got {time}"
            ));
        }
        // Validate presentation anchoring before any fallible session advance.
        // Required-callback sessions use the non-looping clock, so this cannot
        // turn a forward diagnostic control into an implicit replay.
        let mut clock = self.clock.clone();
        clock.seek(time).map_err(|error| error.to_string())?;
        let phase = self.advance_to_callback_phase(time)?;
        if phase.is_none() {
            self.clock = clock;
        }
        Ok(phase)
    }

    /// Derive one browser wake directive for the active ordinary continuation segment.
    ///
    /// This is deliberately a typed WASM value rather than a host-authored duration.
    #[cfg(any(target_arch = "wasm32", test))]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = liveSegmentWake))]
    pub fn live_segment_wake_wasm(
        &mut self,
        wall_time_ms: f64,
    ) -> Result<WasmExecutionWake, String> {
        self.live_segment_wake(wall_time_ms)
    }

    /// Derive the next generic browser wake from the canonical runtime/session.
    #[cfg(any(target_arch = "wasm32", test))]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = executionWake))]
    pub fn execution_wake_wasm(&mut self, wall_time_ms: f64) -> Result<WasmExecutionWake, String> {
        self.execution_wake(wall_time_ms)
    }

    /// Reanchor the next browser interval after a required callback completes.
    #[cfg(any(target_arch = "wasm32", test))]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = reanchorLiveSegmentWake))]
    pub fn reanchor_live_segment_wake_wasm(
        &mut self,
        wall_time_ms: f64,
    ) -> Result<WasmExecutionWake, String> {
        self.reanchor_live_segment_wake(wall_time_ms)
    }

    /// Advance one active ordinary continuation segment from an anchored browser timestamp.
    ///
    /// A callback phase must be committed before this is retried with the same wall
    /// timestamp. `reachedEndpoint` means shared completion is now permitted but
    /// remains a separate operation so authored reconciliation cannot be skipped.
    #[cfg(any(target_arch = "wasm32", test))]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = driveLiveSegmentFromWallTime))]
    pub fn drive_live_segment_from_wall_time_wasm(
        &mut self,
        wall_time_ms: f64,
    ) -> Result<WasmLiveSegmentDrive, String> {
        self.live_drive_segment_from_wall_time(wall_time_ms)
    }

    /// Advance one active continuation segment toward an absolute authored-time
    /// sample without involving a browser clock.
    #[cfg(any(target_arch = "wasm32", test))]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = driveLiveSegmentToAuthoredTime))]
    pub fn drive_live_segment_to_authored_time_wasm(
        &mut self,
        requested_time: f64,
    ) -> Result<WasmLiveSegmentDrive, String> {
        self.live_drive_segment_to_authored_time(requested_time)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = completeLiveSegment))]
    pub fn complete_live_segment_wasm(&mut self) -> Result<(), String> {
        self.live_complete_segment()
    }

    /// Read one typed value from the exact pending callback phase without
    /// committing it. This is the real Python-worker boundary; direct Rust
    /// callbacks call the session API without JSON.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = requiredCallbackReadJson))]
    pub fn required_callback_read_json(
        &mut self,
        token_json: &str,
        request_json: &str,
    ) -> Result<String, String> {
        let token = Self::callback_token_from_json(token_json)?;
        self.pending_callback_phase
            .filter(|(pending, _)| *pending == token)
            .ok_or("callback read does not match the player pending phase")?;
        let request_wire: CallbackReadRequestWire = serde_json::from_str(request_json)
            .map_err(|error| format!("invalid callback read request JSON: {error}"))?;
        let requested_object = match &request_wire {
            CallbackReadRequestWire::Object { node } => Some(node.clone()),
            CallbackReadRequestWire::ScalarSignal { .. } => None,
        };
        let value = self
            .session
            .required_callback_read(token, request_wire.into())
            .map_err(|error| error.to_string())?;
        let wire = match value {
            CallbackReadValue::Scalar(value) => CallbackReadValueWire::Scalar { value },
            CallbackReadValue::Object(properties) => CallbackReadValueWire::Object {
                object: CallbackPhaseObjectWire {
                    node: requested_object.ok_or("scalar callback read returned an object")?,
                    transform: properties.transform,
                    style: properties.style,
                    appearance: properties.appearance,
                    presence: properties.presence,
                    reveal: properties.reveal,
                    morph: properties.morph,
                    bounds: properties.bounds,
                },
            },
        };
        serde_json::to_string(&wire).map_err(|error| error.to_string())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = commitCallbackPhaseJson))]
    pub fn commit_callback_phase_json(&mut self, batch_json: &str) -> Result<(), String> {
        let batch = decode_callback_batch(batch_json)?;
        let token = batch.token();
        let (_, time) = self
            .pending_callback_phase
            .filter(|(pending, _)| *pending == token)
            .ok_or("callback batch does not match the player pending phase")?;
        self.session
            .commit_required_callback_phase(batch)
            .map_err(|error| error.to_string())?;
        // The callback phase time is session-owned. Re-anchoring presentation
        // only after its commit avoids a host-side progression cursor.
        self.clock.seek(time).map_err(|error| error.to_string())?;
        self.pending_callback_phase = None;
        Ok(())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = failCallbackPhaseJson))]
    pub fn fail_callback_phase_json(&mut self, phase_json: &str) -> Result<(), String> {
        let token = Self::phase_token_from_json(phase_json)?;
        self.session
            .fail_required_callback_phase(token)
            .map_err(|error| error.to_string())?;
        self.pending_callback_phase = None;
        Ok(())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = interruptCallbackPhaseJson))]
    pub fn interrupt_callback_phase_json(&mut self, phase_json: &str) -> Result<(), String> {
        let token = Self::phase_token_from_json(phase_json)?;
        self.session
            .interrupt_required_callback_phase(token)
            .map_err(|error| error.to_string())?;
        self.pending_callback_phase = None;
        Ok(())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = callbackTerminationJson))]
    pub fn callback_termination_json(&self) -> Result<Option<String>, String> {
        self.session
            .callback_termination()
            .map(|termination| {
                let kind = match termination.kind() {
                    noon::CallbackTerminationKind::Failed => "failed",
                    noon::CallbackTerminationKind::Interrupted => "interrupted",
                };
                serde_json::to_string(&CallbackTerminationWire {
                    token: termination.token().into(),
                    kind,
                })
                .map_err(|error| error.to_string())
            })
            .transpose()
    }

    /// Decode one sampled native state update at the genuine worker control-port boundary.
    #[cfg(any(target_arch = "wasm32", test))]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = setNativeStateInputJson))]
    pub fn set_native_state_input_json(&mut self, json: &str) -> Result<(), String> {
        let input: NativeStateInputWire = serde_json::from_str(json)
            .map_err(|error| format!("invalid native state input JSON: {error}"))?;
        self.set_native_state_input(input.source, input.value.into())
    }

    /// Decode one ordered native event at the genuine worker control-port boundary.
    #[cfg(any(target_arch = "wasm32", test))]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = emitNativeEventJson))]
    pub fn emit_native_event_json(&mut self, json: &str) -> Result<(), String> {
        let input: NativeEventInputWire = serde_json::from_str(json)
            .map_err(|error| format!("invalid native event input JSON: {error}"))?;
        self.emit_native_event(input.source)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = drainDeltaJson))]
    pub fn drain_delta_json(&mut self) -> Result<Option<String>, String> {
        self.encoded_delta(false)
    }

    /// Drain one callback-published retained delta together with an exact,
    /// single-target renderer observation request for the same transport sequence.
    ///
    /// This opt-in method is the genuine execution-worker to render-worker boundary.
    /// It consumes the same canonical delta as `drainDeltaJson`; callers forward the
    /// two fields without deriving slot identity or runtime state in JavaScript.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = drainRendererObservationPublicationJson))]
    pub fn drain_renderer_observation_publication_json(
        &mut self,
        phase_json: &str,
        semantic_slot: u32,
        semantic_generation: u32,
    ) -> Result<String, String> {
        let token = Self::phase_token_from_json(phase_json)?;
        let target = SemanticNodeId::new(semantic_slot, semantic_generation);
        let committed = match self
            .session
            .committed_callback_renderer_observation(token, target)
        {
            noon::CallbackRendererObservationOutcome::Committed(observation) => observation,
            noon::CallbackRendererObservationOutcome::StaleCallback { .. } => {
                return Err("callback renderer observation token is stale".into());
            }
            noon::CallbackRendererObservationOutcome::StalePublication { .. } => {
                return Err("callback renderer observation publication is stale".into());
            }
            noon::CallbackRendererObservationOutcome::Absent { .. } => {
                return Err("callback renderer observation target is absent".into());
            }
        };
        let delta = self
            .delta(false)?
            .ok_or("callback commit produced no retained renderer publication")?;
        let observation = RendererObservationRequest::from_callback_publication(
            delta.session,
            delta.sequence,
            committed,
        );
        serde_json::to_string(&RendererObservationPublicationWire { delta, observation })
            .map_err(|error| error.to_string())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = initialDeltaJson))]
    pub fn initial_delta_json(&mut self) -> Result<String, String> {
        self.encoded_delta(true)?
            .ok_or_else(|| "initial snapshot missing".into())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = resourceBundleBytes))]
    pub fn resource_bundle_bytes(&self) -> Vec<u8> {
        self.resource_bundle.clone()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = tickDeltaJson))]
    pub fn tick_delta_json(&mut self, timestamp_ms: f64) -> Result<Option<String>, String> {
        let mut clock = self.clock.clone();
        let time = clock.scene_time(timestamp_ms).map_err(|e| e.to_string())?;
        self.session.evaluate(time).map_err(|e| e.to_string())?;
        self.clock = clock;
        self.encoded_delta(false)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = seekDeltaJson))]
    pub fn seek_delta_json(&mut self, time: f64) -> Result<Option<String>, String> {
        if self.session.has_required_callbacks() {
            return Err(
                "direct seek is unsupported for required callbacks; begin a new authoring run"
                    .into(),
            );
        }
        let mut clock = self.clock.clone();
        clock.seek(time).map_err(|e| e.to_string())?;
        self.session.seek(time).map_err(|e| e.to_string())?;
        self.clock = clock;
        self.encoded_delta(false)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = setLoopDuration))]
    pub fn set_loop_duration(&mut self, duration: f64) -> Result<(), String> {
        if self.session.has_required_callbacks() {
            return Err(
                "looping playback is unsupported for opaque required callbacks; begin a new authoring run"
                    .into(),
            );
        }
        #[cfg(any(target_arch = "wasm32", test))]
        self.validate_live_loop_duration(duration)?;
        self.clock
            .set_loop_duration(duration)
            .map_err(|error| error.to_string())
    }
    pub fn pause(&mut self) {
        self.clock.pause();
    }
    pub fn resume(&mut self) {
        self.clock.resume();
    }
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = isPlaying))]
    pub fn is_playing(&self) -> bool {
        self.clock.is_playing()
    }
    pub fn time(&self) -> f64 {
        self.session.frame().time
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RetainedExecutionFrameMirror, TransportObjectContent};
    use noon_core::{
        AnimationOptions, HostCallbackId, RateFunction, SemanticMutationTransaction,
        SemanticObjectProperty, SemanticObjectState, SemanticStore, StoredGeometry,
    };

    fn callback_batch_with_y_and_opacity(phase: &serde_json::Value) -> String {
        let row = &phase["objects"][0];
        let mut transform = row["transform"].clone();
        transform["translation"]["y"] = serde_json::json!(1.0);
        let mut style = row["style"].clone();
        style["opacity"] = serde_json::json!(0.5);
        serde_json::json!({
            "token": phase["token"].clone(),
            "writes": [
                {
                    "kind": "transform",
                    "object": row["node"].clone(),
                    "transform": transform,
                },
                {
                    "kind": "style",
                    "object": row["node"].clone(),
                    "style": style,
                },
            ],
        })
        .to_string()
    }

    #[test]
    fn membership_snapshot_omits_retired_rows_and_preserves_incremental_order() {
        let mut scene = noon::Scene::new();
        let anchor = scene.circle(0.5).unwrap();
        let toggled = scene.circle(1.0).unwrap();
        scene.add(&anchor).unwrap();
        scene.add(&toggled).unwrap();
        let session = scene.execution_session().unwrap();
        let mut player = SemanticExecutionPlayer::from_live_session(
            session,
            std::rc::Rc::clone(scene.store()),
            scene.root(),
            1.0,
            1,
        )
        .unwrap();
        let mut mirror = RetainedExecutionFrameMirror::default();
        mirror.apply(player.delta(true).unwrap().unwrap()).unwrap();
        player.live_remove(&toggled).unwrap();
        player.live_add(&toggled).unwrap();
        assert!(player.session.execution_slot_for_frame_index(1).is_none());
        let snapshot = player.delta(false).unwrap().unwrap();
        assert!(snapshot.snapshot);
        assert_eq!(snapshot.objects.len(), 2);
        assert_eq!(snapshot.objects[1].slot.slot, 2);
        assert_eq!(snapshot.objects[1].order, 1);
        mirror.apply(snapshot).unwrap();
        player.live_set_translation(&toggled, 2.0, -1.0).unwrap();
        let delta = player.delta(false).unwrap().unwrap();
        assert!(!delta.snapshot);
        assert_eq!(delta.objects.len(), 1);
        assert_eq!(delta.objects[0].order, 1);
        mirror.apply(delta).unwrap();
        assert_eq!(
            mirror.frame().unwrap().objects[1].transform.translation,
            noon_core::Vec2::new(2.0, -1.0)
        );
    }

    fn animated_player() -> SemanticExecutionPlayer {
        let mut scene = noon::Scene::new();
        let mut circle = scene.circle(1.0).unwrap();
        circle.shift(2.0, -1.0).unwrap();
        circle.scale(1.5, 0.5).unwrap();
        circle.set_fill(0.0, 0.0, 1.0, 0.4).unwrap();
        circle.set_stroke_join("miter").unwrap();
        circle.set_stroke_cap("butt").unwrap();
        scene.add(&circle).unwrap();
        let static_circle = scene.circle(0.25).unwrap();
        scene.add(&static_circle).unwrap();
        let mut target = circle.target_editor().unwrap();
        target.shift(4.0, 0.0).unwrap();
        let animation = scene
            .store()
            .borrow_mut()
            .insert_semantic_transform_animation(
                circle.node_id(),
                target.node_id(),
                AnimationOptions::new(),
            )
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        session
            .activate_animation(
                &scene.store().borrow(),
                animation,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        SemanticExecutionPlayer::from_session(session, 2.0, 42).unwrap()
    }

    #[test]
    fn native_input_codec_reaches_one_session_and_keeps_event_occurrences_ordered() {
        let mut store = SemanticStore::new();
        let opacity = store.insert_semantic_input_signal(0.25_f64).unwrap();
        let clicks = store.insert_semantic_input_signal(0.0_f64).unwrap();
        store
            .bind_semantic_native_state_input(
                opacity,
                NativeStateSource::Control {
                    name: "opacity".to_owned(),
                },
            )
            .unwrap();
        store
            .bind_semantic_native_event_input(clicks, NativeEventSource::PointerDown { button: 0 })
            .unwrap();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(opacity, object, SemanticObjectProperty::ObjectOpacity)
            .unwrap();
        store
            .bind_semantic_signal(clicks, object, SemanticObjectProperty::RotationZ)
            .unwrap();
        let session = ExecutionSession::from_semantic_store(&store).unwrap();
        let mut player = SemanticExecutionPlayer::from_session(session, 2.0, 61).unwrap();

        player
            .set_native_state_input_json(
                r#"{"source":{"kind":"control","name":"opacity"},"value":{"kind":"scalar","value":0.75}}"#,
            )
            .unwrap();
        let event = r#"{"source":{"kind":"pointer_down","button":0}}"#;
        player.emit_native_event_json(event).unwrap();
        player.emit_native_event_json(event).unwrap();

        assert_eq!(player.session.frame().objects[0].style.opacity, 0.75);
        assert_eq!(player.session.frame().objects[0].transform.rotation, 2.0);
        assert_eq!(player.next_native_event_sequence, 2);
    }

    #[test]
    fn rejected_native_input_keeps_frame_and_player_event_sequence_unchanged() {
        let mut store = SemanticStore::new();
        let clicks = store.insert_semantic_input_signal(0.0_f64).unwrap();
        store
            .bind_semantic_native_event_input(clicks, NativeEventSource::PointerDown { button: 0 })
            .unwrap();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(clicks, object, SemanticObjectProperty::RotationZ)
            .unwrap();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_updater(object, HostCallbackId::new(9), 0.0, None);
        transaction.apply(&mut store).unwrap();
        let session = ExecutionSession::from_semantic_store(&store).unwrap();
        let mut player = SemanticExecutionPlayer::from_session(session, 2.0, 62).unwrap();
        let frame = player.session.frame().clone();

        let error = player
            .emit_native_event_json(r#"{"source":{"kind":"pointer_down","button":0}}"#)
            .unwrap_err();

        assert!(error.contains("unsupported while required callbacks are configured"));
        assert_eq!(player.session.frame(), &frame);
        assert_eq!(player.next_native_event_sequence, 0);
    }

    #[test]
    fn live_segment_wake_drives_one_leased_session_without_a_host_timeline() {
        let mut scene = noon::Scene::new();
        let circle = scene.circle(0.4).unwrap();
        scene.add(&circle).unwrap();
        let mut target = circle.target_editor().unwrap();
        target.set_translation(2.0, -1.0).unwrap();
        let session = scene.execution_session().unwrap();
        let mut player = SemanticExecutionPlayer::from_live_session(
            session,
            std::rc::Rc::clone(scene.store()),
            scene.root(),
            2.0,
            63,
        )
        .unwrap();

        let endpoint = player
            .live_declare_and_activate_transform_to(
                &circle,
                &target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        assert_eq!(endpoint, 2.0);
        assert_eq!(
            player.time(),
            0.0,
            "begin must not fast-forward the segment"
        );

        let wake = player.live_segment_wake(1_000.0).unwrap();
        assert_eq!(wake.cadence(), "animation_frame");
        assert_eq!(wake.timer_after_milliseconds(), None);
        assert!(!player
            .live_drive_segment_from_wall_time(2_000.0)
            .unwrap()
            .reached_endpoint());
        assert_eq!(
            player
                .live_effective(&circle)
                .unwrap()
                .transform
                .translation,
            Vec2::new(1.0, -0.5)
        );

        assert!(player
            .live_drive_segment_from_wall_time(4_000.0)
            .unwrap()
            .reached_endpoint());
        assert_eq!(player.time(), endpoint);
        player.live_complete_segment().unwrap();
        assert_eq!(
            player
                .live_effective(&circle)
                .unwrap()
                .transform
                .translation,
            Vec2::new(2.0, -1.0)
        );

        assert_eq!(player.live_wait(1.0).unwrap(), 3.0);
        assert_eq!(player.time(), 2.0, "beginning a wait must not advance it");
        let wait_wake = player.live_segment_wake(5_000.0).unwrap();
        assert_eq!(wait_wake.cadence(), "timer");
        assert_eq!(wait_wake.timer_after_milliseconds(), Some(1_000.0));
        assert!(player
            .live_drive_segment_from_wall_time(6_000.0)
            .unwrap()
            .reached_endpoint());
        player.live_complete_segment().unwrap();
        assert_eq!(player.time(), 3.0);
    }

    #[test]
    fn external_authored_samples_are_monotonic_and_reuse_the_live_player() {
        let mut scene = noon::Scene::new();
        let circle = scene.circle(0.4).unwrap();
        scene.add(&circle).unwrap();
        let mut target = circle.target_editor().unwrap();
        target.set_translation(2.0, 0.0).unwrap();
        let session = scene.execution_session().unwrap();
        let mut player = SemanticExecutionPlayer::from_live_session(
            session,
            std::rc::Rc::clone(scene.store()),
            scene.root(),
            4.0,
            67,
        )
        .unwrap();

        player
            .live_declare_and_activate_transform_to(
                &circle,
                &target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        let midpoint = player
            .live_drive_segment_to_authored_time(1.25)
            .unwrap();
        assert!(midpoint.callback_phase_json().is_none());
        assert!(!midpoint.reached_endpoint());
        assert_eq!(player.time(), 1.25);

        let frame = player.session.frame().clone();
        assert!(player
            .live_drive_segment_to_authored_time(1.0)
            .unwrap_err()
            .contains("time at or after 1.25"));
        assert_eq!(player.session.frame(), &frame);

        assert!(player
            .live_drive_segment_to_authored_time(3.0)
            .unwrap()
            .reached_endpoint());
        assert_eq!(player.time(), 2.0, "Rust clamps the external sample at the segment boundary");
        player.live_complete_segment().unwrap();
        assert!(player.live_drive_segment_to_authored_time(3.0).is_err());

        player.live_wait(1.0).unwrap();
        assert!(player
            .live_drive_segment_to_authored_time(3.0)
            .unwrap()
            .reached_endpoint());
        assert_eq!(player.time(), 3.0);
    }

    #[test]
    fn callback_segment_drive_pins_time_until_exact_phase_commit() {
        let mut scene = noon::Scene::new();
        let circle = scene.circle(0.4).unwrap();
        scene.add(&circle).unwrap();
        let mut target = circle.target_editor().unwrap();
        target.set_translation(2.0, 0.0).unwrap();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_updater(circle.node_id(), HostCallbackId::new(7), 0.0, None);
        transaction.add_updater(circle.node_id(), HostCallbackId::new(8), 0.0, None);
        transaction.apply(&mut scene.store().borrow_mut()).unwrap();
        let session = scene.execution_session().unwrap();
        let mut player = SemanticExecutionPlayer::from_live_session(
            session,
            std::rc::Rc::clone(scene.store()),
            scene.root(),
            1.0,
            64,
        )
        .unwrap();
        player
            .live_declare_and_activate_transform_to(
                &circle,
                &target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();

        assert_eq!(
            player.live_segment_wake(1_000.0).unwrap().cadence(),
            "animation_frame"
        );
        let initial = player.live_drive_segment_from_wall_time(1_000.0).unwrap();
        assert!(!initial.reached_endpoint());
        let initial_phase: serde_json::Value =
            serde_json::from_str(&initial.callback_phase_json().unwrap()).unwrap();
        assert_eq!(initial_phase["time"], serde_json::json!(0.0));
        assert_eq!(
            initial_phase["invocations"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| entry["callback_id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["7", "8"]
        );
        assert_eq!(player.time(), 0.0);
        assert!(player.live_drive_segment_from_wall_time(1_000.0).is_err());

        player
            .commit_callback_phase_json(&callback_batch_with_y_and_opacity(&initial_phase))
            .unwrap();
        let ready = player.live_drive_segment_from_wall_time(1_000.0).unwrap();
        assert!(ready.callback_phase_json().is_none());
        assert!(!ready.reached_endpoint());
        assert_eq!(player.time(), 0.0);

        // A mid-segment sample stays pinned while its host callback is
        // outstanding. Retrying the same sample reaches exactly 0.5 rather
        // than charging callback latency into authored time.
        let midpoint = player.live_drive_segment_from_wall_time(1_500.0).unwrap();
        let midpoint_phase: serde_json::Value =
            serde_json::from_str(&midpoint.callback_phase_json().unwrap()).unwrap();
        assert_eq!(midpoint_phase["time"], serde_json::json!(0.5));
        assert_eq!(player.time(), 0.0);
        player
            .commit_callback_phase_json(&callback_batch_with_y_and_opacity(&midpoint_phase))
            .unwrap();
        let ready = player.live_drive_segment_from_wall_time(1_500.0).unwrap();
        assert!(ready.callback_phase_json().is_none());
        assert!(!ready.reached_endpoint());
        assert_eq!(player.time(), 0.5);

        // Simulate an opaque callback host taking 7.5 seconds after the
        // midpoint commit. Reanchoring at actual completion keeps the next
        // 16 ms wake to exactly 16 ms of authored progress.
        let wake = player.reanchor_live_segment_wake(9_000.0).unwrap();
        assert_eq!(wake.cadence(), "animation_frame");
        let after_slow_callback = player.live_drive_segment_from_wall_time(9_016.0).unwrap();
        let after_slow_callback_phase: serde_json::Value =
            serde_json::from_str(&after_slow_callback.callback_phase_json().unwrap()).unwrap();
        assert!((after_slow_callback_phase["time"].as_f64().unwrap() - 0.516).abs() < 1.0e-9);
        assert_eq!(player.time(), 0.5);
        player
            .commit_callback_phase_json(&callback_batch_with_y_and_opacity(
                &after_slow_callback_phase,
            ))
            .unwrap();
        let ready = player.live_drive_segment_from_wall_time(9_016.0).unwrap();
        assert!(ready.callback_phase_json().is_none());
        assert!(!ready.reached_endpoint());
        assert!((player.time() - 0.516).abs() < 1.0e-9);
        player.reanchor_live_segment_wake(12_000.0).unwrap();

        // The endpoint follows the same phase/commit protocol before reporting
        // readiness for completion and source resumption.
        let endpoint = player.live_drive_segment_from_wall_time(12_484.0).unwrap();
        let endpoint_phase: serde_json::Value =
            serde_json::from_str(&endpoint.callback_phase_json().unwrap()).unwrap();
        assert_eq!(endpoint_phase["time"], serde_json::json!(1.0));
        assert!((player.time() - 0.516).abs() < 1.0e-9);
        player
            .commit_callback_phase_json(&callback_batch_with_y_and_opacity(&endpoint_phase))
            .unwrap();
        let ready = player.live_drive_segment_from_wall_time(12_484.0).unwrap();
        assert!(ready.callback_phase_json().is_none());
        assert!(ready.reached_endpoint());
        assert_eq!(player.time(), 1.0);
        player.live_complete_segment().unwrap();
        assert_eq!(
            player.session.frame().objects[0].transform.translation.x,
            2.0
        );
        assert_eq!(
            player.session.frame().objects[0].transform.translation.y,
            1.0
        );
        assert_eq!(player.session.frame().objects[0].style.opacity, 0.5);
    }

    #[test]
    fn shared_authoring_to_transport_preserves_style_and_emits_only_dirty_rows() {
        let mut player = animated_player();
        let mut mirror = RetainedExecutionFrameMirror::default();
        let initial: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&player.initial_delta_json().unwrap()).unwrap();
        assert_eq!(
            (initial.session, initial.sequence, initial.snapshot),
            (42, 0, true)
        );
        assert_eq!(initial.objects.len(), 2);
        assert_eq!(
            initial.objects[0].transform.translation,
            noon_core::Vec2::new(2.0, -1.0)
        );
        assert_eq!(
            initial.objects[0].style.stroke_join,
            noon_core::StrokeJoin::Miter
        );
        assert_eq!(
            initial.objects[0].style.stroke_cap,
            noon_core::StrokeCap::Butt
        );
        assert_eq!(initial.objects[0].style.fill.unwrap().alpha, 0.4);
        mirror.apply(initial).unwrap();
        player.tick_delta_json(0.0).unwrap();
        let halfway: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&player.tick_delta_json(500.0).unwrap().unwrap()).unwrap();
        assert!(!halfway.snapshot);
        assert_eq!(halfway.objects.len(), 1);
        assert_eq!(halfway.objects[0].transform.translation.x, 4.0);
        mirror.apply(halfway).unwrap();
        assert_eq!(
            mirror.frame().unwrap().objects[0].transform.translation.x,
            4.0
        );
        let end: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&player.tick_delta_json(1000.0).unwrap().unwrap()).unwrap();
        assert_eq!(end.objects[0].transform.translation.x, 6.0);
    }

    #[test]
    fn callback_phase_wire_pins_runtime_publication_and_orders_effective_writes() {
        let mut scene = noon::Scene::new();
        let source = scene.circle(1.0).unwrap();
        let drift = scene.circle(0.25).unwrap();
        scene.add(&source).unwrap();
        scene.add(&drift).unwrap();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_updater(source.node_id(), HostCallbackId::new(9), 0.0, None);
        transaction.add_updater(source.node_id(), HostCallbackId::new(4), 0.0, None);
        transaction.add_updater(drift.node_id(), HostCallbackId::new(2), 0.0, None);
        transaction.apply(&mut scene.store().borrow_mut()).unwrap();
        let session = scene.execution_session().unwrap();
        let mut player = SemanticExecutionPlayer::from_live_session(
            session,
            std::rc::Rc::clone(scene.store()),
            scene.root(),
            2.0,
            12,
        )
        .unwrap();

        let phase: serde_json::Value = serde_json::from_str(
            &player
                .initial_callback_phase_json()
                .unwrap()
                .expect("time-zero callbacks require one phase"),
        )
        .unwrap();
        assert_eq!(phase["objects"].as_array().unwrap().len(), 2);
        assert_eq!(
            phase["invocations"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| entry["callback_id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["9", "4", "2"]
        );
        for field in ["runtime", "sequence"] {
            assert!(phase["token"][field].is_string());
        }
        for field in ["scene_revision", "execution_revision", "frame_epoch"] {
            assert!(phase["token"]["publication"][field].is_string());
        }
        let pending_wake = player.execution_wake(1_000.0).unwrap();
        assert_eq!(pending_wake.cadence(), "idle");
        assert_eq!(pending_wake.timer_after_milliseconds(), None);

        let source_row = phase["objects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| {
                entry["node"]["slot"].as_u64() == Some(u64::from(source.node_id().slot()))
            })
            .unwrap();
        let mut source_transform = source_row["transform"].clone();
        source_transform["translation"]["y"] = serde_json::json!(1.0);
        let mut source_style = source_row["style"].clone();
        source_style["opacity"] = serde_json::json!(0.5);
        let batch = serde_json::json!({
            "token": phase["token"].clone(),
            "writes": [
                {
                    "kind": "transform",
                    "object": source_row["node"].clone(),
                    "transform": source_transform,
                },
                {
                    "kind": "style",
                    "object": source_row["node"].clone(),
                    "style": source_style,
                },
            ],
        });
        player
            .commit_callback_phase_json(&batch.to_string())
            .unwrap();
        let committed_wake = player.execution_wake(9_000.0).unwrap();
        assert_eq!(committed_wake.cadence(), "animation_frame");
        assert_eq!(
            player.session.frame().objects[0].transform.translation.y,
            1.0
        );
        assert_eq!(player.session.frame().objects[0].style.opacity, 0.5);
        let publication: serde_json::Value = serde_json::from_str(
            &player
                .drain_renderer_observation_publication_json(
                    &phase.to_string(),
                    source.node_id().slot(),
                    source.node_id().generation(),
                )
                .unwrap(),
        )
        .unwrap();
        assert_eq!(publication["delta"]["session"], 12);
        assert_eq!(publication["delta"]["sequence"], 0);
        assert_eq!(publication["observation"]["publication"]["session"], 12);
        assert_eq!(publication["observation"]["publication"]["sequence"], 0);
        assert_eq!(
            publication["observation"]["slot"],
            publication["delta"]["objects"][0]["slot"]
        );
        assert_eq!(
            publication["observation"]["committed"]["transform"]["translation"]["y"],
            1.0
        );
        assert_eq!(publication["observation"]["committed"]["dirty"], "all");
    }

    #[test]
    fn callback_sparse_read_accepts_the_raw_pending_token_and_rejects_a_foreign_one() {
        let mut scene = noon::Scene::new();
        let circle = scene.circle(1.0).unwrap();
        scene.add(&circle).unwrap();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_updater(circle.node_id(), HostCallbackId::new(1), 0.0, None);
        transaction.apply(&mut scene.store().borrow_mut()).unwrap();
        let session = scene.execution_session().unwrap();
        let mut player = SemanticExecutionPlayer::from_live_session(
            session,
            std::rc::Rc::clone(scene.store()),
            scene.root(),
            1.0,
            13,
        )
        .unwrap();

        let phase: serde_json::Value = serde_json::from_str(
            &player
                .initial_callback_phase_json()
                .unwrap()
                .expect("time-zero callbacks require one phase"),
        )
        .unwrap();
        let raw_token = phase["token"].to_string();
        let object_request = serde_json::json!({
            "kind": "object",
            "node": phase["objects"][0]["node"].clone(),
        })
        .to_string();

        let response: serde_json::Value = serde_json::from_str(
            &player
                .required_callback_read_json(&raw_token, &object_request)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(response["kind"], "object");
        assert_eq!(response["object"]["node"], phase["objects"][0]["node"]);
        assert!(player.pending_callback_phase.is_some());

        let mut foreign_token = phase["token"].clone();
        foreign_token["sequence"] = serde_json::json!("999");
        assert!(player
            .required_callback_read_json(&foreign_token.to_string(), &object_request)
            .is_err());
        assert!(player.pending_callback_phase.is_some());
    }

    #[test]
    fn interrupted_callback_phase_stays_terminal_after_player_recovery() {
        let mut scene = noon::Scene::new();
        let circle = scene.circle(1.0).unwrap();
        scene.add(&circle).unwrap();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_updater(circle.node_id(), HostCallbackId::new(1), 0.0, None);
        transaction.apply(&mut scene.store().borrow_mut()).unwrap();
        let session = scene.execution_session().unwrap();
        let mut player = SemanticExecutionPlayer::from_live_session(
            session,
            std::rc::Rc::clone(scene.store()),
            scene.root(),
            2.0,
            13,
        )
        .unwrap();
        let phase = player.initial_callback_phase_json().unwrap().unwrap();
        player.interrupt_callback_phase_json(&phase).unwrap();
        let termination: serde_json::Value =
            serde_json::from_str(&player.callback_termination_json().unwrap().unwrap()).unwrap();
        assert_eq!(termination["kind"], "interrupted");
        assert!(player.tick_callback_phase_json(16.0).is_err());
    }

    #[test]
    fn forward_callback_control_uses_authored_time_without_a_browser_timestamp() {
        let mut scene = noon::Scene::new();
        let circle = scene.circle(1.0).unwrap();
        scene.add(&circle).unwrap();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_updater(circle.node_id(), HostCallbackId::new(1), 0.0, None);
        transaction.apply(&mut scene.store().borrow_mut()).unwrap();
        let session = scene.execution_session().unwrap();
        let mut player = SemanticExecutionPlayer::from_live_session(
            session,
            std::rc::Rc::clone(scene.store()),
            scene.root(),
            2.0,
            14,
        )
        .unwrap();

        let initial: serde_json::Value = serde_json::from_str(
            &player
                .initial_callback_phase_json()
                .unwrap()
                .expect("time-zero callback phase"),
        )
        .unwrap();
        player
            .commit_callback_phase_json(
                &serde_json::json!({ "token": initial["token"].clone(), "writes": [] }).to_string(),
            )
            .unwrap();

        let phase: serde_json::Value = serde_json::from_str(
            &player
                .advance_forward_to_callback_phase_json(1.0)
                .unwrap()
                .expect("active callback requires one authored-time phase"),
        )
        .unwrap();
        assert_eq!(phase["time"], serde_json::json!(1.0));
        player
            .commit_callback_phase_json(
                &serde_json::json!({ "token": phase["token"].clone(), "writes": [] }).to_string(),
            )
            .unwrap();
        assert_eq!(player.time(), 1.0);
        assert!(player.advance_forward_to_callback_phase_json(0.5).is_err());
    }

    #[test]
    fn invalid_controls_leave_the_clock_frame_and_delta_sequence_unchanged() {
        let mut player = animated_player();
        player.initial_delta_json().unwrap();
        assert!(player.seek_delta_json(f64::NAN).is_err());
        assert!(player.tick_delta_json(f64::INFINITY).is_err());
        assert_eq!(player.time(), 0.0);
        let delta: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&player.seek_delta_json(0.5).unwrap().unwrap()).unwrap();
        assert_eq!(delta.sequence, 1);
        assert_eq!(delta.objects[0].transform.translation.x, 4.0);
    }

    #[test]
    fn unchanged_static_ticks_do_not_retransmit_geometry() {
        let mut scene = noon::Scene::new();
        scene.add(&scene.circle(1.0).unwrap()).unwrap();
        let mut player =
            SemanticExecutionPlayer::from_session(scene.execution_session().unwrap(), 2.0, 1)
                .unwrap();
        player.initial_delta_json().unwrap();
        assert!(player.tick_delta_json(0.0).unwrap().is_none());
        assert!(player.tick_delta_json(500.0).unwrap().is_none());
        assert_eq!(player.time(), 0.5);
    }

    #[test]
    fn generic_wake_settles_a_playing_static_session() {
        let mut scene = noon::Scene::new();
        scene.add(&scene.circle(1.0).unwrap()).unwrap();
        let mut player =
            SemanticExecutionPlayer::from_session(scene.execution_session().unwrap(), 2.0, 1)
                .unwrap();
        player.initial_delta_json().unwrap();

        let wake = player.execution_wake(8_000.0).unwrap();
        assert!(player.is_playing());
        assert_eq!(wake.cadence(), "idle");
        assert_eq!(wake.timer_after_milliseconds(), None);
        assert!(!wake.present_now());
        assert!(!player.session.has_replay_timeline_work());
    }

    #[test]
    fn generic_wake_uses_runtime_activity_then_the_real_loop_boundary() {
        let mut player = animated_player();
        player.initial_delta_json().unwrap();
        assert!(player.session.has_replay_timeline_work());

        let active = player.execution_wake(10_000.0).unwrap();
        assert_eq!(active.cadence(), "animation_frame");
        assert!(player.tick_callback_phase_json(11_000.0).unwrap().is_none());
        player.drain_delta_json().unwrap();

        let settled = player.execution_wake(11_250.0).unwrap();
        assert_eq!(settled.cadence(), "timer");
        assert_eq!(settled.timer_after_milliseconds(), Some(750.0));

        let overdue = player.execution_wake(12_100.0).unwrap();
        assert_eq!(overdue.cadence(), "timer");
        assert_eq!(overdue.timer_after_milliseconds(), Some(0.0));
        assert!(player.tick_callback_phase_json(12_100.0).unwrap().is_none());
        let replaying = player.execution_wake(12_100.0).unwrap();
        assert_eq!(replaying.cadence(), "animation_frame");
    }

    #[test]
    fn generic_wake_suppresses_timeline_cadence_while_paused() {
        let mut player = animated_player();
        player.pause();
        let paused = player.execution_wake(1_000.0).unwrap();
        assert_eq!(paused.cadence(), "idle");

        player.resume();
        let resumed = player.execution_wake(9_000.0).unwrap();
        assert_eq!(resumed.cadence(), "animation_frame");
        assert!(player.tick_callback_phase_json(9_250.0).unwrap().is_none());
        assert_eq!(player.time(), 0.25);
    }

    #[test]
    fn opaque_callback_history_is_explicitly_non_looping() {
        let mut scene = noon::Scene::new();
        let circle = scene.circle(1.0).unwrap();
        scene.add(&circle).unwrap();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_updater(circle.node_id(), HostCallbackId::new(1), 0.0, None);
        transaction.remove_updater(circle.node_id(), HostCallbackId::new(1), 0.5);
        transaction.apply(&mut scene.store().borrow_mut()).unwrap();
        let mut player =
            SemanticExecutionPlayer::from_session(scene.execution_session().unwrap(), 2.0, 7)
                .unwrap();

        assert_eq!(player.clock.loop_duration(), None);
        assert!(!player.session.has_replay_timeline_work());
        let before = player.clock.clone();
        assert!(player.set_loop_duration(2.0).is_err());
        assert_eq!(player.clock, before);
    }

    #[test]
    fn shared_session_text_uses_the_mixed_resource_boundary() {
        let mut scene = noon::Scene::new();
        let label = scene
            .text(noon::Text::new("Noon").with_font_size(48.0))
            .unwrap();
        scene.add(&label).unwrap();
        let mut player =
            SemanticExecutionPlayer::from_session(scene.execution_session().unwrap(), 2.0, 8)
                .unwrap();

        let bundle =
            RetainedResourceBundle::decode_binary(&player.resource_bundle_bytes()).unwrap();
        assert_eq!(bundle.text_count(), 1);
        let initial: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&player.initial_delta_json().unwrap()).unwrap();
        assert!(matches!(
            initial.objects[0].content,
            TransportObjectContent::Text { .. }
        ));
    }
}
