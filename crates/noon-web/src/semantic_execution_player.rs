//! Transport adapter for an already-lowered semantic session; never parses authoring JSON.
use noon::ExecutionSession;

use crate::{ExecutionDeltaEncoder, ExecutionDeltaEnvelope, PlaybackClock};

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub struct SemanticExecutionPlayer {
    session: ExecutionSession,
    clock: PlaybackClock,
    encoder: ExecutionDeltaEncoder,
    /// Present when the player came from canonical authoring. This is the one
    /// semantic store that produced `session`, not an execution mirror.
    #[cfg(any(target_arch = "wasm32", test))]
    semantics: Option<std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>>,
}

impl SemanticExecutionPlayer {
    pub fn from_session(
        session: ExecutionSession,
        duration: f64,
        transport_session: u32,
    ) -> Result<Self, String> {
        Ok(Self {
            session,
            clock: PlaybackClock::looping(duration).map_err(|e| e.to_string())?,
            encoder: ExecutionDeltaEncoder::new(transport_session),
            #[cfg(any(target_arch = "wasm32", test))]
            semantics: None,
        })
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn from_live_session(
        session: ExecutionSession,
        semantics: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        duration: f64,
        transport_session: u32,
    ) -> Result<Self, String> {
        Ok(Self {
            session,
            clock: PlaybackClock::looping(duration).map_err(|e| e.to_string())?,
            encoder: ExecutionDeltaEncoder::new(transport_session),
            semantics: Some(semantics),
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
        self.encoder = ExecutionDeltaEncoder::new(transport_session);
        Ok(())
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
        noon::LiveSession::new(&semantics, &mut self.session)
            .set_translation(mobject, x, y)
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
        noon::LiveSession::new(&semantics, &mut self.session)
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
        noon::LiveSession::new(&semantics, &mut self.session)
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
        noon::LiveSession::new(&semantics, &mut self.session)
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
        noon::LiveSession::new(&semantics, &mut self.session)
            .effective(mobject)
            .map_err(|error| error.to_string())
    }

    #[cfg(test)]
    pub(crate) fn session_mut_for_test(&mut self) -> &mut ExecutionSession {
        &mut self.session
    }

    fn delta(&mut self, snapshot: bool) -> Result<Option<ExecutionDeltaEnvelope>, String> {
        let camera = self.session.camera().map_err(|e| e.to_string())?;
        let changes = self.session.take_frame_changes();
        if snapshot || changes.is_all() || !self.encoder.is_initialized() {
            let slots = (0..self.session.frame().objects.len())
                .filter_map(|index| {
                    self.session
                        .execution_slot_for_frame_index(index)
                        .map(|slot| (slot, index))
                })
                .collect::<Vec<_>>();
            self.encoder
                .encode_snapshot_with_camera(self.session.frame(), &slots, camera)
                .map(Some)
                .map_err(|e| e.to_string())
        } else {
            let slots = changes
                .object_indices()
                .iter()
                .filter_map(|&index| {
                    self.session
                        .execution_slot_for_frame_index(index)
                        .map(|slot| (slot, index))
                })
                .collect::<Vec<_>>();
            self.encoder
                .encode_incremental_with_camera(self.session.frame(), &slots, &[], &[], camera)
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
    use crate::ExecutionFrameMirror;
    use noon_core::{AnimationOptions, RateFunction};

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
        let mut mirror = ExecutionFrameMirror::default();
        let initial: ExecutionDeltaEnvelope =
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
        let halfway: ExecutionDeltaEnvelope =
            serde_json::from_str(&player.tick_delta_json(500.0).unwrap().unwrap()).unwrap();
        assert!(!halfway.snapshot);
        assert_eq!(halfway.objects.len(), 1);
        assert_eq!(halfway.objects[0].transform.translation.x, 4.0);
        mirror.apply(halfway).unwrap();
        assert_eq!(
            mirror.frame().unwrap().objects[0].transform.translation.x,
            4.0
        );
        let end: ExecutionDeltaEnvelope =
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
        let delta: ExecutionDeltaEnvelope =
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
}
