//! Finite audio declarations through the canonical Scene owner.

use super::CanonicalAuthoringScene;
use crate::{authoring_audio::AuthoringAudioClipHandle, authoring_error::AuthoringFailure};
use noon::{AudioClipOptions, AudioResource, PlaybackControl};
use std::rc::Rc;

impl CanonicalAuthoringScene {
    fn require_pre_execution_media_authoring(&self) -> Result<(), AuthoringFailure> {
        if !self.player_ownership.is_unstarted() {
            return Err(AuthoringFailure::new(
                "unsupported_operation",
                "authoring.finite_media_after_execution",
                "audio and playback-control declarations must precede execution",
            ));
        }
        Ok(())
    }

    pub(crate) fn audio(
        &mut self,
        source: &AudioResource,
        options: AudioClipOptions,
    ) -> Result<AuthoringAudioClipHandle, AuthoringFailure> {
        self.require_pre_execution_media_authoring()?;
        let node = self.scene.audio(source.clone(), options)?;
        Ok(AuthoringAudioClipHandle {
            store: Rc::clone(self.scene.integration_store()),
            node,
        })
    }

    pub(crate) fn on_click_playback(
        &mut self,
        target: &noon::Mobject,
        action: PlaybackControl,
    ) -> Result<(), AuthoringFailure> {
        self.require_pre_execution_media_authoring()?;
        self.scene
            .on_click_playback(target, action)
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authoring_audio::pcm_resource;
    use noon::{RateFunction, SectionType, SemanticVec3};
    use noon_core::{ObjectId, SemanticMutationTransaction};

    #[test]
    fn finite_clip_uses_shared_identity_options_pcm_and_cursor() {
        let mut context = CanonicalAuthoringScene::default();
        context.authored_wait(2.0).unwrap();
        let source = pcm_resource(vec![0.25; 8_000], 8_000, 1).unwrap();
        let clip = context
            .audio(
                &source,
                AudioClipOptions {
                    start_time: 0.5,
                    source_start_frame: 2_000,
                    source_end_frame: Some(6_000),
                    duration: Some(2.0),
                    looped: true,
                    gain: 0.4,
                    fade_in: 0.25,
                    fade_out: 0.5,
                },
            )
            .unwrap();
        let definition = clip.definition().unwrap();
        assert_eq!(context.scene.time(), 2.0);
        assert_eq!(definition.start_time(), 0.5);
        assert_eq!(definition.end_time(), 2.5);
        assert_eq!(definition.source_start_frame(), 2_000);
        assert_eq!(definition.source_end_frame(), 6_000);
        assert_eq!(definition.gain(), 0.4);
        assert_eq!(definition.fade_in(), 0.25);
        assert_eq!(definition.fade_out(), 0.5);
        assert!(definition.is_looped());
        let store = context.scene.integration_store().borrow();
        assert_eq!(
            store.semantic_audio_clip_state(clip.node).unwrap().scope(),
            context.scene.root()
        );
        assert_eq!(
            store
                .audio_resources()
                .get(definition.resource())
                .unwrap()
                .samples()
                .as_ptr(),
            source.samples().as_ptr()
        );
    }

    #[test]
    fn failed_audio_admission_preserves_the_scene_and_invalidated_handles_stay_stale() {
        let mut context = CanonicalAuthoringScene::default();
        let source = pcm_resource(vec![0.25; 8_000], 8_000, 1).unwrap();
        let before = (
            context.scene.revision(),
            context
                .scene
                .integration_store()
                .borrow()
                .audio_resources()
                .stats(),
        );
        assert!(context
            .audio(
                &source,
                AudioClipOptions {
                    fade_in: 2.0,
                    ..AudioClipOptions::default()
                }
            )
            .is_err());
        assert_eq!(
            before,
            (
                context.scene.revision(),
                context
                    .scene
                    .integration_store()
                    .borrow()
                    .audio_resources()
                    .stats(),
            )
        );
        let clip = context.audio(&source, AudioClipOptions::default()).unwrap();
        let alias = clip.clone();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.remove_node(clip.node);
        transaction
            .apply(&mut context.scene.integration_store().borrow_mut())
            .unwrap();
        assert!(clip.definition().is_err());
        assert!(alias.definition().is_err());
    }

    #[test]
    fn scalar_sections_and_audio_use_the_same_predeclared_authored_time() {
        let mut context = CanonicalAuthoringScene::default();
        let dot = context.scene.circle(0.2).unwrap();
        context.bind_mobject(ObjectId::new(0), &dot).unwrap();
        let tracker = context.create_value_tracker(0.0).unwrap();
        let position = context
            .tracker_position(
                &tracker,
                SemanticVec3::new(1.0, 0.0, 0.0),
                SemanticVec3::new(-2.0, 0.0, 0.0),
            )
            .unwrap();
        context.bind_tracker_position(&dot, &position).unwrap();
        context
            .scene
            .next_section("Begin", SectionType::Normal)
            .unwrap();
        context
            .declare_value(&tracker, 2.0, 1.0, RateFunction::Linear)
            .unwrap();
        context
            .scene
            .next_section("Return", SectionType::Normal)
            .unwrap();
        context
            .declare_value(&tracker, 0.0, 1.0, RateFunction::Linear)
            .unwrap();
        context.authored_wait(0.5).unwrap();
        let source = pcm_resource(vec![0.25; 8_000], 8_000, 1).unwrap();
        context
            .audio(
                &source,
                AudioClipOptions {
                    start_time: 0.25,
                    duration: Some(2.0),
                    looped: true,
                    ..AudioClipOptions::default()
                },
            )
            .unwrap();
        assert_eq!(context.authored_duration(), 2.5);
        assert_eq!(
            context
                .scene
                .sections()
                .iter()
                .map(|section| section.time)
                .collect::<Vec<_>>(),
            [0.0, 1.0]
        );
        assert!(context.player_ownership.is_unstarted());
        let mut session = context.lower_execution().unwrap();
        for (time, x, source_offset) in [(0.5, -1.0, 0.25), (1.5, -1.0, 0.25), (2.0, -2.0, 0.75)] {
            session.seek(time).unwrap();
            assert_eq!(session.frame().objects[0].transform.translation.x, x);
            assert_eq!(
                session
                    .audio_playback()
                    .unwrap()
                    .next()
                    .unwrap()
                    .source_offset,
                source_offset
            );
        }
        session.seek(2.5).unwrap();
        assert!(session.audio_playback().unwrap().next().is_none());
    }

    #[test]
    fn invalid_scalar_declaration_keeps_cursor_and_unrelated_semantics() {
        let mut context = CanonicalAuthoringScene::default();
        let tracker = context.create_value_tracker(1.0).unwrap();
        let foreign = CanonicalAuthoringScene::default()
            .create_value_tracker(0.0)
            .unwrap();
        for (target, duration) in [(f64::NAN, 1.0), (2.0, 0.0), (2.0, f64::INFINITY)] {
            let revision = context.scene.revision();
            assert!(context
                .declare_value(&tracker, target, duration, RateFunction::Linear)
                .is_err());
            assert_eq!(context.scene.revision(), revision);
            assert_eq!(context.scene.time(), 0.0);
        }
        assert!(context
            .declare_value(&foreign, 2.0, 1.0, RateFunction::Linear)
            .is_err());
        assert_eq!(context.scene.time(), 0.0);
    }

    #[test]
    fn active_transferred_and_returned_owners_reject_finite_declarations() {
        let mut context = CanonicalAuthoringScene::default();
        let dot = context.scene.circle(0.2).unwrap();
        context.bind_mobject(ObjectId::new(0), &dot).unwrap();
        let tracker = context.create_value_tracker(0.0).unwrap();
        let source = pcm_resource(vec![0.25; 8], 8_000, 1).unwrap();
        context.live_player(1.0).unwrap();
        let revision = context.scene.revision();
        let reject = |context: &mut CanonicalAuthoringScene| {
            assert!(context.audio(&source, AudioClipOptions::default()).is_err());
            assert!(context
                .declare_value(&tracker, 1.0, 1.0, RateFunction::Linear)
                .is_err());
            assert!(context
                .on_click_playback(&dot, PlaybackControl::Restart)
                .is_err());
            assert_eq!(context.scene.revision(), revision);
            assert!(context
                .scene
                .integration_store()
                .borrow()
                .audio_resources()
                .is_empty());
        };
        reject(&mut context);
        let player = context.take_execution_player(1.0, 17).unwrap();
        reject(&mut context);
        context.return_execution_player(player).unwrap();
        reject(&mut context);
    }

    #[test]
    fn lesson_binding_entrypoints_match_first_class_rust_frames_and_audio() {
        use noon::example_scenes::background_music_lesson as lesson;
        use noon_core::{ObjectContentRef, TextResourceLookup};

        // Real Rust execution on both sides. The common static illustration
        // isolates the wrapper operations being qualified here; the executable
        // Python pair is exercised separately in the actual Pyodide host.
        let native = lesson::scene().unwrap();
        let mut context = CanonicalAuthoringScene::default();
        let [input, output] = lesson::illustration(&mut context.scene).unwrap();
        let tracker = context.create_value_tracker(0.0).unwrap();
        for (index, (point, direction, y)) in [
            (&input, 1.0, lesson::INPUT_Y),
            (&output, 2.0, lesson::OUTPUT_Y),
        ]
        .into_iter()
        .enumerate()
        {
            context
                .bind_mobject(ObjectId::new(index as u64), point)
                .unwrap();
            let position = context
                .tracker_position(
                    &tracker,
                    SemanticVec3::new(direction, 0.0, 0.0),
                    SemanticVec3::new(lesson::ORIGIN_X, y, 0.0),
                )
                .unwrap();
            context.bind_tracker_position(point, &position).unwrap();
        }
        for (name, target) in lesson::SECTIONS {
            context
                .scene
                .next_section(name, SectionType::Normal)
                .unwrap();
            context.authored_wait(1.0).unwrap();
            context
                .declare_value(&tracker, target, 8.0, RateFunction::Linear)
                .unwrap();
            context.authored_wait(1.0).unwrap();
        }
        context
            .audio(
                &crate::authoring_audio::small_discovery().unwrap(),
                AudioClipOptions {
                    duration: Some(30.0),
                    gain: 0.5,
                    fade_in: 1.5,
                    fade_out: 2.5,
                    ..AudioClipOptions::default()
                },
            )
            .unwrap();
        assert_eq!(native.sections(), context.scene.sections());
        assert_eq!(native.time(), context.authored_duration());
        let mut native = native.execution_session().unwrap();
        let mut wrapped = context.lower_execution().unwrap();
        assert_eq!(native.finite_timeline_end_time(), Some(30.0));
        assert_eq!(wrapped.finite_timeline_end_time(), Some(30.0));
        for time in [
            0.0, 0.75, 5.0, 9.0, 10.0, 13.0, 15.0, 19.0, 20.0, 25.0, 28.75, 30.0,
        ] {
            native.advance_to(time).unwrap();
            wrapped.seek(time).unwrap();
            assert_eq!(native.frame().objects.len(), wrapped.frame().objects.len());
            for (direct, through_binding) in
                native.frame().objects.iter().zip(&wrapped.frame().objects)
            {
                // Separate stores deliberately have separate resource arenas.
                // Compare actual immutable shaped text before aligning only the
                // observation's handle for the remaining frame-row comparison.
                let mut observed = through_binding.clone();
                if let (ObjectContentRef::Text(left), ObjectContentRef::Text(right)) =
                    (&direct.content, &observed.content)
                {
                    assert_eq!(
                        native.text_resources().get(*left),
                        wrapped.text_resources().get(*right)
                    );
                    observed.content = direct.content.clone();
                }
                assert_eq!(*direct, observed, "lesson frame row at {time}");
            }
            let native_audio = native.audio_playback().unwrap().collect::<Vec<_>>();
            let wrapped_audio = wrapped.audio_playback().unwrap().collect::<Vec<_>>();
            assert_eq!(native_audio.len(), wrapped_audio.len());
            for (direct, through_binding) in native_audio.iter().zip(wrapped_audio.iter()) {
                let authored = |definition: &noon::AudioClipDefinition| {
                    (
                        definition.start_time(),
                        definition.end_time(),
                        definition.duration(),
                        definition.source_start_frame(),
                        definition.source_end_frame(),
                        definition.is_looped(),
                        definition.gain(),
                        definition.fade_in(),
                        definition.fade_out(),
                    )
                };
                assert_eq!(
                    authored(direct.definition),
                    authored(through_binding.definition)
                );
                assert_eq!(direct.start_delay, through_binding.start_delay);
                assert_eq!(direct.source_offset, through_binding.source_offset);
                assert_eq!(direct.duration, through_binding.duration);
                assert_eq!(direct.loop_region, through_binding.loop_region);
                assert_eq!(direct.gain_points, through_binding.gain_points);
                assert_eq!(direct.pcm.sample_rate(), through_binding.pcm.sample_rate());
                assert_eq!(direct.pcm.channels(), through_binding.pcm.channels());
                if time == 0.0 {
                    assert_eq!(direct.pcm.samples(), through_binding.pcm.samples());
                }
            }
        }
    }
}
