//! Transport adapter for an already-lowered semantic session; never parses authoring JSON.
use noon::ExecutionSession;

use crate::{
    PlaybackClock, RetainedExecutionDeltaEncoder, RetainedExecutionDeltaEnvelope,
    RetainedResourceBundle,
};

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub struct SemanticExecutionPlayer {
    session: ExecutionSession,
    clock: PlaybackClock,
    encoder: RetainedExecutionDeltaEncoder,
    /// Immutable text/font/vector dependencies transferred once at the genuine
    /// authoring-worker to render-worker boundary.
    resource_bundle: Vec<u8>,
    snapshot_sent: bool,
    /// Present when the player came from canonical authoring. This is the one
    /// semantic store that produced `session`, not an execution mirror.
    #[cfg(any(target_arch = "wasm32", test))]
    semantics: Option<std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>>,
    #[cfg(any(target_arch = "wasm32", test))]
    semantic_root: Option<noon_core::SemanticNodeId>,
    /// Continuation metadata for this one session-owned runtime, never a
    /// frontend scheduler or animation state mirror.
    #[cfg(any(target_arch = "wasm32", test))]
    live_segment: Option<noon::ExecutionSegment>,
}

impl SemanticExecutionPlayer {
    pub fn from_session(
        session: ExecutionSession,
        duration: f64,
        transport_session: u32,
    ) -> Result<Self, String> {
        let resource_bundle = Self::resource_bundle_for(&session)?;
        Ok(Self {
            session,
            clock: PlaybackClock::looping(duration).map_err(|e| e.to_string())?,
            encoder: RetainedExecutionDeltaEncoder::new(transport_session),
            resource_bundle,
            snapshot_sent: false,
            #[cfg(any(target_arch = "wasm32", test))]
            semantics: None,
            #[cfg(any(target_arch = "wasm32", test))]
            semantic_root: None,
            #[cfg(any(target_arch = "wasm32", test))]
            live_segment: None,
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
        let resource_bundle = Self::resource_bundle_for(&session)?;
        Ok(Self {
            session,
            clock: PlaybackClock::looping(duration).map_err(|e| e.to_string())?,
            encoder: RetainedExecutionDeltaEncoder::new(transport_session),
            resource_bundle,
            snapshot_sent: false,
            semantics: Some(semantics),
            semantic_root: Some(semantic_root),
            live_segment: None,
        })
    }

    /// Change only derived transport framing while retaining the same runtime.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn rebind_transport(
        &mut self,
        duration: f64,
        transport_session: u32,
    ) -> Result<(), String> {
        self.clock
            .set_loop_duration(duration)
            .map_err(|error| error.to_string())?;
        // Live publication may have installed sparse text/font dependencies after
        // this player was bootstrapped. Refresh only at the explicit cross-worker
        // handoff boundary so ordinary typed in-process property edits stay local.
        self.resource_bundle = Self::resource_bundle_for(&self.session)?;
        self.encoder = RetainedExecutionDeltaEncoder::new(transport_session);
        self.snapshot_sent = false;
        Ok(())
    }

    /// The authored duration needed to hand this live session to presentation.
    ///
    /// The current frame is authoritative once a segment completes. An active
    /// continuation must also keep its endpoint addressable before it completes.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_handoff_duration(&self) -> Option<f64> {
        self.semantics.as_ref()?;
        Some(
            self.live_segment
                .map_or(self.session.frame().time, |segment| {
                    self.session.frame().time.max(segment.end_time())
                }),
        )
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

    #[cfg(target_arch = "wasm32")]
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
        if let Some(segment) = self.live_segment {
            if !self.session.segment_state(segment).is_complete() {
                return Err(
                    "advance the current live segment to its endpoint before continuing".into(),
                );
            }
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
        self.live_segment = Some(segment);
        Ok(end_time)
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
        self.live_segment = Some(segment);
        Ok(end_time)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_advance_segment_to(&mut self, requested_time: f64) -> Result<bool, String> {
        let segment = self
            .live_segment
            .ok_or("play an animation or wait before advancing a live segment")?;
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        let mut live = noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        );
        live.advance_segment_to(segment, requested_time)
            .map_err(|error| error.to_string())?;
        Ok(live.segment_state(segment).is_complete())
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
impl SemanticExecutionPlayer {
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
        let mut clock = self.clock.clone();
        clock.seek(time).map_err(|e| e.to_string())?;
        self.session.seek(time).map_err(|e| e.to_string())?;
        self.clock = clock;
        self.encoded_delta(false)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = setLoopDuration))]
    pub fn set_loop_duration(&mut self, duration: f64) -> Result<(), String> {
        self.clock
            .set_loop_duration(duration)
            .map_err(|e| e.to_string())
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
    use noon_core::{AnimationOptions, RateFunction};

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
