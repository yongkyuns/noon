use crate::{
    effects::{EffectRadius, EffectUpdate, Glow, Pixels},
    integration::{NativeInputModifiers, NativePointerInputKind, NativePointerPosition},
    AnimationOptions, Color, ExecutionSession, ExecutionSessionInputError, Mobject, Scene,
};
use noon_core::{NativePointerId, NativePointerInput, SemanticNodeId, Vec2};

struct Fixture {
    scene: Scene,
    session: ExecutionSession,
    targets: [Mobject; 2],
}

impl Fixture {
    fn new(persistent: Option<EffectUpdate>) -> Self {
        let mut scene = Scene::new();
        let mut first = scene.circle(0.75).unwrap();
        first.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
        first.disable_stroke().unwrap();
        if let Some(update) = persistent {
            first.set_glow(update).unwrap();
        }
        let mut second = scene.square(1.5).unwrap();
        second.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
        second.disable_stroke().unwrap();
        second.set_translation(4.0, 0.0).unwrap();
        scene
            .add_many(&[(&first).into(), (&second).into()])
            .unwrap();
        for target in [&first, &second] {
            let pulse = scene
                .declare_glow_pulse(target, 2.0, AnimationOptions::new())
                .unwrap();
            scene.on_click(target, Some(&pulse)).unwrap();
        }
        let mut session = scene.execution_session().unwrap();
        session
            .configure_native_pointer_input(
                NativePointerId {
                    source: 81,
                    pointer: 2,
                },
                1,
            )
            .unwrap();
        session.take_frame_changes();
        Self {
            scene,
            session,
            targets: [first, second],
        }
    }

    fn send(
        &mut self,
        sequence: u64,
        target: usize,
        press: bool,
    ) -> Result<(), ExecutionSessionInputError> {
        let token = self.session.native_pointer_input_token().unwrap();
        let position = NativePointerPosition::new(
            Vec2::new(target as f32 * 4.0, 0.0),
            Vec2::new(300.0 + target as f32 * 100.0, 150.0),
        )
        .unwrap();
        let kind = if press {
            NativePointerInputKind::Press {
                position,
                button: 0,
            }
        } else {
            NativePointerInputKind::Release {
                position,
                button: 0,
            }
        };
        let input = NativePointerInput::new(
            sequence,
            token.pointer(),
            token.context(),
            NativeInputModifiers::default(),
            kind,
        );
        self.session
            .submit_native_pointer_input(
                &mut self.scene.integration_store().borrow_mut(),
                &token,
                input,
            )
            .map(|_| ())
    }

    fn click(&mut self, sequence: u64, target: usize) {
        self.send(sequence, target, true).unwrap();
        self.send(sequence + 1, target, false).unwrap();
    }

    fn tick(&mut self, time: f64) {
        self.session
            .advance_interactions(&mut self.scene.integration_store().borrow_mut(), time)
            .unwrap();
    }

    fn effect(&self, target: usize) -> Option<SemanticNodeId> {
        self.scene
            .integration_store()
            .borrow()
            .effect_by_name(self.targets[target].node_id(), "glow")
            .unwrap()
    }

    fn glow(&self, target: usize) -> Option<Glow> {
        let object = self
            .session
            .execution_object_id(self.targets[target].node_id())
            .unwrap();
        self.session
            .frame()
            .objects
            .iter()
            .find(|row| row.id == object)
            .unwrap()
            .effect
            .as_ref()
            .map(|binding| binding.definition.as_glow().unwrap())
    }

    fn update(&mut self, target: usize, update: EffectUpdate) {
        self.scene
            .live(&mut self.session)
            .set_glow(&self.targets[target], update)
            .unwrap();
    }
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}

#[test]
fn click_pulse_rejects_non_glow_attachments_without_publishing_or_acknowledging_the_release() {
    for definition in [
        noon_core::EffectDefinition::Blur(crate::effects::Blur::default()),
        noon_core::EffectDefinition::Shadow(crate::effects::Shadow::default()),
    ] {
        let mut f = Fixture::new(None);
        f.scene
            .live(&mut f.session)
            .add_effect(&f.targets[0], definition, "glow")
            .unwrap();
        let attachment = f.effect(0).unwrap();
        f.send(1, 0, true).unwrap();
        let context = f.session.publication_context();
        let frame = f.session.frame().clone();
        let revision = f.scene.revision();
        assert!(matches!(
            f.send(2, 0, false),
            Err(ExecutionSessionInputError::Interaction(_))
        ));
        assert_eq!(f.session.publication_context(), context);
        assert_eq!(f.session.frame(), &frame);
        assert_eq!(f.scene.revision(), revision);
        assert_eq!(f.effect(0), Some(attachment));
        assert_eq!(
            f.scene
                .get_effect(&f.targets[0], "glow")
                .unwrap()
                .authored_definition()
                .unwrap(),
            definition
        );
        assert!(!f.session.interactions_active());
        f.scene
            .live(&mut f.session)
            .on_click(&f.targets[0], None)
            .unwrap();
        // The rejected release is still the next admitted input occurrence.
        f.send(2, 0, false).unwrap();
    }
}

#[test]
fn click_pulse_preserves_gradient_paint_in_both_attachment_lifetimes() {
    use crate::{GradientStop, LinearGradient};
    use std::sync::Arc;

    for persistent in [false, true] {
        let mut f = Fixture::new(persistent.then(|| EffectUpdate::default().intensity(0.35)));
        let gradient = LinearGradient::new(
            [0.0, 0.5],
            [1.0, 0.5],
            [
                GradientStop::new(0.0, Color::rgba(1.0, 0.0, 0.0, 0.4)).unwrap(),
                GradientStop::new(1.0, Color::rgba(0.0, 0.0, 1.0, 0.8)).unwrap(),
            ],
        )
        .unwrap();
        f.scene
            .live(&mut f.session)
            .set_fill_gradient(&f.targets[0], gradient, Some(0.6))
            .unwrap();
        let paint = Arc::clone(f.session.frame().objects[0].fill_gradient.as_ref().unwrap());
        let style = f.session.frame().objects[0].style;
        let original = f.glow(0);
        let attachment = f.effect(0);
        f.click(1, 0);
        for time in [10.0, 10.5, 11.0] {
            f.tick(time);
            let row = &f.session.frame().objects[0];
            assert!(Arc::ptr_eq(row.fill_gradient.as_ref().unwrap(), &paint));
            assert_eq!(row.style, style);
            assert_eq!(f.session.frame().time, 0.0);
            if time == 10.5 {
                close(f.glow(0).unwrap().intensity(), 2.0);
            }
        }
        assert_eq!(f.glow(0), original);
        assert_eq!(f.effect(0), attachment);
        assert!(!f.session.interactions_active());
    }
}

#[test]
fn click_glow_pulse_absent_attachment_enrolls_neutrally_and_retires_exactly() {
    let mut f = Fixture::new(None);
    let original = f.session.frame().objects.clone();
    let nodes = f.scene.integration_store().borrow().len();
    assert_eq!(f.effect(0), None);
    assert!(!f.session.has_replay_timeline_work());
    f.click(1, 0);
    let temporary = f
        .effect(0)
        .expect("admitted click enrolls one neutral attachment");
    assert_eq!(f.scene.integration_store().borrow().len(), nodes + 1);
    assert_eq!(f.glow(0).unwrap().intensity(), 0.0);
    assert!(f.session.interactions_active());
    f.tick(1.0);
    f.tick(1.5);
    close(f.glow(0).unwrap().intensity(), 2.0);
    assert_eq!(f.session.frame().time, 0.0);
    f.tick(2.0);
    assert!(!f.session.interactions_active());
    assert_eq!(f.effect(0), None);
    assert!(f
        .scene
        .integration_store()
        .borrow()
        .node(temporary)
        .is_none());
    assert_eq!(f.scene.integration_store().borrow().len(), nodes);
    assert_eq!(f.session.frame().objects, original);
    assert!(!f.session.has_replay_timeline_work());
}

#[test]
fn click_glow_pulse_captures_the_persistent_value_at_activation() {
    let mut f = Fixture::new(Some(
        EffectUpdate::default()
            .color(Color::RED)
            .radius(Pixels(3.0))
            .intensity(0.35),
    ));
    let original = f.effect(0).unwrap();
    // This edit follows registration and must become the captured baseline.
    f.update(0, EffectUpdate::default().intensity(1.2));
    let baseline = f.glow(0).unwrap();
    f.click(1, 0);
    f.tick(10.0);
    f.tick(10.5);
    close(f.glow(0).unwrap().intensity(), 2.0);
    f.tick(11.0);
    assert_eq!(f.effect(0), Some(original));
    assert_eq!(f.glow(0), Some(baseline));
    assert!(!f.session.interactions_active());
    assert_eq!(f.session.frame().time, 0.0);
}

#[test]
fn click_glow_pulses_ignore_active_repeats_and_finish_independently() {
    let mut f = Fixture::new(None);
    f.click(1, 0);
    let first = f.effect(0).unwrap();
    f.tick(1.0);
    f.tick(1.25);
    f.click(3, 0);
    assert_eq!(f.effect(0), Some(first));
    f.click(5, 1);
    let second = f.effect(1).unwrap();
    f.tick(1.25);
    f.tick(1.5);
    close(f.glow(0).unwrap().intensity(), 2.0);
    assert!(f.glow(1).unwrap().intensity() > 0.0);
    f.tick(2.0);
    assert_eq!(
        f.effect(0),
        None,
        "repeat must not extend the first deadline"
    );
    assert_eq!(f.effect(1), Some(second));
    assert!(f.session.interactions_active());
    f.tick(2.25);
    assert_eq!(f.effect(1), None);
    assert!(!f.session.interactions_active());
    f.click(7, 0);
    assert_ne!(
        f.effect(0),
        Some(first),
        "retired attachment generations are never reused"
    );
    f.tick(3.0);
    f.tick(4.0);
    assert_eq!(f.effect(0), None);
}

#[test]
fn click_glow_pulse_linear_and_reversed_timing_restore_at_the_endpoint() {
    for (reverse, expected) in [
        (false, [0.5, 0.875, 1.25, 1.625, 0.5]),
        (true, [2.0, 1.625, 1.25, 0.875, 0.5]),
    ] {
        let mut f = Fixture::new(Some(EffectUpdate::default().intensity(0.5)));
        let attachment = f.effect(0);
        f.scene
            .live(&mut f.session)
            .on_click_glow_pulse(
                &f.targets[0],
                2.0,
                AnimationOptions::new()
                    .rate_func(noon_core::RateFunction::Linear)
                    .reverse_rate_function(reverse),
            )
            .unwrap();
        f.click(1, 0);
        for (time, intensity) in [1.0, 1.25, 1.5, 1.75, 2.0].into_iter().zip(expected) {
            f.tick(time);
            close(f.glow(0).unwrap().intensity(), intensity);
            assert_eq!(f.effect(0), attachment);
            assert_eq!(f.session.frame().time, 0.0);
        }
        assert!(!f.session.interactions_active());
    }
}

#[test]
fn value_equal_radius_edit_adopts_a_temporary_click_attachment_without_stopping_the_pulse() {
    let mut f = Fixture::new(None);
    f.click(1, 0);
    let attachment = f.effect(0).unwrap();
    f.tick(1.0);
    f.tick(1.25);
    let radius = f.glow(0).unwrap().radius();
    f.update(0, EffectUpdate::default().radius(radius));
    assert_eq!(f.effect(0), Some(attachment));
    assert!(f.session.interactions_active());
    f.tick(1.5);
    close(f.glow(0).unwrap().intensity(), 2.0);
    f.tick(2.0);
    assert_eq!(f.effect(0), Some(attachment));
    assert_eq!(f.glow(0).unwrap().intensity(), 0.0);
    assert_eq!(f.glow(0).unwrap().radius(), radius);
    assert!(!f.session.interactions_active());
}

#[test]
fn click_glow_pulse_keeps_disjoint_color_and_radius_edits() {
    let mut f = Fixture::new(Some(
        EffectUpdate::default()
            .color(Color::RED)
            .radius(Pixels(3.0))
            .intensity(0.8),
    ));
    let attachment = f.effect(0).unwrap();
    f.click(1, 0);
    f.tick(1.0);
    f.tick(1.25);
    let intensity = f.glow(0).unwrap().intensity();
    f.update(
        0,
        EffectUpdate::default()
            .color(Color::BLUE)
            .radius(Pixels(7.0)),
    );
    close(f.glow(0).unwrap().intensity(), intensity);
    f.tick(1.5);
    let peak = f.glow(0).unwrap();
    close(peak.intensity(), 2.0);
    assert_eq!(peak.color(), Color::BLUE);
    assert_eq!(peak.radius(), EffectRadius::Pixels(7.0));
    f.tick(2.0);
    let restored = f.glow(0).unwrap();
    close(restored.intensity(), 0.8);
    assert_eq!(restored.color(), Color::BLUE);
    assert_eq!(restored.radius(), EffectRadius::Pixels(7.0));
    assert_eq!(f.effect(0), Some(attachment));
}

#[test]
fn explicit_intensity_write_adopts_and_supersedes_click_ownership_even_when_value_equal() {
    for (time, intensity) in [(1.0, 0.0), (1.5, 2.0)] {
        let mut f = Fixture::new(None);
        f.click(1, 0);
        let attachment = f.effect(0).unwrap();
        f.tick(1.0);
        f.tick(time);
        close(f.glow(0).unwrap().intensity(), intensity);
        f.update(0, EffectUpdate::default().intensity(intensity));
        f.tick(1.75);
        close(f.glow(0).unwrap().intensity(), intensity);
        f.tick(2.0);
        assert_eq!(f.effect(0), Some(attachment));
        close(f.glow(0).unwrap().intensity(), intensity);
        assert!(!f.session.interactions_active());
    }
}

#[test]
fn click_glow_pulse_failed_seek_is_atomic_and_successful_seek_retires_the_temporary_attachment() {
    let mut f = Fixture::new(None);
    f.click(1, 0);
    let attachment = f.effect(0).unwrap();
    f.tick(1.0);
    f.tick(1.5);
    let frame = f.session.frame().clone();
    let revision = f.scene.revision();
    let token = f.session.native_pointer_input_token().unwrap();
    assert!(
        f.session.seek(0.0).is_err(),
        "raw seek cannot bypass temporary attachment retirement"
    );
    assert_eq!(f.session.frame(), &frame);
    assert_eq!(f.scene.revision(), revision);
    assert_eq!(f.session.native_pointer_input_token().unwrap(), token);
    assert_eq!(f.effect(0), Some(attachment));
    assert!(f.session.interactions_active());
    assert!(f
        .session
        .seek_with_store(&mut f.scene.integration_store().borrow_mut(), f64::NAN)
        .is_err());
    assert_eq!(f.session.frame(), &frame);
    assert_eq!(f.scene.revision(), revision);
    assert_eq!(f.session.native_pointer_input_token().unwrap(), token);
    assert_eq!(f.effect(0), Some(attachment));
    assert!(f.session.interactions_active());
    f.session
        .seek_with_store(&mut f.scene.integration_store().borrow_mut(), 0.0)
        .unwrap();
    assert_eq!(f.effect(0), None);
    assert_eq!(f.glow(0), None);
    assert!(!f.session.interactions_active());
    assert!(f
        .scene
        .integration_store()
        .borrow()
        .node(attachment)
        .is_none());
}

#[test]
fn foreign_semantic_store_cannot_publish_click_input_or_advance_a_persistent_pulse() {
    let mut f = Fixture::new(Some(EffectUpdate::default().intensity(0.5)));
    let mut foreign = noon_core::SemanticStore::new();
    let foreign_revision = foreign.scene_revision();
    let revision = f.scene.revision();
    let token = f.session.native_pointer_input_token().unwrap();
    let position = NativePointerPosition::new(Vec2::ZERO, Vec2::new(300.0, 150.0)).unwrap();
    let input = NativePointerInput::new(
        1,
        token.pointer(),
        token.context(),
        NativeInputModifiers::default(),
        NativePointerInputKind::Press {
            position,
            button: 0,
        },
    );
    let frame = f.session.frame().clone();
    assert!(f
        .session
        .submit_native_pointer_input(&mut foreign, &token, input)
        .is_err());
    assert_eq!(f.session.frame(), &frame);
    assert_eq!(f.scene.revision(), revision);
    assert_eq!(foreign.scene_revision(), foreign_revision);
    assert_eq!(f.session.native_pointer_input_token().unwrap(), token);
    assert!(!f.session.interactions_active());

    // The same input sequence remains available, and the foreign capability
    // cannot advance even a pulse that needs no attachment retirement.
    f.click(1, 0);
    f.tick(1.0);
    f.tick(1.25);
    let frame = f.session.frame().clone();
    let attachment = f.effect(0);
    assert!(f.session.advance_interactions(&mut foreign, 1.5).is_err());
    assert_eq!(f.session.frame(), &frame);
    assert_eq!(f.scene.revision(), revision);
    assert_eq!(foreign.scene_revision(), foreign_revision);
    assert_eq!(f.effect(0), attachment);
    assert!(f.session.interactions_active());
    f.tick(1.5);
    close(f.glow(0).unwrap().intensity(), 2.0);
    f.tick(2.0);
    close(f.glow(0).unwrap().intensity(), 0.5);
    assert_eq!(f.effect(0), attachment);
}

#[test]
fn rejected_click_input_never_enrolls_and_retired_handles_cannot_mutate_a_new_generation() {
    let mut f = Fixture::new(None);
    f.send(1, 0, true).unwrap();
    let frame = f.session.frame().clone();
    let revision = f.scene.revision();
    let nodes = f.scene.integration_store().borrow().len();
    assert!(
        f.send(1, 0, false).is_err(),
        "replayed release is rejected before enrollment"
    );
    assert_eq!(f.session.frame(), &frame);
    assert_eq!(f.scene.revision(), revision);
    assert_eq!(f.scene.integration_store().borrow().len(), nodes);
    assert_eq!(f.effect(0), None);
    f.send(2, 0, false).unwrap();
    let stale = f.targets[0].get_effect("glow").unwrap();
    f.tick(1.0);
    f.tick(2.0);
    assert_eq!(f.effect(0), None);
    f.click(3, 0);
    let current = f.effect(0).unwrap();
    assert_ne!(current, stale.node_id());
    let revision = f.scene.revision();
    assert!(f
        .scene
        .live(&mut f.session)
        .set_effect(
            &f.targets[0],
            &stale,
            EffectUpdate::default().intensity(4.0)
        )
        .is_err());
    assert_eq!(f.scene.revision(), revision);
    assert_eq!(f.effect(0), Some(current));
    assert_eq!(f.glow(0).unwrap().intensity(), 0.0);
    f.tick(3.0);
    f.tick(4.0);
    assert_eq!(f.effect(0), None);
}

#[test]
fn invalid_persistent_edit_does_not_adopt_the_temporary_click_attachment() {
    let mut f = Fixture::new(None);
    f.click(1, 0);
    let attachment = f.effect(0).unwrap();
    f.tick(1.0);
    f.tick(1.5);
    let frame = f.session.frame().clone();
    let revision = f.scene.revision();
    assert!(f
        .scene
        .live(&mut f.session)
        .set_glow(
            &f.targets[0],
            EffectUpdate::default().radius(Pixels(f64::NAN))
        )
        .is_err());
    assert_eq!(f.scene.revision(), revision);
    assert_eq!(f.session.frame(), &frame);
    assert_eq!(f.effect(0), Some(attachment));
    f.tick(2.0);
    assert_eq!(f.effect(0), None);
    assert!(f
        .scene
        .integration_store()
        .borrow()
        .node(attachment)
        .is_none());
}
