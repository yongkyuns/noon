use crate::{
    AnimationCompositionRequest, AnimationOptions, Mobject, RateFunction, Scene, SceneExecution,
    TransformToRequest,
};
use noon_core::{
    Color, Glow, GlowSource, GlowUpdate, NativeInputModifiers, NativePointerId, NativePointerInput,
    NativePointerInputKind, NativePointerPosition, Vec2,
};

fn clicked() -> (SceneExecution, Mobject) {
    let mut scene = Scene::new();
    let mut object = scene.circle(0.5).unwrap();
    object.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    object.disable_stroke().unwrap();
    scene.add(&object).unwrap();
    let pulse = scene
        .declare_glow_pulse(&object, 2.0, AnimationOptions::new().run_time(1.0))
        .unwrap();
    scene.on_click(&object, Some(&pulse)).unwrap();
    let mut execution = scene.into_execution().unwrap();
    click(&mut execution);
    assert_eq!(glow(&execution, &object).intensity(), 2.0);
    (execution, object)
}

fn click(execution: &mut SceneExecution) {
    execution
        .session_mut()
        .configure_native_pointer_input(
            NativePointerId {
                source: 59,
                pointer: 1,
            },
            1,
        )
        .unwrap();
    let position = NativePointerPosition::new(Vec2::ZERO, Vec2::new(100.0, 100.0)).unwrap();
    for (sequence, kind) in [
        (
            1,
            NativePointerInputKind::Press {
                position,
                button: 0,
            },
        ),
        (
            2,
            NativePointerInputKind::Release {
                position,
                button: 0,
            },
        ),
    ] {
        let token = execution.session().native_pointer_input_token().unwrap();
        let input = NativePointerInput::new(
            sequence,
            token.pointer(),
            token.context(),
            NativeInputModifiers::default(),
            kind,
        );
        execution
            .submit_native_pointer_input(&token, input)
            .unwrap();
    }
    execution.advance_interactions(0.0).unwrap();
    execution.advance_interactions(0.5).unwrap();
}

fn glow(execution: &SceneExecution, object: &Mobject) -> Glow {
    let id = execution
        .session()
        .execution_object_id(object.node_id())
        .unwrap();
    execution
        .session()
        .runtime
        .effective_object(id)
        .unwrap()
        .glow
        .as_ref()
        .unwrap()
        .definition
}

fn assert_no_carry_owner(execution: &SceneExecution, object: &Mobject) {
    let id = execution
        .session()
        .execution_object_id(object.node_id())
        .unwrap();
    assert!(!execution.session().runtime.object_has_effective_driver(id));
}

#[test]
fn transient_glow_disjoint_and_same_value_edits_adopt_without_restarting_intensity() {
    let (mut execution, object) = clicked();
    let attachment = object.get_effect("glow").unwrap().node_id();
    // This is an explicit ownership handoff even though the authored white value
    // produces no semantic parameter impact.
    execution
        .live()
        .set_glow(&object, GlowUpdate::default().color(Color::WHITE))
        .unwrap();
    assert_eq!(glow(&execution, &object).intensity(), 2.0);
    assert!(execution.session().interactions_active());
    execution
        .live()
        .set_glow(
            &object,
            GlowUpdate::default().color(Color::BLUE).radius(0.3),
        )
        .unwrap();
    assert_eq!(glow(&execution, &object).intensity(), 2.0);
    assert_eq!(glow(&execution, &object).color(), Color::BLUE);
    assert_eq!(glow(&execution, &object).radius().value(), 0.3);
    assert_no_carry_owner(&execution, &object);
    execution.advance_interactions(1.0).unwrap();
    assert!(!execution.session().interactions_active());
    assert_eq!(object.get_effect("glow").unwrap().node_id(), attachment);
    assert_eq!(glow(&execution, &object).intensity(), 0.0);
    assert_eq!(glow(&execution, &object).radius().value(), 0.3);
    assert_no_carry_owner(&execution, &object);
}

#[test]
fn transient_glow_same_value_intensity_write_supersedes_the_effective_peak() {
    let (mut execution, object) = clicked();
    let attachment = object.get_effect("glow").unwrap().node_id();
    execution
        .live()
        .set_glow(&object, GlowUpdate::default().intensity(0.0))
        .unwrap();
    assert!(!execution.session().interactions_active());
    assert_eq!(glow(&execution, &object).intensity(), 0.0);
    assert_no_carry_owner(&execution, &object);
    execution.advance_interactions(2.0).unwrap();
    assert_eq!(object.get_effect("glow").unwrap().node_id(), attachment);
    assert_eq!(glow(&execution, &object).intensity(), 0.0);
}

#[test]
fn transient_glow_empty_or_rejected_edit_keeps_temporary_retirement_owned() {
    let (mut execution, object) = clicked();
    let attachment = object.get_effect("glow").unwrap();
    let publication = execution.session().publication_context();
    let frame = execution.session().frame().clone();
    execution
        .live()
        .set_glow(&object, GlowUpdate::default())
        .unwrap();
    assert_eq!(execution.session().publication_context(), publication);
    // A live source-mode schema change is rejected by compiler preparation;
    // submitted intent must not adopt the effect before that preparation succeeds.
    assert!(execution
        .live()
        .set_glow(
            &object,
            GlowUpdate::default().source(GlowSource::Silhouette),
        )
        .is_err());
    assert_eq!(execution.session().publication_context(), publication);
    assert_eq!(execution.session().frame(), &frame);
    assert!(execution.session().interactions_active());
    execution.advance_interactions(1.0).unwrap();
    assert!(object.get_effect("glow").is_err());
    assert!(attachment.authored_definition().is_err());
    assert!(!execution.session().interactions_active());
}

#[test]
fn transient_glow_authored_radius_track_adopts_and_keeps_input_intensity_independent() {
    let (mut execution, object) = clicked();
    let attachment = object.get_effect("glow").unwrap().node_id();
    let target = execution.live().target_editor(&object).unwrap();
    execution
        .live()
        .set_glow(&target, GlowUpdate::default().radius(0.4))
        .unwrap();
    let request = AnimationCompositionRequest::TransformTo(
        TransformToRequest::new(
            &object,
            &target,
            AnimationOptions::new()
                .run_time(0.4)
                .rate_func(RateFunction::Linear),
        )
        .method_target(),
    );
    let segment = execution
        .live()
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .unwrap();
    assert_eq!(glow(&execution, &object).intensity(), 2.0);
    assert!(execution.session().interactions_active());
    assert_no_carry_owner(&execution, &object);
    execution.advance_interactions(1.0).unwrap();
    assert_eq!(glow(&execution, &object).intensity(), 0.0);
    execution
        .live()
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    execution.live().complete_segment(segment).unwrap();
    assert_eq!(object.get_effect("glow").unwrap().node_id(), attachment);
    assert_eq!(glow(&execution, &object).radius().value(), 0.4);
    assert_eq!(glow(&execution, &object).intensity(), 0.0);
}

#[test]
fn transient_glow_authored_intensity_track_keeps_its_reversed_initial_sample() {
    let (mut execution, object) = clicked();
    let attachment = object.get_effect("glow").unwrap().node_id();
    let target = execution.live().target_editor(&object).unwrap();
    execution
        .live()
        .set_glow(&target, GlowUpdate::default().intensity(0.8))
        .unwrap();
    let request = AnimationCompositionRequest::TransformTo(
        TransformToRequest::new(
            &object,
            &target,
            AnimationOptions::new()
                .run_time(0.4)
                .rate_func(RateFunction::Linear)
                .reverse_rate_function(true),
        )
        .method_target(),
    );
    let segment = execution
        .live()
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .unwrap();
    assert!(!execution.session().interactions_active());
    assert_eq!(glow(&execution, &object).intensity(), 0.8);
    assert_no_carry_owner(&execution, &object);
    execution.advance_interactions(2.0).unwrap();
    assert_eq!(object.get_effect("glow").unwrap().node_id(), attachment);
    assert_eq!(glow(&execution, &object).intensity(), 0.8);
    execution
        .live()
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    execution.live().complete_segment(segment).unwrap();
    assert_eq!(object.get_effect("glow").unwrap().node_id(), attachment);
}

#[test]
fn transient_glow_admits_preclick_motion_target_without_adopting_temporary_attachment() {
    let mut scene = Scene::new();
    let mut object = scene.circle(0.5).unwrap();
    object.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    object.disable_stroke().unwrap();
    scene.add(&object).unwrap();
    let mut target = object.target_editor().unwrap();
    target.shift(1.0, 0.0).unwrap();
    let motion = scene
        .declare_transform_to(
            &object,
            &target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let pulse = scene
        .declare_glow_pulse(&object, 2.0, AnimationOptions::new().run_time(1.0))
        .unwrap();
    scene.on_click(&object, Some(&pulse)).unwrap();
    assert!(object.get_effect("glow").is_err());
    assert!(target.get_effect("glow").is_err());
    let mut execution = scene.into_execution().unwrap();
    click(&mut execution);
    let temporary = object.get_effect("glow").unwrap();
    let object_id = execution
        .session()
        .execution_object_id(object.node_id())
        .unwrap();
    let segment = execution.live().play_animation(&motion).unwrap();
    assert_eq!(glow(&execution, &object).intensity(), 2.0);
    assert_eq!(
        object.get_effect("glow").unwrap().node_id(),
        temporary.node_id()
    );
    assert!(execution.session().interactions_active());
    assert_no_carry_owner(&execution, &object);
    execution.live().advance_segment_to(segment, 0.4).unwrap();
    assert_eq!(glow(&execution, &object).intensity(), 2.0);
    assert!(
        (execution
            .session()
            .runtime
            .effective_object(object_id)
            .unwrap()
            .transform
            .translation
            .x
            - 0.4)
            .abs()
            < 1e-6
    );
    execution.advance_interactions(0.75).unwrap();
    let intensity = glow(&execution, &object).intensity();
    assert!(intensity > 0.0 && intensity < 2.0);
    execution.advance_interactions(1.0).unwrap();
    assert!(!execution.session().interactions_active());
    assert_eq!(execution.session().frame().time, 0.4);
    assert!(object.get_effect("glow").is_err());
    assert!(temporary.authored_definition().is_err());
    assert!(execution
        .session()
        .runtime
        .effective_object(object_id)
        .unwrap()
        .glow
        .is_none());
    execution
        .live()
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    execution.live().complete_segment(segment).unwrap();
    assert_eq!(
        execution
            .session()
            .runtime
            .effective_object(object_id)
            .unwrap()
            .transform
            .translation
            .x,
        1.0
    );
    assert!(object.get_effect("glow").is_err());
}
