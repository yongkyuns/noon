//! Target-neutral scene builders shared by executable Rust examples.

use std::error::Error;

use crate::{
    AnimationOptions, Color, ExecutionSession, ExecutionSessionInputError, HostCallbackId,
    RateFunction, ReactiveValue, RustHostCallbackTable, Scene, SemanticPaint, SemanticVec3, Vec2,
};

const SET_Y: HostCallbackId = HostCallbackId::new(1);
const SET_OPACITY: HostCallbackId = HostCallbackId::new(2);
const ACCUMULATE_DT: HostCallbackId = HostCallbackId::new(3);

/// Build the paired affine callback example without selecting a platform host.
///
/// Both native and direct single-context Rust/WASM examples consume these typed
/// values. The execution schedule remains owned by [`ExecutionSession`], while
/// the callable table contains only host-owned Rust closures.
pub fn live_affine_callbacks() -> Result<(ExecutionSession, RustHostCallbackTable), Box<dyn Error>>
{
    let mut scene = Scene::new();
    let mut source = scene.circle(1.0)?;
    source.set_fill(1.0, 1.0, 1.0, 1.0)?;
    let mut drift = scene.circle(0.5)?;
    drift.set_fill(1.0, 1.0, 1.0, 1.0)?;
    drift.set_translation(-3.0, 0.0)?;
    scene.add(&source)?;
    scene.add(&drift)?;

    let mut target = source.target_editor()?;
    target.set_translation(2.0, 0.0)?;
    let animation = scene.declare_transform_to(
        &source,
        &target,
        AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear),
    )?;

    let mut callbacks = RustHostCallbackTable::new();
    callbacks.insert(SET_Y, |context| {
        let mut transform = context.target_state().transform;
        transform.translation.y = 1.0;
        context.set_target_transform(transform)
    })?;
    callbacks.insert(SET_OPACITY, |context| {
        let prior_y = context.target_state().transform.translation.y;
        let mut style = context.target_state().style;
        // The visible result depends on reading SET_Y from this same phase overlay.
        style.opacity = if prior_y == 1.0 { 0.5 } else { 0.0 };
        context.set_target_style(style)
    })?;
    callbacks.insert(ACCUMULATE_DT, |context| {
        let mut transform = context.target_state().transform;
        transform.translation.y += context.delta_time() as f32;
        context.set_target_transform(transform)
    })?;

    {
        let mut store = scene.store().borrow_mut();
        callbacks.add_updater(&mut store, source.node_id(), SET_Y, 0.0, None)?;
        callbacks.add_updater(&mut store, source.node_id(), SET_OPACITY, 0.0, None)?;
        callbacks.add_updater(&mut store, drift.node_id(), ACCUMULATE_DT, 0.0, None)?;
    }

    let mut session = scene.execution_session()?;
    {
        let mut live = scene.live(&mut session);
        live.play_animation(&animation)?;
    }
    Ok((session, callbacks))
}

/// Execute the paired affine-completion example and return its settled session.
///
/// Both animation declarations exist before execution starts. Completion persists
/// each endpoint into authored state and releases its timeline driver, so an
/// intervening authored setter and a later replay use the same canonical path.
pub fn live_affine_completion() -> Result<ExecutionSession, Box<dyn Error>> {
    let mut scene = Scene::new();
    let circle = scene.circle(1.0)?;
    scene.add(&circle)?;

    let mut first_target = circle.target_editor()?;
    first_target.set_translation(2.0, -2.0)?;
    let first = scene.declare_transform_to(
        &circle,
        &first_target,
        AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear),
    )?;
    let mut second_target = circle.target_editor()?;
    second_target.set_translation(5.0, -2.0)?;
    let second = scene.declare_transform_to(
        &circle,
        &second_target,
        AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear),
    )?;

    let mut session = scene.execution_session()?;
    {
        let mut live = scene.live(&mut session);
        let first_segment = live.play_animation(&first)?;
        live.advance_segment_to(first_segment, first_segment.end_time())?;
        assert!(!live.segment_state(first_segment).is_complete());
        live.complete_segment(first_segment)?;
        assert!(live.segment_state(first_segment).is_complete());
        assert_eq!(
            live.effective(&circle)?.transform.translation,
            Vec2::new(2.0, -2.0)
        );
        let first_authored = live.authored(&circle)?.transform.translation;
        assert_eq!((first_authored.x, first_authored.y), (2.0, -2.0));

        live.set_translation(&circle, 3.0, -2.0)?;
        assert_eq!(
            live.effective(&circle)?.transform.translation,
            Vec2::new(3.0, -2.0)
        );

        let next_frame = live.wait_segment(0.25)?;
        live.advance_segment_to(next_frame, next_frame.end_time())?;
        live.complete_segment(next_frame)?;
        assert_eq!(
            live.effective(&circle)?.transform.translation,
            Vec2::new(3.0, -2.0),
            "released timeline state must not overwrite the later authored setter",
        );
        let edited_authored = live.authored(&circle)?.transform.translation;
        assert_eq!((edited_authored.x, edited_authored.y), (3.0, -2.0));

        let second_segment = live.play_animation(&second)?;
        live.advance_segment_to(second_segment, second_segment.start_time() + 1.0)?;
        assert_eq!(
            live.effective(&circle)?.transform.translation,
            Vec2::new(4.0, -2.0),
            "the second declaration must capture the current effective source",
        );
        live.advance_segment_to(second_segment, second_segment.end_time())?;
        live.complete_segment(second_segment)?;
        assert!(live.segment_state(second_segment).is_complete());
        assert_eq!(
            live.effective(&circle)?.transform.translation,
            Vec2::new(5.0, -2.0)
        );
        let final_authored = live.authored(&circle)?.transform.translation;
        assert_eq!((final_authored.x, final_authored.y), (5.0, -2.0));
    }
    Ok(session)
}

/// Execute the paired ordinary affine-play example on one live runtime.
///
/// Targets are authored before execution. Each play is declared and activated
/// atomically against the current effective state, while the intervening wait
/// and authored shift publish through the same continuation/session authority.
pub fn ordinary_affine_play() -> Result<ExecutionSession, Box<dyn Error>> {
    let mut scene = Scene::new();
    let mut circle = scene.circle(0.4)?;
    circle.set_fill(0.0, 0.4, 1.0, 1.0)?;
    scene.add(&circle)?;

    let mut first_target = circle.target_editor()?;
    first_target.set_translation(2.0, -1.0)?;
    let linear = |duration| {
        AnimationOptions::new()
            .run_time(duration)
            .rate_func(RateFunction::Linear)
    };

    let mut session = scene.execution_session()?;
    {
        let mut live = scene.live(&mut session);
        let first = live.declare_and_activate_transform_to(&circle, &first_target, linear(2.0))?;
        assert_eq!((first.start_time(), first.end_time()), (0.0, 2.0));
        live.advance_segment_to(first, first.end_time())?;
        live.complete_segment(first)?;
        assert_eq!(live.effective_layout(&circle)?.center, (2.0, -1.0));
    }
    assert_eq!(session.frame().time, 2.0);

    {
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(1.0)?;
        assert_eq!((wait.start_time(), wait.end_time()), (2.0, 3.0));
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
        assert_eq!(live.effective_layout(&circle)?.center, (2.0, -1.0));
        live.shift(&circle, 1.0, 0.0)?;
        assert_eq!(live.effective_layout(&circle)?.center, (3.0, -1.0));
    }
    assert_eq!(session.frame().time, 3.0);

    {
        let mut live = scene.live(&mut session);
        let second_target = live.target_editor(&circle)?;
        live.set_translation(&second_target, 5.0, -1.0)?;
        let second =
            live.declare_and_activate_transform_to(&circle, &second_target, linear(1.0))?;
        assert_eq!((second.start_time(), second.end_time()), (3.0, 4.0));
        live.advance_segment_to(second, second.end_time())?;
        live.complete_segment(second)?;
        assert_eq!(live.effective_layout(&circle)?.center, (5.0, -1.0));
        let authored = live.authored(&circle)?.transform.translation;
        assert_eq!((authored.x, authored.y), (5.0, -1.0));
    }
    assert_eq!(session.frame().time, 4.0);
    Ok(session)
}

/// Execute one ordinary style play and a following authored style edit.
///
/// The animation's fill and object-opacity channels share one runtime segment.
/// Completion reconciles their exact semantic endpoint before the ordinary green
/// edit publishes through the same live session.
pub fn ordinary_style_play() -> Result<ExecutionSession, Box<dyn Error>> {
    let mut scene = Scene::new();
    let mut circle = scene.circle(0.4)?;
    circle.set_fill(0.0, 0.4, 1.0, 1.0)?;
    circle.set_object_opacity(1.0)?;
    scene.add(&circle)?;

    let mut session = scene.execution_session()?;
    {
        let mut live = scene.live(&mut session);
        let target = live.target_editor(&circle)?;
        let mut target_style = live.authored(&target)?.style;
        target_style.fill = Some(SemanticPaint::Solid(Color::rgb(1.0, 0.0, 0.0)));
        target_style.fill_opacity = 0.4;
        target_style.object_opacity = 0.5;
        live.replace_style(&target, target_style.clone())?;

        let segment = live.declare_and_activate_transform_to(
            &circle,
            &target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )?;
        live.advance_segment_to(segment, 1.0)?;
        let midpoint = live.effective(&circle)?.style;
        let midpoint_fill = midpoint.fill.expect("the style play retains its fill");
        assert!((midpoint_fill.red - 0.5).abs() < 1e-6);
        assert!((midpoint_fill.green - 0.2).abs() < 1e-6);
        assert!((midpoint_fill.blue - 0.5).abs() < 1e-6);
        assert!((midpoint_fill.alpha - 0.7).abs() < 1e-6);
        assert!((midpoint.opacity - 0.75).abs() < 1e-6);

        live.advance_segment_to(segment, segment.end_time())?;
        let endpoint = live.effective(&circle)?.style;
        assert_eq!(endpoint.fill, Some(Color::rgba(1.0, 0.0, 0.0, 0.4)));
        assert!((endpoint.opacity - 0.5).abs() < 1e-6);
        live.complete_segment(segment)?;
        assert_eq!(live.authored(&circle)?.style, target_style);

        let mut final_style = target_style;
        final_style.fill = Some(SemanticPaint::Solid(Color::rgb(0.0, 1.0, 0.0)));
        final_style.fill_opacity = 1.0;
        final_style.object_opacity = 1.0;
        live.replace_style(&circle, final_style.clone())?;
        assert_eq!(live.authored(&circle)?.style, final_style);
        let effective = live.effective(&circle)?.style;
        assert_eq!(effective.fill, Some(Color::rgb(0.0, 1.0, 0.0)));
        assert_eq!(effective.opacity, 1.0);
    }
    assert_eq!(session.frame().time, 2.0);
    Ok(session)
}

/// Build and settle the paired canonical scalar `ValueTracker` example.
///
/// Signal-track scheduling, interpolation, binding evaluation, and direct-write
/// ownership all remain inside the one canonical execution session. Native and
/// direct single-context Rust/WASM hosts consume the returned typed session.
pub fn live_value_tracker() -> Result<ExecutionSession, Box<dyn Error>> {
    let mut scene = Scene::new();
    let mut circle = scene.circle(0.4)?;
    circle.set_fill(1.0, 1.0, 1.0, 1.0)?;
    scene.add(&circle)?;

    let tracker = scene.value_tracker(0.0)?;
    let position = scene.position_from_tracker(
        &tracker,
        SemanticVec3::new(1.0, 0.0, 0.0),
        SemanticVec3::new(-2.0, 0.0, 0.0),
    )?;
    scene.bind_position(&circle, &position)?;
    scene
        .play_value(&tracker, 4.0)
        .rate_func(RateFunction::Linear)
        .run_time(2.0)?;
    assert_eq!(scene.value_tracker_value(&tracker)?, 4.0);

    let mut session = scene.execution_session()?;
    session.evaluate(1.0)?;
    assert_eq!(
        session.effective_signal_value(tracker.node_id()),
        Some(&ReactiveValue::Scalar(2.0))
    );
    assert_eq!(
        scene.live(&mut session).effective_layout(&circle)?.center,
        (0.0, 0.0)
    );

    session.evaluate(2.0)?;
    assert_eq!(
        session.effective_signal_value(tracker.node_id()),
        Some(&ReactiveValue::Scalar(4.0))
    );
    assert_eq!(
        scene.live(&mut session).effective_layout(&circle)?.center,
        (2.0, 0.0)
    );
    assert!(matches!(
        session.set_reactive_input(tracker.node_id(), 3.0_f32),
        Err(ExecutionSessionInputError::TimelineOwnedSignal { .. })
    ));
    Ok(session)
}

/// Build a canonical scene whose native sources lower into one execution session.
///
/// Browser/native hosts deliver normalized source occurrences to the returned
/// session. They do not own signal identities, routing tables, or reactive state.
pub fn live_native_signals() -> Result<ExecutionSession, Box<dyn Error>> {
    let mut scene = Scene::new();
    let mut square = scene.square(0.9)?;
    square.set_fill(0.0, 0.4, 1.0, 1.0)?;
    scene.add(&square)?;

    let pointer = scene.pointer_position_signal()?;
    scene.bind_native_translation(&square, &pointer)?;
    let opacity = scene.control_signal("opacity", 1.0)?;
    scene.bind_opacity(&square, &opacity)?;
    let clicks = scene.pointer_down_events(0)?;
    scene.bind_rotation(&square, &clicks)?;

    // These declarations prove the remaining normalized source vocabulary is
    // authored on semantic signal nodes. Unbound sources lower to runtime no-ops.
    let visible = scene.key_state_signal("Space", false)?;
    scene.bind_presence(&square, &visible)?;
    let _ = scene.viewport_size_signal()?;
    let _ = scene.wheel_delta_signal()?;
    let _ = scene.wheel_events()?;
    let _ = scene.control_commit_events("opacity")?;

    Ok(scene.execution_session()?)
}
