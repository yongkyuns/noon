//! Transport adapter for an already-lowered semantic session; never parses authoring JSON.
#[cfg(target_arch = "wasm32")]
mod brace;
#[cfg(any(target_arch = "wasm32", test))]
mod coordinates;
#[cfg(any(target_arch = "wasm32", test))]
mod graph;
#[cfg(target_arch = "wasm32")]
mod matrix;
#[cfg(target_arch = "wasm32")]
mod numbers;
#[cfg(any(target_arch = "wasm32", test))]
mod pointer_input;
mod provisional_geometry;
#[cfg(any(target_arch = "wasm32", test))]
mod sample_space;
#[cfg(target_arch = "wasm32")]
mod table;
#[cfg(any(target_arch = "wasm32", test))]
mod zoomed_view;
use crate::authoring_error::AuthoringFailure;
#[cfg(any(target_arch = "wasm32", test))]
use crate::browser_pointer_input::BrowserPointerBinding;
use noon::integration::{
    CallbackAdvance, CallbackPhaseToken, EffectivePropertyBatch, EffectiveSemanticPropertyWrite,
    RuntimeIdentity,
};
#[cfg(any(target_arch = "wasm32", test))]
use noon::integration::{CallbackReadRequest, CallbackReadValue, TimelineWakeState};
use noon::ExecutionSession;
#[cfg(any(target_arch = "wasm32", test))]
use noon_core::{
    stage_prepared_semantic_scene_membership, NativeEventOccurrence, NativeEventSource,
    NativeInputValue, NativeStateSource, ReactiveValue, SemanticMutationTransaction, Vec2,
};
use noon_core::{
    ExecutionRevision, FrameEpoch, GeometryRef, ObjectContentRef, PublicationContext, Rect,
    SceneRevision, SemanticNodeId, Style, Transform2D,
};
use serde::{Deserialize, Serialize};

#[cfg(test)]
use crate::RetainedExecutionDeltaEnvelope;
#[cfg(any(target_arch = "wasm32", test))]
use crate::{
    BrowserExecutionCadence, BrowserExecutionWakeClock, BrowserExecutionWakePlan, BrowserHostWake,
};
use crate::{
    PlaybackClock, RendererObservationRequest, RetainedFamilyExecutionDeltaEncoder,
    RetainedFamilyExecutionDeltaEnvelope, RetainedResourceBundle,
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
    encoder: RetainedFamilyExecutionDeltaEncoder,
    /// Immutable text/font/vector dependencies transferred once at the genuine
    /// authoring-worker to render-worker boundary.
    resource_bundle: Vec<u8>,
    snapshot_sent: bool,
    /// Last emitted presentation only. The session remains selection authority;
    /// renderer acknowledgement and retry belong to the existing transport.
    last_sent_selection_overlay: Option<noon::integration::PointerSelectionPresentation>,
    /// Exact phase metadata needed only to re-anchor presentation after the
    /// session atomically commits its own pending callback phase. The session
    /// remains the sole owner of callback progression and termination.
    pending_callback_phase: Option<(CallbackPhaseToken, f64)>,
    /// One pinned callback may stage several authored membership operations.
    /// The transaction remains unpublished until the existing callback commit
    /// co-publishes it with the effective property batch.
    #[cfg(any(target_arch = "wasm32", test))]
    callback_membership_transaction: Option<CallbackMembershipCollector>,
    /// Durable identities resolved by the one callback publication. A
    /// provisional wrapper may redeem its own phase-local token exactly once
    /// after that publication; neither pending nor foreign tokens can be
    /// reconstructed as handles.
    #[cfg(any(target_arch = "wasm32", test))]
    committed_callback_provisionals: Option<CommittedCallbackProvisionals>,
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
    /// Browser control-port pointer binding. The DOM adapter supplies CSS-pixel
    /// surface coordinates and a monotonically changing view revision; the
    /// shared session remains the admission/publication authority.
    #[cfg(any(target_arch = "wasm32", test))]
    browser_pointer_binding: Option<BrowserPointerBinding>,
    #[cfg(any(target_arch = "wasm32", test))]
    worker_pointer_presentation: crate::worker_pointer_presentation::WorkerPointerPresentation,
}

/// A host continuation receipt retains its endpoint after completion for renderer
/// recovery/scrubbing, while only Pending permits one begin/drive/complete lease.
#[cfg(any(target_arch = "wasm32", test))]
#[derive(Clone, Copy)]
enum LiveSegmentReceipt {
    Pending(noon::ExecutionSegment),
    Completed(noon::ExecutionSegment),
}

/// Unpublished authored structure accumulated for one exact callback token.
/// It has no store borrow, runtime, or host-side membership mirror.
#[cfg(any(target_arch = "wasm32", test))]
struct CallbackMembershipCollector {
    token: CallbackPhaseToken,
    transaction: SemanticMutationTransaction,
    /// Transaction-local object names returned to the callback adapter. These
    /// are phase-scoped capabilities, never semantic IDs or store handles.
    provisional_objects: Vec<noon_core::SemanticLocalNodeToken>,
    /// Only admitted local objects may materialize when the callback commits.
    /// Unadmitted constructor temporaries are canceled with the transaction.
    admitted_provisionals: Vec<noon_core::SemanticLocalNodeToken>,
    stages: u16,
}

#[cfg(any(target_arch = "wasm32", test))]
struct CommittedCallbackProvisionals {
    token: CallbackPhaseToken,
    nodes: Vec<(noon_core::SemanticLocalNodeToken, SemanticNodeId)>,
}

/// A phase-bound name for one callback-local object declaration. It carries no
/// semantic slot or generation, so it cannot be mistaken for a durable typed
/// Mobject before the shared callback publication resolves it.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub struct WasmCallbackProvisionalMobject {
    callback_token: CallbackPhaseToken,
    local: noon_core::SemanticLocalNodeToken,
}

#[cfg(target_arch = "wasm32")]
impl WasmCallbackProvisionalMobject {
    pub(crate) const fn local_token(&self) -> noon_core::SemanticLocalNodeToken {
        self.local
    }

    pub(crate) const fn callback_token(&self) -> CallbackPhaseToken {
        self.callback_token
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
impl WasmCallbackProvisionalMobject {
    /// An opaque callback-local key for transaction-local membership reads.
    /// It is neither a semantic slot nor a generational object identity.
    #[wasm_bindgen::prelude::wasm_bindgen(getter, js_name = localKey)]
    pub fn local_key(&self) -> String {
        callback_provisional_key(self.local)
    }
}

/// One callback-local layout read. This is deliberately not a semantic handle:
/// its values remain scoped to the same pending callback token.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub struct WasmCallbackProvisionalPoint {
    x: f64,
    y: f64,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
impl WasmCallbackProvisionalPoint {
    #[wasm_bindgen::prelude::wasm_bindgen(getter)]
    pub fn x(&self) -> f64 {
        self.x
    }

    #[wasm_bindgen::prelude::wasm_bindgen(getter)]
    pub fn y(&self) -> f64 {
        self.y
    }
}

// Each stage re-preflights the accumulated transaction to preserve exact
// rollback proof. Bound the callback-local operation count so this remains
// local to the callback batch, never proportional to an unbounded host loop.
#[cfg(any(target_arch = "wasm32", test))]
const MAX_CALLBACK_MEMBERSHIP_STAGES: u16 = 128;

#[cfg(any(target_arch = "wasm32", test))]
fn callback_provisional_key(local: noon_core::SemanticLocalNodeToken) -> String {
    format!("callback-local:{local:?}")
}

#[cfg(any(target_arch = "wasm32", test))]
impl LiveSegmentReceipt {
    fn segment(self) -> noon::ExecutionSegment {
        match self {
            Self::Pending(segment) | Self::Completed(segment) => segment,
        }
    }
}

/// Existing runtime and transport identities, observed at an ownership handoff.
/// This is not a new identity allocator, runtime, or continuation state machine.
#[cfg(any(target_arch = "wasm32", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ExecutionPlayerIdentity {
    runtime: RuntimeIdentity,
    transport_session: u32,
}

impl SemanticExecutionPlayer {
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn ownership_identity(&self) -> ExecutionPlayerIdentity {
        ExecutionPlayerIdentity {
            runtime: self.session.runtime_identity(),
            transport_session: self.encoder.session(),
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn belongs_to_authoring_scene(
        &self,
        store: &std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        root: noon_core::SemanticNodeId,
    ) -> bool {
        self.semantic_root == Some(root)
            && self
                .semantics
                .as_ref()
                .is_some_and(|owned| std::rc::Rc::ptr_eq(owned, store))
    }

    fn playback_clock(session: &ExecutionSession, duration: f64) -> Result<PlaybackClock, String> {
        if session.has_required_callbacks() {
            Ok(PlaybackClock::once())
        } else {
            PlaybackClock::looping(duration).map_err(|error| error.to_string())
        }
    }

    fn retain_callback_phase(
        &mut self,
        invocations: Vec<noon::integration::RequiredCallbackInvocation>,
        overlay: noon::integration::CallbackPhaseOverlay,
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
        #[cfg(any(target_arch = "wasm32", test))]
        {
            self.committed_callback_provisionals = None;
        }
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
        let encoder = RetainedFamilyExecutionDeltaEncoder::new_with_resources(
            transport_session,
            &resource_bundle,
        );
        let resource_bundle = resource_bundle
            .encode_binary()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            session,
            clock,
            encoder,
            resource_bundle,
            snapshot_sent: false,
            last_sent_selection_overlay: None,
            pending_callback_phase: None,
            #[cfg(any(target_arch = "wasm32", test))]
            callback_membership_transaction: None,
            #[cfg(any(target_arch = "wasm32", test))]
            committed_callback_provisionals: None,
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
            #[cfg(any(target_arch = "wasm32", test))]
            browser_pointer_binding: None,
            #[cfg(any(target_arch = "wasm32", test))]
            worker_pointer_presentation: Default::default(),
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
        let encoder = RetainedFamilyExecutionDeltaEncoder::new_with_resources(
            transport_session,
            &resource_bundle,
        );
        let resource_bundle = resource_bundle
            .encode_binary()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            session,
            clock,
            encoder,
            resource_bundle,
            snapshot_sent: false,
            last_sent_selection_overlay: None,
            pending_callback_phase: None,
            callback_membership_transaction: None,
            committed_callback_provisionals: None,
            semantics: Some(semantics),
            semantic_root: Some(semantic_root),
            live_segment: None,
            live_wake_clock: BrowserExecutionWakeClock::default(),
            next_native_event_sequence: 0,
            browser_pointer_binding: None,
            #[cfg(any(target_arch = "wasm32", test))]
            worker_pointer_presentation: Default::default(),
        })
    }

    /// Request bounded history only for sequential source execution. Pre-authored
    /// immutable plans already contain their complete timeline and need no journal.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn begin_replay_retention(&mut self) -> Result<(), String> {
        self.session
            .begin_replay_retention(noon_runtime::ReplayLimits::default())
            .map_err(|error| error.to_string())
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
        let encoder = RetainedFamilyExecutionDeltaEncoder::new_with_resources(
            transport_session,
            &resource_bundle,
        );
        let resource_bundle = resource_bundle
            .encode_binary()
            .map_err(|error| error.to_string())?;
        self.clock = clock;
        self.resource_bundle = resource_bundle;
        self.encoder = encoder;
        self.worker_pointer_presentation = Default::default();
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

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_bind_click_indicate(
        &mut self,
        target: &noon::Mobject,
        indication: noon::IndicateOptions,
        options: noon_core::AnimationOptions,
    ) -> Result<(), AuthoringFailure> {
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
        .on_click_indicate(target, indication, options)
        .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_set_translation(
        &mut self,
        mobject: &noon::Mobject,
        x: f64,
        y: f64,
    ) -> Result<(), AuthoringFailure> {
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
        .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_move_to(
        &mut self,
        mobject: &noon::Mobject,
        target: noon::LiveLayoutTarget<'_>,
        edge: (f64, f64),
        mask: (f64, f64),
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.move_to(mobject, target, edge, mask))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_become_family(
        &mut self,
        source: &noon::MobjectFamily,
        target: &noon::MobjectFamily,
        options: noon::ManimBecomeOptions,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.become_family(source, target, options))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_points_as_corners(
        &mut self,
        source: &noon::Mobject,
        points: &[noon_core::Vec2],
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_points_as_corners(source, points))
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_start_new_path(
        &mut self,
        source: &noon::Mobject,
        point: noon_core::Vec2,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.start_new_path(source, point))
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_add_line_to(
        &mut self,
        source: &noon::Mobject,
        point: noon_core::Vec2,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.add_line_to(source, point))
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_add_quadratic_bezier_curve_to(
        &mut self,
        source: &noon::Mobject,
        control: noon_core::Vec2,
        anchor: noon_core::Vec2,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.add_quadratic_bezier_curve_to(source, control, anchor))
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_add_cubic_bezier_curve_to(
        &mut self,
        source: &noon::Mobject,
        control1: noon_core::Vec2,
        control2: noon_core::Vec2,
        anchor: noon_core::Vec2,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| {
            live.add_cubic_bezier_curve_to(source, control1, control2, anchor)
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_points_smoothly(
        &mut self,
        object: &noon::Mobject,
        points: &[noon_core::Vec2],
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_points_smoothly(object, points))
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_change_anchor_mode(
        &mut self,
        object: &noon::Mobject,
        smooth: bool,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| {
            if smooth {
                live.make_smooth(object)
            } else {
                live.make_jagged(object)
            }
        })
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_change_family_anchor_mode(
        &mut self,
        family: &noon::MobjectFamily,
        smooth: bool,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| {
            if smooth {
                live.make_family_smooth(family)
            } else {
                live.make_family_jagged(family)
            }
        })
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_insert_n_curves(
        &mut self,
        object: &noon::Mobject,
        additional: usize,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.insert_n_curves(object, additional))
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_reverse_direction(
        &mut self,
        object: &noon::Mobject,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.reverse_direction(object))
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_subcurve(
        &mut self,
        source: &noon::Mobject,
        a: f64,
        b: f64,
    ) -> Result<noon::Mobject, AuthoringFailure> {
        self.with_live_session(|live| live.subcurve(source, a, b))
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_align_points(
        &mut self,
        left: &noon::Mobject,
        right: &noon::Mobject,
    ) -> Result<(), AuthoringFailure> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        let root = self
            .semantic_root
            .expect("live semantic store has one scene root");
        noon::integration::publish_alignment(&semantics, root, &mut self.session, left, right)
            .map_err(AuthoringFailure::from)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_pointwise_become_partial(
        &mut self,
        object: &noon::Mobject,
        source: &noon::Mobject,
        a: f64,
        b: f64,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.pointwise_become_partial(object, source, a, b))
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_close_path(
        &mut self,
        source: &noon::Mobject,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.close_path(source))
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_match_points(
        &mut self,
        source: &noon::Mobject,
        target: &noon::Mobject,
    ) -> Result<(), AuthoringFailure> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        let root = self
            .semantic_root
            .expect("live semantic store has one scene root");
        noon::integration::publish_match_points(&semantics, root, &mut self.session, source, target)
            .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_become_mobject(
        &mut self,
        target: &noon::Mobject,
        other: &noon::Mobject,
        options: noon::ManimBecomeOptions,
    ) -> Result<(), AuthoringFailure> {
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
        .become_mobject(target, other, options)
        .map(|_| ())
        .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_create_manim_geometry(
        &mut self,
        options: noon::ManimGeometryOptions,
    ) -> Result<noon::Mobject, AuthoringFailure> {
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
        .create_manim_geometry(options)
        .map_err(AuthoringFailure::from)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_create_image(
        &mut self,
        options: noon::ImageMobjectOptions,
    ) -> Result<noon::Mobject, AuthoringFailure> {
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
        .create_image(options)
        .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_create_text(
        &mut self,
        text: noon::Text,
    ) -> Result<noon::Mobject, AuthoringFailure> {
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
        .create_text(text)
        .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_create_typst(
        &mut self,
        text: noon::Typst,
    ) -> Result<noon::Mobject, AuthoringFailure> {
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
        .create_typst(text)
        .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_create_math_typst(
        &mut self,
        text: noon::MathTypst,
    ) -> Result<noon::Mobject, AuthoringFailure> {
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
        .create_math_typst(text)
        .map_err(AuthoringFailure::from)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_create_latex(
        &mut self,
        text: crate::authoring_latex::AuthoredLatex,
        compiler: &mut crate::WasmLatexCompiler,
    ) -> Result<noon::LatexParts, AuthoringFailure> {
        self.with_live_session(|live| match text {
            crate::authoring_latex::AuthoredLatex::Text(text) => {
                live.create_tex_parts(text, compiler)
            }
            crate::authoring_latex::AuthoredLatex::Math(text) => {
                live.create_math_tex_parts(text, compiler)
            }
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_color_gradient(
        &mut self,
        target: &noon::Mobject,
        colors: &[noon::Color],
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_color_by_gradient(target, colors))
            .map(|_| ())
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_family_color_gradient(
        &mut self,
        target: &noon::MobjectFamily,
        colors: &[noon::Color],
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_family_color_by_gradient(target, colors))
            .map(|_| ())
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_effective_fill_color(
        &mut self,
        target: &noon::Mobject,
    ) -> Result<Option<noon_core::Color>, AuthoringFailure> {
        self.with_live_session(|live| live.effective_fill_color(target))
    }
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_effective_stroke_color(
        &mut self,
        target: &noon::Mobject,
    ) -> Result<Option<noon_core::Color>, AuthoringFailure> {
        self.with_live_session(|live| live.effective_stroke_color(target))
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_effective_stroke_width(
        &mut self,
        target: &noon::Mobject,
    ) -> Result<f64, AuthoringFailure> {
        self.with_live_session(|live| live.effective_stroke_width(target))
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_style(
        &mut self,
        source: &noon::Mobject,
        update: noon::StyleUpdate,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_style(source, update))
            .map(|_| ())
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_match_style(
        &mut self,
        source: &noon::Mobject,
        target: &noon::Mobject,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.match_style(source, target))
            .map(|_| ())
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_family_style(
        &mut self,
        source: &noon::MobjectFamily,
        update: noon::StyleUpdate,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_family_style(source, update))
            .map(|_| ())
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_match_family_style(
        &mut self,
        source: &noon::MobjectFamily,
        target: &noon::MobjectFamily,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.match_family_style(source, target))
            .map(|_| ())
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_family_color(
        &mut self,
        family: &noon::MobjectFamily,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_family_color(family, red, green, blue, alpha))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_family_member_colors(
        &mut self,
        family: &noon::MobjectFamily,
        colors: &[Option<noon::Color>],
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_family_member_colors(family, colors))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_latex_font_size(
        &mut self,
        parts: &noon::LatexParts,
    ) -> Result<f64, AuthoringFailure> {
        self.with_live_session(|live| live.latex_font_size(parts))
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_family_fill(
        &mut self,
        family: &noon::MobjectFamily,
        color: Option<noon::Color>,
        opacity: Option<f64>,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_family_fill(family, color, opacity))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_family_stroke(
        &mut self,
        family: &noon::MobjectFamily,
        color: Option<noon::Color>,
        width: Option<f64>,
        opacity: Option<f64>,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_family_stroke(family, color, width, opacity))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_family_opacity(
        &mut self,
        family: &noon::MobjectFamily,
        opacity: f64,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_family_opacity(family, opacity))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_fill(
        &mut self,
        mobject: &noon::Mobject,
        red: f64,
        green: f64,
        blue: f64,
        opacity: f64,
    ) -> Result<(), AuthoringFailure> {
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
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_fill_color(mobject, red, green, blue, alpha))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_disable_fill(
        &mut self,
        mobject: &noon::Mobject,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.disable_fill(mobject))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_fill_opacity(
        &mut self,
        mobject: &noon::Mobject,
        opacity: f64,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_fill_opacity(mobject, opacity))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_image_sampling(
        &mut self,
        mobject: &noon::Mobject,
        sampling: noon::RasterImageSampling,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_image_sampling(mobject, sampling))
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_color(
        &mut self,
        mobject: &noon::Mobject,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), AuthoringFailure> {
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
    ) -> Result<(), AuthoringFailure> {
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
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_stroke_color(mobject, red, green, blue, alpha))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_disable_stroke(
        &mut self,
        mobject: &noon::Mobject,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.disable_stroke(mobject))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_stroke_opacity(
        &mut self,
        mobject: &noon::Mobject,
        opacity: f64,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_stroke_opacity(mobject, opacity))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_opacity(
        &mut self,
        mobject: &noon::Mobject,
        opacity: f64,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_opacity(mobject, opacity))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_object_opacity(
        &mut self,
        mobject: &noon::Mobject,
        opacity: f64,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_object_opacity(mobject, opacity))
            .map(|_| ())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn with_live_session<T>(
        &mut self,
        operation: impl FnOnce(&mut noon::LiveSession<'_>) -> Result<T, noon::LiveSessionError>,
    ) -> Result<T, AuthoringFailure> {
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
        .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_replace_content(
        &mut self,
        target: &noon::Mobject,
        source: &noon::Mobject,
    ) -> Result<(), AuthoringFailure> {
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
        .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_shift(
        &mut self,
        mobject: &noon::Mobject,
        x: f64,
        y: f64,
    ) -> Result<(), AuthoringFailure> {
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
        .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_shift_family(
        &mut self,
        family: &noon::MobjectFamily,
        x: f64,
        y: f64,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|session| session.shift_family(family, x, y))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_arrange_family_in_grid(
        &mut self,
        family: &noon::MobjectFamily,
        options: &noon::FamilyGridOptions,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.arrange_family_in_grid_with_options(family, options))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_rescale_to_fit(
        &mut self,
        source: &noon::LayoutAnchor,
        length: f64,
        dimension: noon::LayoutDimension,
        stretch: bool,
        pivot: noon::ManimRotationPivot,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| {
            live.rescale_to_fit_with_pivot(source, length, dimension, stretch, pivot)
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_replace_layout(
        &mut self,
        source: &noon::LayoutAnchor,
        target: &noon::LayoutAnchor,
        dimension: noon::LayoutDimension,
        stretch: bool,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.replace_layout(source, target, dimension, stretch))
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_match_dim_size(
        &mut self,
        source: &noon::LayoutAnchor,
        target: &noon::LayoutAnchor,
        dimension: noon::LayoutDimension,
        stretch: bool,
        pivot: noon::ManimRotationPivot,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| {
            live.match_dim_size_with_pivot(source, target, dimension, stretch, pivot)
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_scale_family(
        &mut self,
        family: &noon::MobjectFamily,
        x: f64,
        y: f64,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|session| session.scale_family(family, x, y))
            .map(|_| ())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_arrange_family(
        &mut self,
        family: &noon::MobjectFamily,
        options: &noon::FamilyArrangeOptions,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|session| session.arrange_family_with_options(family, options))
            .map(|_| ())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_copy_family(
        &mut self,
        source: &noon::MobjectFamily,
        references: &[noon::MobjectTarget<'_>],
    ) -> Result<noon::FamilyCopy, AuthoringFailure> {
        self.with_live_session(|live| live.copy_family_with_references(source, references))
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_cyclic_replace_target(
        &mut self,
        source: &noon::MobjectFamily,
        references: &[noon::MobjectTarget<'_>],
    ) -> Result<noon::FamilyCopy, AuthoringFailure> {
        self.with_live_session(|live| {
            live.cyclic_replace_target_with_references(source, references)
        })
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_family_layout(
        &mut self,
        family: &noon::MobjectFamily,
    ) -> Result<noon::EffectiveMobjectLayout, AuthoringFailure> {
        self.with_live_session(|live| live.effective_family_layout(family))
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_move_family_to(
        &mut self,
        family: &noon::MobjectFamily,
        target: noon::LiveLayoutTarget<'_>,
        edge: (f64, f64),
        mask: (f64, f64),
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.move_family_to(family, target, edge, mask))
            .map(|_| ())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_next_layout_to_aligned(
        &mut self,
        source: &noon::LayoutAnchor,
        target: noon::LiveLayoutTarget<'_>,
        aligner: &noon::LayoutAnchor,
        args: noon::ManimNextToArgs,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.next_layout_to_aligned(source, target, aligner, args))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_align_family_on_frame(
        &mut self,
        family: &noon::MobjectFamily,
        direction: (f64, f64),
        buff: f64,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.align_family_on_frame(family, direction, buff))
            .map(|_| ())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_align_family_to(
        &mut self,
        family: &noon::MobjectFamily,
        target: noon::LiveLayoutTarget<'_>,
        axis: (f64, f64),
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.align_family_to(family, target, axis))
            .map(|_| ())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_set_scale(
        &mut self,
        mobject: &noon::Mobject,
        x: f64,
        y: f64,
    ) -> Result<(), AuthoringFailure> {
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
        .map_err(AuthoringFailure::from)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_apply_matrix(
        &mut self,
        mobject: &noon::Mobject,
        values: &[f64],
        rows: usize,
        columns: usize,
        about_x: f64,
        about_y: f64,
    ) -> Result<(), AuthoringFailure> {
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
        .apply_matrix(mobject, values, rows, columns, about_x, about_y)
        .map_err(AuthoringFailure::from)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_scale(
        &mut self,
        mobject: &noon::Mobject,
        x: f64,
        y: f64,
    ) -> Result<(), AuthoringFailure> {
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
        .manim_scale(mobject, x, y)
        .map(|_| ())
        .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_set_rotation(
        &mut self,
        mobject: &noon::Mobject,
        angle: f64,
    ) -> Result<(), AuthoringFailure> {
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
        .map_err(AuthoringFailure::from)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_scale_layout(
        &mut self,
        anchor: &noon::LayoutAnchor,
        x: f64,
        y: f64,
        pivot: noon::ManimRotationPivot,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.scale_layout(anchor, x, y, pivot))
            .map(|_| ())
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_rotate_layout(
        &mut self,
        anchor: &noon::LayoutAnchor,
        angle: f64,
        pivot: noon::ManimRotationPivot,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.rotate_layout(anchor, angle, pivot))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_z_index(
        &mut self,
        anchor: &noon::LayoutAnchor,
    ) -> Result<f64, AuthoringFailure> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::integration::effective_z_index(&semantics, &self.session, anchor)
            .map_err(AuthoringFailure::from)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_set_z_index(
        &mut self,
        anchor: &noon::LayoutAnchor,
        value: f64,
        family: bool,
    ) -> Result<(), AuthoringFailure> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::integration::publish_z_index(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
            anchor,
            value,
            family,
        )
        .map_err(AuthoringFailure::from)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_flip_layout(
        &mut self,
        anchor: &noon::LayoutAnchor,
        axis: noon::SemanticVec3,
        pivot: noon::ManimRotationPivot,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.flip_layout(anchor, axis, pivot))
            .map(|_| ())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_effective(
        &mut self,
        mobject: &noon::Mobject,
    ) -> Result<noon::EffectiveMobjectState, AuthoringFailure> {
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
        .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_effective_layout(
        &mut self,
        mobject: &noon::Mobject,
    ) -> Result<noon::EffectiveMobjectLayout, AuthoringFailure> {
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
        .map_err(AuthoringFailure::from)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_boolean_geometry_options(
        &mut self,
        operation: noon::BooleanOperation,
        operands: &[noon::Mobject],
    ) -> Result<noon::ManimGeometryOptions, AuthoringFailure> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::integration::effective_boolean_geometry_options(
            &semantics,
            &self.session,
            operation,
            operands,
        )
        .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn effective_path_query(
        &mut self,
        mobject: &noon::Mobject,
    ) -> Result<noon::PathQuery, AuthoringFailure> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::integration::effective_path_query(&semantics, &self.session, mobject)
            .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_effective_line_endpoints(
        &mut self,
        mobject: &noon::Mobject,
    ) -> Result<noon::ManimLineEndpoints, AuthoringFailure> {
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
        .effective_line_endpoints(mobject)
        .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_effective_manim_color(
        &mut self,
        mobject: &noon::Mobject,
    ) -> Result<noon_core::Color, AuthoringFailure> {
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
        .effective_manim_color(mobject)
        .map_err(AuthoringFailure::from)
    }

    /// Publish one already validated scene-membership batch through the active
    /// semantic session. The player retains no membership or painter-order mirror.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_edit_membership(
        &mut self,
        request: noon::SceneMembershipRequest<'_>,
    ) -> Result<(), AuthoringFailure> {
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
        .edit_membership(request)
        .map(|_| ())
        .map_err(AuthoringFailure::from)
    }

    /// Stage one existing-handle root-membership operation beside an exact
    /// required callback token. The collector owns one transaction across the
    /// entire callback; every accepted operation remains unpublished until the
    /// existing callback completion co-publishes it with effective writes.
    /// A rejected staging operation restores the prior collector so a callback
    /// may catch it and continue. A later final publication failure is terminal
    /// because the shared runtime contract consumes the prepared proof.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn stage_required_callback_membership(
        &mut self,
        expected_token: CallbackPhaseToken,
        batch: &crate::canonical_authoring_scene::SceneMembershipBatch,
    ) -> Result<(), AuthoringFailure> {
        let token = self
            .pending_callback_phase
            .map(|(token, _)| token)
            .ok_or("callback membership has no player pending phase")?;
        if token != expected_token {
            return Err("callback membership token is stale".into());
        }
        if batch.has_callback_provisionals() {
            return self.stage_required_callback_mixed_addition(expected_token, batch);
        }
        let semantics = self
            .semantics
            .clone()
            .ok_or("callback membership requires a live semantic store")?;
        let root = self
            .semantic_root
            .ok_or("callback membership requires one semantic scene root")?;
        let mut collector = self.callback_membership_transaction.take();
        if collector
            .as_ref()
            .is_some_and(|existing| existing.token != token)
        {
            self.callback_membership_transaction = collector;
            return Err("callback membership collector token is stale".into());
        }
        let (transaction, provisional_objects, admitted_provisionals, stages) = collector
            .take()
            .map(|existing| {
                (
                    existing.transaction,
                    existing.provisional_objects,
                    existing.admitted_provisionals,
                    existing.stages,
                )
            })
            .unwrap_or_default();
        if stages == MAX_CALLBACK_MEMBERSHIP_STAGES {
            self.callback_membership_transaction = Some(CallbackMembershipCollector {
                token,
                transaction,
                provisional_objects,
                admitted_provisionals,
                stages,
            });
            return Err("callback membership staging exceeded its bounded operation limit".into());
        }

        let mut transaction = Some(transaction);
        let outcome = batch.with_existing_callback_membership(&semantics, |request| {
            let current_transaction = transaction
                .take()
                .expect("callback membership transaction is retained across one stage");
            let mut store = semantics.borrow_mut();
            let prepared = match current_transaction.prepare_recoverable(&mut store) {
                Ok(prepared) => prepared,
                Err((recovered, error)) => {
                    transaction = Some(recovered);
                    return Err(AuthoringFailure::from(error));
                }
            };
            match stage_prepared_semantic_scene_membership(prepared, root, request) {
                Ok(prepared) => {
                    transaction = Some(prepared.into_transaction());
                    Ok(())
                }
                Err(error) => {
                    let (prepared, cause) = error.into_parts();
                    transaction = Some(prepared.into_transaction());
                    Err(match cause {
                        noon_core::PreparedSemanticMembershipErrorKind::Operation(error) => {
                            AuthoringFailure::from(error)
                        }
                        noon_core::PreparedSemanticMembershipErrorKind::Transaction(error) => {
                            AuthoringFailure::from(error)
                        }
                    })
                }
            }
        });
        let transaction = transaction.expect("callback membership stage restores its transaction");
        self.callback_membership_transaction = Some(CallbackMembershipCollector {
            token,
            transaction,
            provisional_objects,
            admitted_provisionals,
            stages: stages + u16::from(outcome.is_ok()),
        });
        outcome
    }

    /// Stage one ordered Scene.add containing original typed handles and
    /// callback-local objects. Validation and planning cover the complete list
    /// before this collector accepts any membership edge, so a caught bad
    /// argument leaves earlier callback work untouched.
    #[cfg(any(target_arch = "wasm32", test))]
    fn stage_required_callback_mixed_addition(
        &mut self,
        expected_token: CallbackPhaseToken,
        batch: &crate::canonical_authoring_scene::SceneMembershipBatch,
    ) -> Result<(), AuthoringFailure> {
        let token = self
            .pending_callback_phase
            .map(|(token, _)| token)
            .ok_or("callback membership has no player pending phase")?;
        if token != expected_token {
            return Err("callback membership token is stale".into());
        }
        let semantics = self
            .semantics
            .clone()
            .ok_or("callback membership requires a live semantic store")?;
        let root = self
            .semantic_root
            .ok_or("callback membership requires one semantic scene root")?;
        let Some(mut collector) = self.callback_membership_transaction.take() else {
            return Err("callback provisional geometry is unknown".into());
        };
        if collector.token != token {
            self.callback_membership_transaction = Some(collector);
            return Err("callback membership collector token is stale".into());
        }
        if collector.stages == MAX_CALLBACK_MEMBERSHIP_STAGES {
            self.callback_membership_transaction = Some(collector);
            return Err("callback membership staging exceeded its bounded operation limit".into());
        }
        let transaction = std::mem::take(&mut collector.transaction);
        let mut transaction = Some(transaction);
        let outcome = batch.with_callback_mixed_addition(token, &semantics, |members| {
            let provisionals = members
                .iter()
                .filter_map(|member| match member {
                    noon_core::SemanticTransactionNodeRef::Pending(local) => Some(*local),
                    noon_core::SemanticTransactionNodeRef::Existing(_) => None,
                })
                .collect::<Vec<_>>();
            if provisionals.is_empty()
                || provisionals.iter().any(|local| {
                    !collector.provisional_objects.contains(local)
                        || collector.admitted_provisionals.contains(local)
                })
            {
                return Err(AuthoringFailure::new(
                    "invalid_input",
                    "callback.membership",
                    "callback provisional geometry token is unknown, stale, or already admitted",
                ));
            }
            let current = transaction
                .take()
                .expect("callback membership transaction is retained across one stage");
            let mut store = semantics.borrow_mut();
            let prepared = match current.prepare_recoverable(&mut store) {
                Ok(prepared) => prepared,
                Err((recovered, error)) => {
                    transaction = Some(recovered);
                    return Err(AuthoringFailure::from(error));
                }
            };
            match noon_core::stage_prepared_semantic_scene_admission(prepared, root, &members) {
                Ok(prepared) => {
                    transaction = Some(prepared.into_transaction());
                    Ok(())
                }
                Err(error) => {
                    let (prepared, cause) = error.into_parts();
                    transaction = Some(prepared.into_transaction());
                    Err(match cause {
                        noon_core::PreparedSemanticMembershipErrorKind::Operation(error) => {
                            AuthoringFailure::from(error)
                        }
                        noon_core::PreparedSemanticMembershipErrorKind::Transaction(error) => {
                            AuthoringFailure::from(error)
                        }
                    })
                }
            }
        });
        collector.transaction =
            transaction.expect("callback membership stage restores its transaction");
        if outcome.is_ok() {
            collector.admitted_provisionals.extend(
                batch
                    .callback_provisionals()
                    .expect("mixed batch retains local members"),
            );
            collector.stages += 1;
        }
        self.callback_membership_transaction = Some(collector);
        outcome
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn take_committed_callback_provisional(
        &mut self,
        token: CallbackPhaseToken,
        local: noon_core::SemanticLocalNodeToken,
    ) -> Result<SemanticNodeId, AuthoringFailure> {
        let Some(mut committed) = self.committed_callback_provisionals.take() else {
            return Err(AuthoringFailure::new(
                "stale_publication",
                "callback.stale_provisional",
                "callback provisional geometry has no committed phase",
            ));
        };
        if committed.token != token {
            self.committed_callback_provisionals = Some(committed);
            return Err(AuthoringFailure::new(
                "stale_publication",
                "callback.stale_provisional",
                "callback provisional geometry token is stale",
            ));
        }
        let Some(index) = committed
            .nodes
            .iter()
            .position(|(candidate, _)| *candidate == local)
        else {
            self.committed_callback_provisionals = Some(committed);
            return Err(AuthoringFailure::new(
                "stale_publication",
                "callback.stale_provisional",
                "callback provisional geometry token is unknown",
            ));
        };
        let (_, node) = committed.nodes.remove(index);
        if !committed.nodes.is_empty() {
            self.committed_callback_provisionals = Some(committed);
        }
        Ok(node)
    }

    /// Read the callback collector's direct-root order without publishing it.
    /// The temporary prepared view is consumed immediately, returning the exact
    /// transaction to the pinned collector rather than cloning a scene mirror.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn callback_membership_root_keys(
        &mut self,
        expected_token: CallbackPhaseToken,
    ) -> Result<Vec<String>, AuthoringFailure> {
        let token = self
            .pending_callback_phase
            .map(|(token, _)| token)
            .ok_or("callback membership has no player pending phase")?;
        if token != expected_token {
            return Err("callback membership token is stale".into());
        }
        let semantics = self
            .semantics
            .clone()
            .ok_or("callback membership requires a live semantic store")?;
        let root = self
            .semantic_root
            .ok_or("callback membership requires one semantic scene root")?;
        let Some(collector) = self.callback_membership_transaction.take() else {
            return semantics
                .borrow()
                .semantic_family_members_checked(root)
                .map(|members| {
                    members
                        .iter()
                        .map(|node| format!("{}:{}", node.slot(), node.generation()))
                        .collect()
                })
                .map_err(AuthoringFailure::from);
        };
        if collector.token != token {
            self.callback_membership_transaction = Some(collector);
            return Err("callback membership collector token is stale".into());
        }
        let mut store = semantics.borrow_mut();
        let prepared = match collector.transaction.prepare_recoverable(&mut store) {
            Ok(prepared) => prepared,
            Err((transaction, error)) => {
                self.callback_membership_transaction = Some(CallbackMembershipCollector {
                    token,
                    transaction,
                    provisional_objects: collector.provisional_objects,
                    admitted_provisionals: collector.admitted_provisionals,
                    stages: collector.stages,
                });
                return Err(AuthoringFailure::from(error));
            }
        };
        let members = prepared
            .family_members(root)
            .map_err(|error| AuthoringFailure::unclassified("callback.membership_read", &error));
        let transaction = prepared.into_transaction();
        self.callback_membership_transaction = Some(CallbackMembershipCollector {
            token,
            transaction,
            provisional_objects: collector.provisional_objects,
            admitted_provisionals: collector.admitted_provisionals,
            stages: collector.stages,
        });
        members?
            .into_iter()
            .map(|member| match member {
                noon_core::SemanticTransactionNodeRef::Existing(node) => {
                    Ok(format!("{}:{}", node.slot(), node.generation()))
                }
                noon_core::SemanticTransactionNodeRef::Pending(local) => {
                    Ok(callback_provisional_key(local))
                }
            })
            .collect()
    }

    /// Route callback declarations through the same shared semantic publication
    /// as native authoring. The browser host owns no callback schedule mirror.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_edit_updaters(
        &mut self,
        transaction: noon_core::SemanticMutationTransaction,
    ) -> Result<(), String> {
        self.require_completed_live_segment()
            .map_err(|error| error.to_string())?;
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
        .apply(transaction)
        .map(|_| ())
        .map_err(|error| error.to_string())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn require_completed_live_segment(&self) -> Result<(), AuthoringFailure> {
        self.require_callback_progression_available()?;
        if self.has_pending_live_segment() {
            return Err(AuthoringFailure::from(
                noon::ExecutionSessionPublicationError::SegmentCompletionPending,
            )
            .with_message("complete the current live segment before continuing"));
        }
        Ok(())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn require_callback_progression_available(&self) -> Result<(), AuthoringFailure> {
        if let Some(termination) = self.session.callback_termination() {
            return Err(AuthoringFailure::from(
                noon::ExecutionSegmentCompletionError::CallbackTerminated(termination),
            )
            .with_message(format!(
                "required callback progression terminated: {:?}",
                termination.kind()
            )));
        }
        if self.pending_callback_phase.is_some() {
            return Err(AuthoringFailure::from(
                noon::ExecutionSessionPublicationError::RequiredCallbackPending,
            )
            .with_message("a required callback phase is pending host completion"));
        }
        Ok(())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_play_animation(
        &mut self,
        animation: &noon::DeclaredAnimation,
    ) -> Result<f64, String> {
        self.require_completed_live_segment()
            .map_err(|error| error.to_string())?;
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

    /// Atomically declare and activate one shared affine appearance lifecycle.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_declare_and_activate_affine_lifecycle(
        &mut self,
        target: &noon::Mobject,
        direction: noon::AffineLifecycleDirection,
        endpoint: noon::AffineLifecycleEndpoint,
        options: noon_core::AnimationOptions,
    ) -> Result<f64, String> {
        self.require_completed_live_segment()
            .map_err(|error| error.to_string())?;
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
        .declare_and_activate_affine_lifecycle(target, direction, endpoint, options)
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
    pub(crate) fn live_contains(
        &mut self,
        target: &noon::Mobject,
    ) -> Result<bool, AuthoringFailure> {
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
        .map_err(AuthoringFailure::from)
    }

    /// Atomically admit and activate one recursive composition request through the shared
    /// session. The request remains inert until this call; schedule, admission, and runtime
    /// publication stay owned by Rust.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_declare_and_activate_composition(
        &mut self,
        request: &noon::AnimationCompositionRequest<'_>,
        play_options: noon_core::AnimationOptions,
    ) -> Result<f64, String> {
        self.require_completed_live_segment()
            .map_err(|error| error.to_string())?;
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
        .declare_and_activate_composition(request, play_options)
        .map_err(|error| error.to_string())?;
        let end_time = segment.end_time();
        self.clock = self
            .live_clock_at(self.session.frame().time, end_time, true)
            .expect("validated execution segment must produce a valid presentation clock");
        self.live_segment = Some(LiveSegmentReceipt::Pending(segment));
        self.live_wake_clock = BrowserExecutionWakeClock::default();
        Ok(end_time)
    }

    /// Create and sparsely enroll one scalar tracker in this retained session.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_value_tracker(
        &mut self,
        initial: f64,
    ) -> Result<noon::ValueTracker, AuthoringFailure> {
        self.require_completed_live_segment()?;
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::integration::publish_value_tracker_creation(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
            initial,
        )
        .map_err(AuthoringFailure::from)
    }

    /// Associate and sparsely enroll one pre-existing tracker in this live root.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_associate_value_tracker(
        &mut self,
        tracker: &noon::ValueTracker,
    ) -> Result<(), AuthoringFailure> {
        self.require_completed_live_segment()?;
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::integration::publish_value_tracker_association(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
            tracker,
        )
        .map_err(AuthoringFailure::from)
    }

    /// Create a detached target through the retained session so its semantic
    /// publication remains coherent with this runtime. Detached target rows do
    /// not create execution objects or frame work.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_target_editor(
        &mut self,
        source: &noon::Mobject,
    ) -> Result<noon::Mobject, AuthoringFailure> {
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
        .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_edit_family_members(
        &mut self,
        family: &noon::MobjectFamily,
        members: &[noon::MobjectTarget<'_>],
        adding: bool,
    ) -> Result<Vec<bool>, AuthoringFailure> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        let mut live = noon::LiveSession::new(
            &semantics,
            self.semantic_root.expect("live root exists"),
            &mut self.session,
        );
        if adding {
            live.add_family_members(family, members)
        } else {
            live.remove_family_members(family, members)
        }
        .map_err(AuthoringFailure::from)
    }

    /// Create one detached family through the retained session so its node and
    /// ordered edges share the current semantic/runtime publication.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_family(
        &mut self,
        members: &[noon::MobjectTarget<'_>],
        z_index: f64,
    ) -> Result<noon::MobjectFamily, AuthoringFailure> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::integration::publish_family_creation(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
            members,
            z_index,
        )
        .map_err(AuthoringFailure::from)
    }

    /// Apply subset-display constructor preparation through the active retained
    /// session so semantic and runtime publication remain one transaction.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn prepare_family_subset_display(
        &mut self,
        family: &noon::MobjectFamily,
    ) -> Result<(), String> {
        self.require_completed_live_segment()
            .map_err(|error| error.to_string())?;
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
        .prepare_family_subset_display(family)
        .map(|_| ())
        .map_err(|error| error.to_string())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_wait(&mut self, duration: f64) -> Result<f64, AuthoringFailure> {
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
        .map_err(AuthoringFailure::from)?;
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

    /// Callback wrapper finalizers may associate their resolved handles only
    /// after the session-owned callback phase has committed.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn has_pending_callback_phase(&self) -> bool {
        self.pending_callback_phase.is_some()
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
    ) -> Result<WasmExecutionWake, AuthoringFailure> {
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
        let plan = self.execution_wake_plan();
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

    #[cfg(any(target_arch = "wasm32", test))]
    fn execution_wake_plan(&self) -> BrowserExecutionWakePlan {
        let callback_blocked =
            self.pending_callback_phase.is_some() || self.session.callback_termination().is_some();
        let mut wake = self.session.wake_state();
        if callback_blocked || !self.clock.is_playing() {
            wake = wake.without_timeline_wake();
        } else if let Some(loop_duration) = self.clock.loop_duration() {
            // A completed authored wait has duration even without animated
            // channels. An un-authored static scene still remains fully idle.
            let authored_interval = self
                .live_segment
                .is_some_and(|receipt| receipt.segment().end_time() > 0.0);
            if self.session.has_replay_timeline_work() || authored_interval {
                wake = wake.with_additional_timeline(TimelineWakeState::Deadline(loop_duration));
            }
        }
        if !callback_blocked && self.session.interactions_active() {
            wake = wake.with_additional_timeline(TimelineWakeState::Continuous);
        }
        BrowserExecutionWakePlan::from_runtime(wake)
    }

    /// Observe elapsed playback during a visually static interval without evaluating
    /// a frame, publishing a delta, invoking callbacks or changing either clock.
    /// Active animation/callback work stays pinned to its coherent runtime sample.
    /// Sleeping waits project the existing Rust clock only up to the next runtime
    /// barrier; they cannot speculate past a source segment or a loop boundary.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn playback_time_at(&self, wall_time_ms: f64) -> Result<f64, String> {
        if !wall_time_ms.is_finite() {
            return Err("playback observation requires a finite wall timestamp".to_owned());
        }
        let current = self.time();
        if self.pending_callback_phase.is_some() || self.session.callback_termination().is_some() {
            return Ok(current);
        }
        if let Some(LiveSegmentReceipt::Pending(segment)) = self.live_segment {
            return Ok(match self.session.segment_state(segment).timeline() {
                TimelineWakeState::Deadline(deadline) => self
                    .live_wake_clock
                    .scene_time_at(wall_time_ms)
                    .unwrap_or(current)
                    .min(deadline)
                    .max(current),
                TimelineWakeState::Continuous | TimelineWakeState::Quiescent => current,
            });
        }
        if !self.clock.is_playing() {
            return Ok(current);
        }
        let BrowserExecutionCadence::TimerAtSceneTime(deadline) =
            self.execution_wake_plan().cadence()
        else {
            return Ok(current);
        };
        // Project through the same loop-aware deadline conversion used by wake
        // delivery. A copy keeps observation from starting/reanchoring playback.
        let remaining = self
            .clock
            .clone()
            .timer_delay_milliseconds(deadline, wall_time_ms, current)
            .map_err(|error| error.to_string())?;
        Ok((deadline - remaining / 1_000.0).min(deadline).max(current))
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
    ) -> Result<WasmExecutionWake, AuthoringFailure> {
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
    ) -> Result<WasmLiveSegmentDrive, AuthoringFailure> {
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
    ) -> Result<WasmLiveSegmentDrive, AuthoringFailure> {
        self.require_callback_progression_available()?;
        let current = self.session.frame().time;
        if !requested_time.is_finite() || requested_time < current {
            return Err(format!(
                "external continuation sample requires time at or after {current}, got {requested_time}"
            ).into());
        }
        let segment = self.live_segment()?;
        self.live_drive_segment_to(segment, requested_time)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_drive_segment_to(
        &mut self,
        segment: noon::ExecutionSegment,
        requested_time: f64,
    ) -> Result<WasmLiveSegmentDrive, AuthoringFailure> {
        let current_time = self.session.frame().time;
        let mut clock = self.live_clock_at(current_time, segment.end_time(), false)?;
        match self
            .session
            .advance_segment_to_callback_barrier(segment, requested_time)
            .map_err(AuthoringFailure::from)?
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
    pub(crate) fn live_advance_segment_to(
        &mut self,
        requested_time: f64,
    ) -> Result<bool, AuthoringFailure> {
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
    pub(crate) fn live_complete_segment(&mut self) -> Result<(), AuthoringFailure> {
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
        .map_err(AuthoringFailure::from)?;
        self.clock = clock;
        self.live_segment = Some(LiveSegmentReceipt::Completed(segment));
        self.live_wake_clock = BrowserExecutionWakeClock::default();
        Ok(())
    }

    /// Evaluate scalar tracks through the one execution session, then align the
    /// hold presentation at that same absolute time for a later handoff.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_evaluate(&mut self, time: f64) -> Result<(), AuthoringFailure> {
        let mut clock = self.clock.clone();
        clock.seek(time).map_err(AuthoringFailure::from)?;
        clock.pause();
        self.session
            .advance_to(time)
            .map_err(AuthoringFailure::from)?;
        self.clock = clock;
        Ok(())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_effective_signal(
        &self,
        tracker: &noon::ValueTracker,
    ) -> Result<f64, AuthoringFailure> {
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
    ) -> Result<(), AuthoringFailure> {
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
        .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn set_translation_drag_targets(
        &mut self,
        targets: &[noon::Mobject],
    ) -> Result<(), String> {
        self.with_live_session(|live| live.set_translation_drag_targets(targets.iter()))
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

    fn resource_bundle_for(session: &ExecutionSession) -> Result<RetainedResourceBundle, String> {
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
        .and_then(|bundle| {
            bundle.with_images(
                session
                    .frame()
                    .objects
                    .iter()
                    .filter_map(|object| object.content.image().map(|image| image.resource())),
                session.raster_image_resources(),
            )
        })
        .map_err(|error| error.to_string())
    }

    fn delta(
        &mut self,
        snapshot: bool,
    ) -> Result<Option<RetainedFamilyExecutionDeltaEnvelope>, String> {
        let presentation = self.session.pointer_selection_presentation();
        let overlay = presentation
            .as_ref()
            .map(crate::SelectionOverlayPresentation::from_presentation)
            .transpose()
            .map_err(|error| error.to_string())?;
        let camera = self
            .session
            .inspection_camera()
            .map_err(|e| e.to_string())?;
        #[cfg(any(target_arch = "wasm32", test))]
        let pointer_frame = self.worker_pointer_presentation.capture(&self.session)?;
        #[cfg(any(target_arch = "wasm32", test))]
        let pointer_refresh = self.worker_pointer_presentation.needs_delta(&self.session);
        #[cfg(not(any(target_arch = "wasm32", test)))]
        let pointer_refresh = false;
        let inset_2d_views = self.session.inset_2d_views().map_err(|e| e.to_string())?;
        let publication = self.session.take_renderer_publication();
        let mut changes = publication.changes().clone();
        if changes.is_empty()
            && (presentation != self.last_sent_selection_overlay || pointer_refresh)
        {
            // Presentation-only transport work, not authored/runtime dirtiness.
            // Reuse the existing sequence and backpressure; never invent a row.
            changes = noon_runtime::FrameChanges::presentation_redraw();
        }
        let frame = publication.frame();
        let planned = publication.planned_family_frame();
        let plans = publication.family_animation_plans();
        let painter_order = publication.painter_order();

        let mut delta = if snapshot || changes.is_all() || !self.snapshot_sent {
            let indices = painter_order
                .iter()
                .map(|&index| index as usize)
                .collect::<Vec<_>>();
            let text_handles = indices
                .iter()
                .filter_map(|&index| frame.objects[index].text())
                .collect::<Vec<_>>();
            let mut delta = self
                .encoder
                .encode_planned_snapshot_indices(&planned, plans, camera, indices)
                .map_err(|e| e.to_string())?;
            self.encoder
                .attach_resource_additions(
                    &mut delta,
                    text_handles,
                    publication.text_resources(),
                    publication.geometry_resources(),
                    publication.font_resources(),
                    publication.raster_image_resources(),
                )
                .map_err(|error| error.to_string())?;
            self.snapshot_sent = true;
            delta
        } else if changes.is_structural() || changes.has_painter_order_change() {
            let text_handles = changes
                .object_indices()
                .iter()
                .filter_map(|&index| frame.objects.get(index)?.text())
                .collect::<Vec<_>>();
            let Some(mut delta) = self
                .encoder
                .encode_planned_incremental_with_painter_order(
                    &planned,
                    plans,
                    &changes,
                    camera,
                    painter_order,
                )
                .map_err(|e| e.to_string())?
            else {
                return Ok(None);
            };
            self.encoder
                .attach_resource_additions(
                    &mut delta,
                    text_handles,
                    publication.text_resources(),
                    publication.geometry_resources(),
                    publication.font_resources(),
                    publication.raster_image_resources(),
                )
                .map_err(|error| error.to_string())?;
            delta
        } else {
            let text_handles = changes
                .object_indices()
                .iter()
                .filter_map(|&index| frame.objects.get(index)?.text())
                .collect::<Vec<_>>();
            let Some(mut delta) = self
                .encoder
                .encode_planned_incremental(&planned, plans, &changes, camera)
                .map_err(|e| e.to_string())?
            else {
                return Ok(None);
            };
            self.encoder
                .attach_resource_additions(
                    &mut delta,
                    text_handles,
                    publication.text_resources(),
                    publication.geometry_resources(),
                    publication.font_resources(),
                    publication.raster_image_resources(),
                )
                .map_err(|error| error.to_string())?;
            delta
        };
        delta.retained.inset_2d_views = inset_2d_views;
        delta
            .replace_transient_presentations(frame, publication.transient_presentations())
            .map_err(|error| error.to_string())?;
        delta.selection_overlay = overlay;
        #[cfg(any(target_arch = "wasm32", test))]
        {
            delta.pointer_view = self
                .worker_pointer_presentation
                .view()
                .filter(|view| view.drawable());
            self.worker_pointer_presentation.issue(
                delta.retained.session,
                delta.retained.sequence,
                pointer_frame,
            );
        }
        self.last_sent_selection_overlay = presentation;
        Ok(Some(delta))
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
            noon::integration::CallbackSequence::new(parse("sequence", token.sequence)?),
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
    region: u32,
    time: f64,
    delta_time: f64,
    objects: Vec<CallbackPhaseObjectWire>,
    invocations: Vec<CallbackInvocationWire>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct CallbackPhaseTokenEnvelope {
    token: CallbackTokenWire,
}

#[cfg(any(target_arch = "wasm32", test))]
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CallbackReadRequestWire {
    ScalarSignal { node: CallbackNodeWire },
    Object { node: CallbackNodeWire },
    Family { node: CallbackNodeWire },
}

#[cfg(any(target_arch = "wasm32", test))]
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CallbackReadValueWire {
    Scalar {
        value: f32,
    },
    Object {
        object: CallbackPhaseObjectWire,
    },
    Family {
        objects: Vec<CallbackPhaseObjectWire>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct CallbackTerminationWire {
    token: CallbackTokenWire,
    kind: &'static str,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct RendererObservationPublicationWire {
    delta: RetainedFamilyExecutionDeltaEnvelope,
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
    Translation {
        object: CallbackNodeWire,
        translation: noon_core::Vec2,
    },
    Rotation {
        object: CallbackNodeWire,
        rotation: f32,
    },
    Scale {
        object: CallbackNodeWire,
        scale: noon_core::Vec2,
    },
    Fill {
        object: CallbackNodeWire,
        fill: Option<noon_core::Color>,
    },
    Stroke {
        object: CallbackNodeWire,
        stroke: Option<noon_core::Color>,
    },
    StrokeWidth {
        object: CallbackNodeWire,
        stroke_width: f32,
    },
    Opacity {
        object: CallbackNodeWire,
        opacity: f32,
    },
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct CallbackBatchWire {
    token: CallbackTokenWire,
    #[serde(default)]
    region: u32,
    writes: Vec<CallbackWriteWire>,
    #[serde(default)]
    content: Option<CallbackContentWire>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct CallbackContentWire {
    object: CallbackNodeWire,
    geometry: CallbackAnalyticGeometryWire,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CallbackAnalyticGeometryWire {
    Circle { radius: f32 },
}

impl CallbackAnalyticGeometryWire {
    fn into_geometry(self) -> GeometryRef {
        match self {
            Self::Circle { radius } => GeometryRef::circle(radius),
        }
    }
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

fn decode_callback_batch(
    json: &str,
) -> Result<
    (
        EffectivePropertyBatch,
        Option<(SemanticNodeId, ObjectContentRef)>,
    ),
    String,
> {
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
            CallbackWriteWire::Translation {
                object,
                translation,
            } => Ok(EffectiveSemanticPropertyWrite::Translation {
                object: object.into(),
                translation,
            }),
            CallbackWriteWire::Rotation { object, rotation } => {
                Ok(EffectiveSemanticPropertyWrite::Rotation {
                    object: object.into(),
                    rotation,
                })
            }
            CallbackWriteWire::Scale { object, scale } => {
                Ok(EffectiveSemanticPropertyWrite::Scale {
                    object: object.into(),
                    scale,
                })
            }
            CallbackWriteWire::Fill { object, fill } => Ok(EffectiveSemanticPropertyWrite::Fill {
                object: object.into(),
                fill,
            }),
            CallbackWriteWire::Stroke { object, stroke } => {
                Ok(EffectiveSemanticPropertyWrite::Stroke {
                    object: object.into(),
                    stroke,
                })
            }
            CallbackWriteWire::StrokeWidth {
                object,
                stroke_width,
            } => Ok(EffectiveSemanticPropertyWrite::StrokeWidth {
                object: object.into(),
                stroke_width,
            }),
            CallbackWriteWire::Opacity { object, opacity } => {
                Ok(EffectiveSemanticPropertyWrite::Opacity {
                    object: object.into(),
                    opacity,
                })
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    let content = wire.content.map(|content| {
        (
            content.object.into(),
            ObjectContentRef::Geometry(content.geometry.into_geometry()),
        )
    });
    Ok((
        EffectivePropertyBatch::new(token, writes).with_region(wire.region),
        content,
    ))
}

// Keep the shared callback failures typed until the actual JS boundary. The
// decoding and preflight/commit order below are the existing worker protocol.
impl SemanticExecutionPlayer {
    #[cfg(any(target_arch = "wasm32", test))]
    pub fn required_callback_read_json(
        &mut self,
        token_json: &str,
        request_json: &str,
    ) -> Result<String, AuthoringFailure> {
        let token = Self::callback_token_from_json(token_json)?;
        self.pending_callback_phase
            .filter(|(pending, _)| *pending == token)
            .ok_or("callback read does not match the player pending phase")?;
        let request_wire: CallbackReadRequestWire = serde_json::from_str(request_json)
            .map_err(|error| format!("invalid callback read request JSON: {error}"))?;
        let requested_object = match &request_wire {
            CallbackReadRequestWire::Object { node } => Some(node.clone()),
            CallbackReadRequestWire::ScalarSignal { .. }
            | CallbackReadRequestWire::Family { .. } => None,
        };
        let request = match request_wire {
            CallbackReadRequestWire::Family { node } => {
                let store = self
                    .semantics
                    .as_ref()
                    .ok_or("family callback reads require a live semantic store")?;
                let rows = self
                    .session
                    .required_callback_family_read(&store.borrow(), token, node.into())
                    .map_err(AuthoringFailure::from)?;
                let objects = rows
                    .into_iter()
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
                    .collect();
                return serde_json::to_string(&CallbackReadValueWire::Family { objects })
                    .map_err(|error| AuthoringFailure::from(error.to_string()));
            }
            CallbackReadRequestWire::Object { node } => CallbackReadRequest::Object(node.into()),
            CallbackReadRequestWire::ScalarSignal { node } => {
                CallbackReadRequest::ScalarSignal(node.into())
            }
        };
        let value = self
            .session
            .required_callback_read(token, request)
            .map_err(AuthoringFailure::from)?;
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
        serde_json::to_string(&wire).map_err(|error| AuthoringFailure::from(error.to_string()))
    }

    pub fn commit_callback_phase_json(
        &mut self,
        batch_json: &str,
    ) -> Result<Option<String>, AuthoringFailure> {
        let (batch, content) = decode_callback_batch(batch_json)?;
        let token = batch.token();
        let time = match self.pending_callback_phase {
            Some((pending, time)) if pending == token => time,
            Some((pending, _)) => {
                return Err(noon::ExecutionSessionCallbackError::StaleToken {
                    expected: pending,
                    actual: token,
                }
                .into());
            }
            None => return Err(noon::ExecutionSessionCallbackError::NoPendingPhase.into()),
        };
        if content.is_some() {
            #[cfg(any(target_arch = "wasm32", test))]
            if self.callback_membership_transaction.is_some() {
                return Err("callback effective content and authored membership cannot share this publication yet".into());
            }
            self.session
                .require_terminal_callback_content_region(token, batch.region())
                .map_err(AuthoringFailure::from)?;
        }
        let advance = match self.session.submit_required_callback_region(batch) {
            Ok(advance) => advance,
            Err(error) => {
                // A collector may own provisional identities whose combined
                // publication cannot be retried after an invalid effective
                // write. Stale region replies, however, leave it intact.
                #[cfg(any(target_arch = "wasm32", test))]
                if self.callback_membership_transaction.is_some()
                    && matches!(
                        error,
                        noon::ExecutionSessionCallbackError::UnknownObject(_)
                            | noon::ExecutionSessionCallbackError::InvalidEffectiveWrite(_)
                            | noon::ExecutionSessionCallbackError::Commit(_)
                    )
                {
                    self.callback_membership_transaction = None;
                    self.pending_callback_phase = None;
                    let _ = self.session.fail_required_callback_phase(token);
                }
                return Err(AuthoringFailure::from(error));
            }
        };
        let batch = match advance {
            noon::integration::CallbackRegionAdvance::HostRequired {
                invocations,
                overlay,
            } => {
                debug_assert!(content.is_none(), "content region was proven terminal");
                return self
                    .retain_callback_phase(invocations, overlay)
                    .map(Some)
                    .map_err(AuthoringFailure::from);
            }
            noon::integration::CallbackRegionAdvance::Complete(batch) => batch,
        };
        #[cfg(any(target_arch = "wasm32", test))]
        {
            if let Some(collector) = self.callback_membership_transaction.take() {
                if collector.token != token {
                    self.callback_membership_transaction = Some(collector);
                    return Err("callback membership collector token is stale".into());
                }
                let semantics = self
                    .semantics
                    .clone()
                    .ok_or("callback membership requires a live semantic store")?;
                let order_root = self.semantic_root;
                let mut store = semantics.borrow_mut();
                let prepared = match collector.transaction.prepare_recoverable(&mut store) {
                    Ok(prepared) => prepared,
                    Err((transaction, error)) => {
                        self.callback_membership_transaction = Some(CallbackMembershipCollector {
                            token,
                            transaction,
                            provisional_objects: collector.provisional_objects,
                            admitted_provisionals: collector.admitted_provisionals,
                            stages: collector.stages,
                        });
                        return Err(AuthoringFailure::from(error));
                    }
                };
                let unadmitted = collector
                    .provisional_objects
                    .iter()
                    .copied()
                    .filter(|local| !collector.admitted_provisionals.contains(local))
                    .collect::<Vec<_>>();
                let prepared = if unadmitted.is_empty() {
                    prepared
                } else {
                    match prepared.with_pending_object_update(|transaction| {
                        for local in unadmitted {
                            transaction.remove_node(local);
                        }
                    }) {
                        Ok(prepared) => prepared,
                        Err((prepared, error)) => {
                            self.callback_membership_transaction =
                                Some(CallbackMembershipCollector {
                                    token,
                                    transaction: prepared.into_transaction(),
                                    provisional_objects: collector.provisional_objects,
                                    admitted_provisionals: collector.admitted_provisionals,
                                    stages: collector.stages,
                                });
                            return Err(AuthoringFailure::from(error));
                        }
                    }
                };
                // Once the prepared transaction enters the shared publication
                // contract it cannot be recovered: lowering may have consumed
                // its proof before reporting a runtime failure. Treat that
                // final combined-commit failure as an exact callback failure,
                // rather than leaving a retryable token whose collector has
                // already been consumed.
                let result = match self
                    .session
                    .commit_prepared_required_callback_transaction(batch, prepared, order_root)
                {
                    Ok(result) => result,
                    Err(error) => {
                        self.callback_membership_transaction = None;
                        self.pending_callback_phase = None;
                        let _ = self.session.fail_required_callback_phase(token);
                        return Err(AuthoringFailure::from(error));
                    }
                };
                if !collector.admitted_provisionals.is_empty() {
                    let nodes = collector
                        .admitted_provisionals
                        .into_iter()
                        .map(|local| {
                            let node = result.resolve(local).expect(
                                "a committed callback provisional object must resolve its exact token",
                            );
                            (local, node)
                        })
                        .collect();
                    self.committed_callback_provisionals =
                        Some(CommittedCallbackProvisionals { token, nodes });
                }
            } else {
                if let Some((target, content)) = content {
                    self.session
                        .commit_required_callback_phase_with_owned_content(batch, target, content)
                        .map_err(AuthoringFailure::from)?;
                } else {
                    self.session
                        .commit_required_callback_phase(batch)
                        .map_err(AuthoringFailure::from)?;
                }
            }
        }
        #[cfg(not(any(target_arch = "wasm32", test)))]
        if let Some((target, content)) = content {
            self.session
                .commit_required_callback_phase_with_owned_content(batch, target, content)
                .map_err(AuthoringFailure::from)?;
        } else {
            self.session
                .commit_required_callback_phase(batch)
                .map_err(AuthoringFailure::from)?;
        }
        // The callback phase time is session-owned. Re-anchoring presentation
        // only after its commit avoids a host-side progression cursor.
        self.clock.seek(time).map_err(|error| error.to_string())?;
        self.pending_callback_phase = None;
        Ok(None)
    }

    pub fn fail_callback_phase_json(&mut self, phase_json: &str) -> Result<(), AuthoringFailure> {
        let token = Self::phase_token_from_json(phase_json)?;
        self.session
            .fail_required_callback_phase(token)
            .map_err(AuthoringFailure::from)?;
        #[cfg(any(target_arch = "wasm32", test))]
        {
            self.callback_membership_transaction = None;
        }
        self.pending_callback_phase = None;
        Ok(())
    }

    pub fn interrupt_callback_phase_json(
        &mut self,
        phase_json: &str,
    ) -> Result<(), AuthoringFailure> {
        let token = Self::phase_token_from_json(phase_json)?;
        self.session
            .interrupt_required_callback_phase(token)
            .map_err(AuthoringFailure::from)?;
        #[cfg(any(target_arch = "wasm32", test))]
        {
            self.callback_membership_transaction = None;
        }
        self.pending_callback_phase = None;
        Ok(())
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
impl SemanticExecutionPlayer {
    fn callback_phase_json(
        overlay: &noon::integration::CallbackPhaseOverlay,
        invocations: &[noon::integration::RequiredCallbackInvocation],
    ) -> Result<String, String> {
        let phase = CallbackPhaseWire {
            token: overlay.token().into(),
            region: overlay.region(),
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

    #[cfg(any(target_arch = "wasm32", test))]
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
            self.session
                .advance_interactions(timestamp_ms / 1_000.0)
                .map_err(|error| error.to_string())?;
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
    #[cfg(target_arch = "wasm32")]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = liveSegmentWake))]
    pub fn live_segment_wake_wasm(
        &mut self,
        wall_time_ms: f64,
    ) -> Result<WasmExecutionWake, wasm_bindgen::JsValue> {
        self.live_segment_wake(wall_time_ms)
            .map_err(crate::authoring_error::js_error)
    }

    /// Derive the next generic browser wake from the canonical runtime/session.
    #[cfg(any(target_arch = "wasm32", test))]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = executionWake))]
    pub fn execution_wake_wasm(&mut self, wall_time_ms: f64) -> Result<WasmExecutionWake, String> {
        self.execution_wake(wall_time_ms)
    }

    /// Current playback position during a static wait, without a new runtime frame.
    #[cfg(any(target_arch = "wasm32", test))]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = playbackTimeAt))]
    pub fn playback_time_at_wasm(&self, wall_time_ms: f64) -> Result<f64, String> {
        self.playback_time_at(wall_time_ms)
    }

    /// Reanchor the next browser interval after a required callback completes.
    #[cfg(target_arch = "wasm32")]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = reanchorLiveSegmentWake))]
    pub fn reanchor_live_segment_wake_wasm(
        &mut self,
        wall_time_ms: f64,
    ) -> Result<WasmExecutionWake, wasm_bindgen::JsValue> {
        self.reanchor_live_segment_wake(wall_time_ms)
            .map_err(crate::authoring_error::js_error)
    }

    /// Advance one active ordinary continuation segment from an anchored browser timestamp.
    ///
    /// A callback phase must be committed before this is retried with the same wall
    /// timestamp. `reachedEndpoint` means shared completion is now permitted but
    /// remains a separate operation so authored reconciliation cannot be skipped.
    #[cfg(target_arch = "wasm32")]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = driveLiveSegmentFromWallTime))]
    pub fn drive_live_segment_from_wall_time_wasm(
        &mut self,
        wall_time_ms: f64,
    ) -> Result<WasmLiveSegmentDrive, wasm_bindgen::JsValue> {
        self.live_drive_segment_from_wall_time(wall_time_ms)
            .map_err(crate::authoring_error::js_error)
    }

    /// Advance one active continuation segment toward an absolute authored-time
    /// sample without involving a browser clock.
    #[cfg(target_arch = "wasm32")]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = driveLiveSegmentToAuthoredTime))]
    pub fn drive_live_segment_to_authored_time_wasm(
        &mut self,
        requested_time: f64,
    ) -> Result<WasmLiveSegmentDrive, wasm_bindgen::JsValue> {
        self.live_drive_segment_to_authored_time(requested_time)
            .map_err(crate::authoring_error::js_error)
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen::prelude::wasm_bindgen(js_name = completeLiveSegment)]
    pub fn complete_live_segment_wasm(&mut self) -> Result<(), wasm_bindgen::JsValue> {
        self.live_complete_segment()
            .map_err(crate::authoring_error::js_error)
    }

    /// Read one typed value from the exact pending callback phase without
    /// committing it. This is the real Python-worker boundary; direct Rust
    /// callbacks call the session API without JSON.
    #[cfg(target_arch = "wasm32")]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = requiredCallbackReadJson))]
    pub fn required_callback_read_json_wasm(
        &mut self,
        token_json: &str,
        request_json: &str,
    ) -> Result<String, wasm_bindgen::JsValue> {
        self.required_callback_read_json(token_json, request_json)
            .map_err(crate::authoring_error::js_error)
    }

    /// Extend the current required callback's one shared authored transaction
    /// with an inert, original-handle membership request. This only stages;
    /// `commitCallbackPhaseJson` remains the sole callback publication.
    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen::prelude::wasm_bindgen(js_name = stageCallbackMembership)]
    pub fn stage_callback_membership_wasm(
        &mut self,
        token_json: &str,
        batch: crate::canonical_authoring_scene::WasmSceneMembershipBatch,
    ) -> Result<(), wasm_bindgen::JsValue> {
        let batch = batch.into_callback_batch();
        let token =
            Self::callback_token_from_json(token_json).map_err(crate::authoring_error::js_error)?;
        self.stage_required_callback_membership(token, &batch)
            .map_err(crate::authoring_error::js_error)
    }

    /// Read the pending callback's staged direct-root membership in painter
    /// order. This is a transaction-local Rust read, never a Python mirror.
    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen::prelude::wasm_bindgen(js_name = callbackMembershipRootKeys)]
    pub fn callback_membership_root_keys_wasm(
        &mut self,
        token_json: &str,
    ) -> Result<Vec<String>, wasm_bindgen::JsValue> {
        let token =
            Self::callback_token_from_json(token_json).map_err(crate::authoring_error::js_error)?;
        self.callback_membership_root_keys(token)
            .map_err(crate::authoring_error::js_error)
    }

    /// Stage one provisional geometry declaration in the exact pending callback
    /// transaction. Inline and retained-path constructors remain phase-local until
    /// the callback's final shared publication.
    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen::prelude::wasm_bindgen(js_name = stageCallbackProvisionalGeometry)]
    pub fn stage_callback_provisional_geometry_wasm(
        &mut self,
        token_json: &str,
        options: crate::WasmManimGeometryOptions,
    ) -> Result<WasmCallbackProvisionalMobject, wasm_bindgen::JsValue> {
        let token =
            Self::callback_token_from_json(token_json).map_err(crate::authoring_error::js_error)?;
        let local = self
            .stage_required_callback_provisional_geometry(token, options.options)
            .map_err(crate::authoring_error::js_error)?;
        Ok(WasmCallbackProvisionalMobject {
            callback_token: token,
            local,
        })
    }

    /// Apply one authored translation while the object still has only a
    /// callback-local name. This is typed construction staging, not an
    /// execution-frame property write.
    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen::prelude::wasm_bindgen(js_name = stageCallbackProvisionalShift)]
    pub fn stage_callback_provisional_shift_wasm(
        &mut self,
        token_json: &str,
        object: &WasmCallbackProvisionalMobject,
        x: f64,
        y: f64,
    ) -> Result<(), wasm_bindgen::JsValue> {
        let token =
            Self::callback_token_from_json(token_json).map_err(crate::authoring_error::js_error)?;
        if object.callback_token != token {
            return Err(crate::authoring_error::js_error(
                "callback provisional geometry token is stale",
            ));
        }
        self.stage_required_callback_provisional_shift(token, object.local, x, y)
            .map_err(crate::authoring_error::js_error)
    }

    /// Apply one authored fill while the object still has only a callback-local
    /// name. The final callback publication materializes this style with the
    /// object declaration in one semantic transaction.
    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen::prelude::wasm_bindgen(js_name = stageCallbackProvisionalFill)]
    pub fn stage_callback_provisional_fill_wasm(
        &mut self,
        token_json: &str,
        object: &WasmCallbackProvisionalMobject,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        opacity: Option<f64>,
    ) -> Result<(), wasm_bindgen::JsValue> {
        let token =
            Self::callback_token_from_json(token_json).map_err(crate::authoring_error::js_error)?;
        if object.callback_token != token {
            return Err(crate::authoring_error::js_error(
                "callback provisional geometry token is stale",
            ));
        }
        self.stage_required_callback_provisional_fill(
            token,
            object.local,
            [red, green, blue, alpha],
            opacity,
        )
        .map_err(crate::authoring_error::js_error)
    }

    /// Read a phase-local center from the prepared declaration. This returns
    /// coordinates only and never manufactures a permanent semantic identity.
    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen::prelude::wasm_bindgen(js_name = callbackProvisionalCenter)]
    pub fn callback_provisional_center_wasm(
        &mut self,
        token_json: &str,
        object: &WasmCallbackProvisionalMobject,
    ) -> Result<WasmCallbackProvisionalPoint, wasm_bindgen::JsValue> {
        let token =
            Self::callback_token_from_json(token_json).map_err(crate::authoring_error::js_error)?;
        if object.callback_token != token {
            return Err(crate::authoring_error::js_error(
                "callback provisional geometry token is stale",
            ));
        }
        let (x, y) = self
            .callback_provisional_center(token, object.local)
            .map_err(crate::authoring_error::js_error)?;
        Ok(WasmCallbackProvisionalPoint { x, y })
    }

    /// Associate delayed Python wrapper bindings with callback-published
    /// Mobjects through the still-leased canonical context. This consumes only
    /// inert add binding reservations and never replays semantic membership.
    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen::prelude::wasm_bindgen(js_name = associatePublishedCallbackMobjects)]
    pub fn associate_published_callback_mobjects_wasm(
        &self,
        context: &mut crate::CanonicalAuthoringSceneContext,
        batch: crate::WasmSceneMembershipBatch,
    ) -> Result<(), wasm_bindgen::JsValue> {
        context
            .inner
            .associate_published_callback_mobjects(
                self,
                batch
                    .into_inner()
                    .map_err(crate::authoring_error::js_error)?,
            )
            .map_err(crate::authoring_error::js_error)
    }

    /// Redeem a callback-local name only after that exact callback committed.
    /// The returned ordinary typed handle comes from the original store and can
    /// be bound by Python's delayed wrapper finalizer.
    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen::prelude::wasm_bindgen(js_name = resolveCallbackProvisionalMobject)]
    pub fn resolve_callback_provisional_mobject_wasm(
        &mut self,
        token_json: &str,
        object: &WasmCallbackProvisionalMobject,
    ) -> Result<crate::WasmAuthoringMobjectHandle, wasm_bindgen::JsValue> {
        let token =
            Self::callback_token_from_json(token_json).map_err(crate::authoring_error::js_error)?;
        if object.callback_token != token {
            return Err(crate::authoring_error::js_error(
                "callback provisional geometry token is stale",
            ));
        }
        let node = self
            .take_committed_callback_provisional(token, object.local)
            .map_err(crate::authoring_error::js_error)?;
        let store = self.semantics.clone().ok_or_else(|| {
            crate::authoring_error::js_error(
                "callback provisional geometry requires a live semantic store",
            )
        })?;
        noon::Mobject::from_node(store, node)
            .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(crate::authoring_error::js_error)
    }

    #[cfg(target_arch = "wasm32")]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = commitCallbackPhaseJson))]
    pub fn commit_callback_phase_json_wasm(
        &mut self,
        batch_json: &str,
    ) -> Result<Option<String>, wasm_bindgen::JsValue> {
        self.commit_callback_phase_json(batch_json)
            .map_err(crate::authoring_error::js_error)
    }

    #[cfg(target_arch = "wasm32")]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = failCallbackPhaseJson))]
    pub fn fail_callback_phase_json_wasm(
        &mut self,
        phase_json: &str,
    ) -> Result<(), wasm_bindgen::JsValue> {
        self.fail_callback_phase_json(phase_json)
            .map_err(crate::authoring_error::js_error)
    }

    #[cfg(target_arch = "wasm32")]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = interruptCallbackPhaseJson))]
    pub fn interrupt_callback_phase_json_wasm(
        &mut self,
        phase_json: &str,
    ) -> Result<(), wasm_bindgen::JsValue> {
        self.interrupt_callback_phase_json(phase_json)
            .map_err(crate::authoring_error::js_error)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = callbackTerminationJson))]
    pub fn callback_termination_json(&self) -> Result<Option<String>, String> {
        self.session
            .callback_termination()
            .map(|termination| {
                let kind = match termination.kind() {
                    noon::integration::CallbackTerminationKind::Failed => "failed",
                    noon::integration::CallbackTerminationKind::Interrupted => "interrupted",
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

    /// Configure session/editor fill selection, never an authored interaction binding.
    /// Configuration is ordered with native input and is not replayed into a new scene.
    #[cfg(any(target_arch = "wasm32", test))]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = setPointerFillSelection))]
    pub fn set_pointer_fill_selection(&mut self, max_movement: Option<f32>) -> Result<(), String> {
        match max_movement {
            Some(value) => self.session.enable_pointer_fill_selection(value),
            None => self.session.disable_pointer_fill_selection(),
        }
        .map_err(|error| error.to_string())
    }

    /// Decode one contextual browser pointer occurrence at the genuine worker
    /// control-port boundary. Coordinates are CSS pixels relative to the content
    /// viewport. The immutable collection-time receipt selects the issued shared
    /// snapshot; delayed input is never projected with a newer execution camera.
    #[cfg(any(target_arch = "wasm32", test))]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = submitBrowserPointerInputJson))]
    pub fn submit_browser_pointer_input_json(&mut self, json: &str) -> Result<bool, String> {
        let envelope: pointer_input::WorkerPointerInput = serde_json::from_str(json)
            .map_err(|error| format!("invalid browser pointer input JSON: {error}"))?;
        let Self {
            session,
            semantics,
            worker_pointer_presentation,
            browser_pointer_binding,
            next_native_event_sequence,
            ..
        } = self;
        let mut target = pointer_input::PlayerPointerTarget {
            session,
            semantics: semantics.as_ref(),
        };
        worker_pointer_presentation.submit(
            &mut target,
            browser_pointer_binding,
            next_native_event_sequence,
            envelope.input,
            envelope.presentation,
        )
    }

    /// Admit one collection-time inspection occurrence through the same owned
    /// session and retained presentation path as pointer input.
    #[cfg(any(target_arch = "wasm32", test))]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = scrollInspectionViewJson))]
    pub fn scroll_inspection_view_json(&mut self, json: &str) -> Result<Option<bool>, String> {
        let input = serde_json::from_str(json)
            .map_err(|error| format!("invalid inspection scroll JSON: {error}"))?;
        let Self {
            session,
            semantics,
            worker_pointer_presentation,
            browser_pointer_binding,
            ..
        } = self;
        let mut target = pointer_input::PlayerPointerTarget {
            session,
            semantics: semantics.as_ref(),
        };
        worker_pointer_presentation.scroll(&mut target, browser_pointer_binding, input)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = setBrowserPointerViewJson))]
    pub fn set_browser_pointer_view_json(&mut self, json: &str) -> Result<(), String> {
        let view =
            serde_json::from_str(json).map_err(|e| format!("invalid pointer view JSON: {e}"))?;
        let Self {
            session,
            semantics,
            worker_pointer_presentation,
            browser_pointer_binding,
            next_native_event_sequence,
            ..
        } = self;
        let mut target = pointer_input::PlayerPointerTarget {
            session,
            semantics: semantics.as_ref(),
        };
        worker_pointer_presentation.set_view(
            &mut target,
            browser_pointer_binding,
            next_native_event_sequence,
            view,
        )
    }

    #[cfg(any(target_arch = "wasm32", test))]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = notePointerPresentationJson))]
    pub fn note_pointer_presentation_json(&mut self, json: &str) -> Result<bool, String> {
        let receipt =
            serde_json::from_str(json).map_err(|e| format!("invalid pointer receipt JSON: {e}"))?;
        self.worker_pointer_presentation
            .note_presented(&mut self.session, receipt)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = invalidatePointerPresentationJson))]
    pub fn invalidate_pointer_presentation_json(&mut self, json: &str) -> Result<bool, String> {
        let receipt =
            serde_json::from_str(json).map_err(|e| format!("invalid pointer receipt JSON: {e}"))?;
        let Self {
            session,
            semantics,
            worker_pointer_presentation,
            browser_pointer_binding,
            next_native_event_sequence,
            ..
        } = self;
        let mut target = pointer_input::PlayerPointerTarget {
            session,
            semantics: semantics.as_ref(),
        };
        worker_pointer_presentation.invalidate(
            &mut target,
            browser_pointer_binding,
            next_native_event_sequence,
            receipt,
        )
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
            noon::integration::CallbackRendererObservationOutcome::Committed(observation) => {
                observation
            }
            noon::integration::CallbackRendererObservationOutcome::StaleCallback { .. } => {
                return Err("callback renderer observation token is stale".into());
            }
            noon::integration::CallbackRendererObservationOutcome::StalePublication { .. } => {
                return Err("callback renderer observation publication is stale".into());
            }
            noon::integration::CallbackRendererObservationOutcome::Absent { .. } => {
                return Err("callback renderer observation target is absent".into());
            }
        };
        let delta = self
            .delta(false)?
            .ok_or("callback commit produced no retained renderer publication")?;
        let observation = RendererObservationRequest::from_callback_publication(
            delta.retained.session,
            delta.retained.sequence,
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

    /// Validate completed replay in Rust, before the host exposes seeking/looping.
    /// A failed capability leaves the final coherent frame and semantic scene intact.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = sealReplay))]
    pub fn seal_replay(&mut self) -> Result<(), String> {
        if self.session.replay_scope_active() {
            self.session
                .seal_replay()
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    /// Explicit read-only diagnostics; never used to drive or reconstruct execution.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = debugFrameJson))]
    pub fn debug_frame_json(&self) -> String {
        noon::diagnostics::execution_frame_value(&self.session).to_string()
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
        self.session
            .advance_interactions(timestamp_ms / 1_000.0)
            .map_err(|error| error.to_string())?;
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
mod tests;

#[cfg(test)]
mod callback_error_tests;
#[cfg(test)]
mod graph_transport_tests;
