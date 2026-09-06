//! Target-neutral scene builders shared by executable Rust examples.

use std::error::Error;

use crate::{
    AnimationOptions, Color, ExecutionSession, HostCallbackId, LiveContinuation, LiveProgram,
    LiveSession, Mobject, RateFunction, RustHostCallbackTable, Scene,
    SemanticAnimationCompositionKind, SemanticFadeDirection, SemanticMutationTransaction,
    SemanticNodeId, SemanticPaint, SemanticStyle, SemanticVec3, TransformToRequest, ValueTracker,
    Vec2,
};

const SET_Y: HostCallbackId = HostCallbackId::new(1);
const SET_OPACITY: HostCallbackId = HostCallbackId::new(2);
const ACCUMULATE_DT: HostCallbackId = HostCallbackId::new(3);
const ACCUMULATE_TEXT_DT: HostCallbackId = HostCallbackId::new(4);
const ROTATE_LINE_FORWARD: HostCallbackId = HostCallbackId::new(5);
const ROTATE_LINE_BACKWARD: HostCallbackId = HostCallbackId::new(6);
const FOLLOW_SPARSE_READS: HostCallbackId = HostCallbackId::new(7);

fn ordered_affine_callbacks() -> Result<RustHostCallbackTable, Box<dyn Error>> {
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
    Ok(callbacks)
}

/// Build the paired affine callback example without selecting a platform host.
///
/// Both native and direct single-context Rust/WASM examples consume these typed
/// values. The execution schedule remains owned by [`ExecutionSession`], while
/// the callable table contains only host-owned Rust closures.
pub fn live_affine_callbacks() -> Result<(ExecutionSession, RustHostCallbackTable), Box<dyn Error>>
{
    let mut scene = Scene::new();
    let mut label = scene.text("Noon")?;
    label.set_translation(0.0, -2.0)?;
    let mut source = scene.circle(1.0)?;
    source.set_fill(1.0, 1.0, 1.0, 1.0)?;
    let mut drift = scene.circle(0.5)?;
    drift.set_fill(1.0, 1.0, 1.0, 1.0)?;
    drift.set_translation(-3.0, 0.0)?;
    scene.add(&label)?;
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

    let mut callbacks = ordered_affine_callbacks()?;
    callbacks.insert(ACCUMULATE_DT, |context| {
        let mut transform = context.target_state().transform;
        transform.translation.y += context.delta_time() as f32;
        context.set_target_transform(transform)
    })?;
    callbacks.insert(ACCUMULATE_TEXT_DT, |context| {
        let mut transform = context.target_state().transform;
        transform.translation.x += context.delta_time() as f32;
        context.set_target_transform(transform)
    })?;

    {
        let mut store = scene.store().borrow_mut();
        callbacks.add_updater(&mut store, label.node_id(), ACCUMULATE_TEXT_DT, 0.0, None)?;
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

/// Build the paired Line callback example used by renderer-observation proofs.
///
/// The compiler owns the adjacent forward and reverse callback windows. Circle,
/// reference-Line, and Text siblings stay resident while only the moving Line's
/// one effective transform changes.
pub fn live_line_callback_rotation(
) -> Result<(ExecutionSession, RustHostCallbackTable), Box<dyn Error>> {
    let mut scene = Scene::new();
    let mut marker = scene.circle(0.35)?;
    marker.set_fill(0.1, 0.4, 1.0, 1.0)?;
    marker.set_translation(-3.0, 0.0)?;
    let mut reference = scene.line((0.0, 0.0), (-1.0, 0.0))?;
    reference.set_color(1.0, 1.0, 1.0, 1.0)?;
    let mut moving = scene.line((0.0, 0.0), (-1.0, 0.0))?;
    moving.set_color(1.0, 0.8, 0.0, 1.0)?;
    let mut label = scene.text("Noon")?;
    label.set_translation(0.0, -2.0)?;
    scene.add(&marker)?;
    scene.add(&reference)?;
    scene.add(&moving)?;
    scene.add(&label)?;

    let mut callbacks = RustHostCallbackTable::new();
    callbacks.insert(ROTATE_LINE_FORWARD, |context| {
        let transform = context
            .target_transform_rotated_about_point(context.delta_time(), Vec2::ZERO)
            .map_err(std::io::Error::other)?;
        context
            .set_target_transform(transform)
            .map_err(|error| std::io::Error::other(error.to_string()))
    })?;
    callbacks.insert(ROTATE_LINE_BACKWARD, |context| {
        let transform = context
            .target_transform_rotated_about_point(-context.delta_time(), Vec2::ZERO)
            .map_err(std::io::Error::other)?;
        context
            .set_target_transform(transform)
            .map_err(|error| std::io::Error::other(error.to_string()))
    })?;
    {
        let mut store = scene.store().borrow_mut();
        callbacks.add_updater(&mut store, moving.node_id(), ROTATE_LINE_FORWARD, 0.0, None)?;
        callbacks.add_updater(
            &mut store,
            moving.node_id(),
            ROTATE_LINE_BACKWARD,
            2.0,
            None,
        )?;
        let mut close_windows = SemanticMutationTransaction::new();
        close_windows.remove_updater(moving.node_id(), ROTATE_LINE_FORWARD, 2.0);
        close_windows.remove_updater(moving.node_id(), ROTATE_LINE_BACKWARD, 4.0);
        close_windows.apply(&mut store)?;
    }

    Ok((scene.execution_session()?, callbacks))
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

/// Ordinary Rust continuation used unchanged by native and direct Rust/WASM hosts.
///
/// It owns only direct Rust locals and shared semantic handles. Every interval,
/// endpoint, completion, and authored mutation goes through the borrowed
/// [`LiveSession`], so a host only drives the returned segment and admits its
/// renderer publication.
pub struct OrdinaryAffineContinuation {
    circle: Mobject,
    first_target: Mobject,
    stage: u8,
}

impl LiveContinuation for OrdinaryAffineContinuation {
    type Error = String;

    fn resume(
        &mut self,
        live: &mut LiveSession<'_>,
    ) -> Result<crate::ContinuationStep, Self::Error> {
        let linear = |duration| {
            AnimationOptions::new()
                .run_time(duration)
                .rate_func(RateFunction::Linear)
        };
        match self.stage {
            0 => {
                self.stage = 1;
                live.declare_and_activate_transform_to(
                    &self.circle,
                    &self.first_target,
                    linear(2.0),
                )
                .map(crate::ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            1 => {
                self.stage = 2;
                live.wait_segment(1.0)
                    .map(crate::ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            2 => {
                // This resumes after the wait's no-op endpoint. It proves that
                // authored edits and late targets keep using the one live session.
                live.shift(&self.circle, 1.0, 0.0)
                    .map_err(|error| error.to_string())?;
                let late_target = live
                    .target_editor(&self.circle)
                    .map_err(|error| error.to_string())?;
                live.set_translation(&late_target, 5.0, -1.0)
                    .map_err(|error| error.to_string())?;
                self.stage = 3;
                live.declare_and_activate_transform_to(&self.circle, &late_target, linear(1.0))
                    .map(crate::ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            3 => {
                self.stage = 4;
                Ok(crate::ContinuationStep::Finished)
            }
            _ => Err("ordinary affine continuation resumed after it finished".to_owned()),
        }
    }
}

/// Build the ordinary affine continuation program without selecting a platform host.
///
/// The scene matches the ordinary affine example: an opaque blue radius-0.4 circle
/// moves to `(2, -1)` over two seconds, waits one second, is shifted to `(3, -1)`,
/// then a late live target moves it to `(5, -1)` over one second.
pub fn ordinary_affine_continuation_program(
) -> Result<LiveProgram<OrdinaryAffineContinuation>, String> {
    let mut scene = Scene::new();
    let mut circle = scene.circle(0.4).map_err(|error| error.to_string())?;
    circle
        .set_fill(0.0, 0.4, 1.0, 1.0)
        .map_err(|error| error.to_string())?;
    scene.add(&circle).map_err(|error| error.to_string())?;

    let mut first_target = circle.target_editor().map_err(|error| error.to_string())?;
    first_target
        .set_translation(2.0, -1.0)
        .map_err(|error| error.to_string())?;
    scene
        .into_live_program(OrdinaryAffineContinuation {
            circle,
            first_target,
            stage: 0,
        })
        .map_err(|error| error.to_string())
}

/// Scalar continuation that keeps the signal timeline and persistent setter in Rust.
pub struct OrdinaryValueTrackerContinuation {
    tracker: ValueTracker,
    stage: u8,
}

impl LiveContinuation for OrdinaryValueTrackerContinuation {
    type Error = String;

    fn resume(
        &mut self,
        live: &mut LiveSession<'_>,
    ) -> Result<crate::ContinuationStep, Self::Error> {
        match self.stage {
            0 => {
                self.stage = 1;
                live.declare_and_activate_value_tracker(
                    &self.tracker,
                    2.0,
                    2.0,
                    RateFunction::Linear,
                )
                .map(crate::ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            1 => {
                // Completion releases the first track. This setter is one
                // authored hold at t=2, not a host-side scalar cache.
                live.set_value(&self.tracker, 3.0)
                    .map_err(|error| error.to_string())?;
                self.stage = 2;
                live.wait_segment(1.0)
                    .map(crate::ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            2 => {
                self.stage = 3;
                live.declare_and_activate_value_tracker(
                    &self.tracker,
                    5.0,
                    1.0,
                    RateFunction::Linear,
                )
                .map(crate::ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            3 => {
                self.stage = 4;
                Ok(crate::ContinuationStep::Finished)
            }
            _ => Err("ordinary ValueTracker continuation resumed after it finished".to_owned()),
        }
    }
}

/// Build the paired Python/Rust scalar continuation without selecting a host.
///
/// A white circle follows `offset + tracker * RIGHT`: the tracker moves from
/// `0` to `2` over two seconds, is held at `3` through a one-second wait, and
/// then moves from `3` to `5` over one second. The returned `LiveProgram` owns
/// the one session, segment lifecycle, and renderer publication barriers.
pub fn ordinary_value_tracker_continuation_program(
) -> Result<LiveProgram<OrdinaryValueTrackerContinuation>, String> {
    let mut scene = Scene::new();
    let mut circle = scene.circle(0.4).map_err(|error| error.to_string())?;
    circle
        .set_fill(1.0, 1.0, 1.0, 1.0)
        .map_err(|error| error.to_string())?;
    scene.add(&circle).map_err(|error| error.to_string())?;

    let tracker = scene
        .value_tracker(0.0)
        .map_err(|error| error.to_string())?;
    let position = scene
        .position_from_tracker(
            &tracker,
            SemanticVec3::new(1.0, 0.0, 0.0),
            SemanticVec3::new(-2.0, 0.0, 0.0),
        )
        .map_err(|error| error.to_string())?;
    scene
        .bind_position(&circle, &position)
        .map_err(|error| error.to_string())?;

    scene
        .into_live_program(OrdinaryValueTrackerContinuation { stage: 0, tracker })
        .map_err(|error| error.to_string())
}

/// One flat Parallel/Sequence continuation over the same shared live session.
pub struct OrdinaryCompositionContinuation {
    left: Mobject,
    right: Mobject,
    left_position: Mobject,
    right_position: Mobject,
    stage: u8,
}

impl LiveContinuation for OrdinaryCompositionContinuation {
    type Error = String;

    fn resume(
        &mut self,
        live: &mut LiveSession<'_>,
    ) -> Result<crate::ContinuationStep, Self::Error> {
        let linear = |duration| {
            AnimationOptions::new()
                .run_time(duration)
                .rate_func(RateFunction::Linear)
        };
        match self.stage {
            0 => {
                self.stage = 1;
                let requests = [
                    TransformToRequest::new(&self.left, &self.left_position, linear(2.0)),
                    TransformToRequest::new(&self.right, &self.right_position, linear(2.0)),
                ];
                live.declare_and_activate_transform_composition(
                    SemanticAnimationCompositionKind::Parallel,
                    &requests,
                    AnimationOptions::new()
                        .lag_ratio(0.0)
                        .rate_func(RateFunction::Linear),
                    AnimationOptions::new().rate_func(RateFunction::Linear),
                )
                .map(crate::ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            1 => {
                let left_fill = live
                    .target_editor(&self.left)
                    .map_err(|error| error.to_string())?;
                live.set_fill(&left_fill, 1.0, 0.0, 0.0, 1.0)
                    .map_err(|error| error.to_string())?;
                let right_fill = live
                    .target_editor(&self.right)
                    .map_err(|error| error.to_string())?;
                live.set_fill(&right_fill, 0.0, 0.0, 1.0, 1.0)
                    .map_err(|error| error.to_string())?;
                self.stage = 2;
                let requests = [
                    TransformToRequest::new(&self.left, &left_fill, linear(1.0)),
                    TransformToRequest::new(&self.right, &right_fill, linear(1.0)),
                ];
                live.declare_and_activate_transform_composition(
                    SemanticAnimationCompositionKind::Sequence,
                    &requests,
                    AnimationOptions::new()
                        .lag_ratio(1.0)
                        .rate_func(RateFunction::Linear),
                    AnimationOptions::new().rate_func(RateFunction::Linear),
                )
                .map(crate::ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            2 => {
                self.stage = 3;
                live.set_fill(&self.left, 0.0, 1.0, 0.0, 1.0)
                    .map_err(|error| error.to_string())?;
                Ok(crate::ContinuationStep::Finished)
            }
            _ => Err("ordinary composition continuation resumed after it finished".to_owned()),
        }
    }
}

/// Build the paired Python/Rust flat composition continuation without choosing a host.
pub fn ordinary_composition_continuation_program(
) -> Result<LiveProgram<OrdinaryCompositionContinuation>, String> {
    let mut scene = Scene::new();
    let mut left = scene.circle(0.4).map_err(|error| error.to_string())?;
    left.set_fill(1.0, 1.0, 1.0, 1.0)
        .map_err(|error| error.to_string())?;
    left.set_translation(-2.0, 0.0)
        .map_err(|error| error.to_string())?;
    let mut right = scene.circle(0.4).map_err(|error| error.to_string())?;
    right
        .set_fill(1.0, 1.0, 1.0, 1.0)
        .map_err(|error| error.to_string())?;
    right
        .set_translation(2.0, 0.0)
        .map_err(|error| error.to_string())?;
    scene.add(&left).map_err(|error| error.to_string())?;
    scene.add(&right).map_err(|error| error.to_string())?;

    let mut left_position = left.target_editor().map_err(|error| error.to_string())?;
    left_position
        .set_translation(-2.0, 1.0)
        .map_err(|error| error.to_string())?;
    let mut right_position = right.target_editor().map_err(|error| error.to_string())?;
    right_position
        .set_translation(2.0, -1.0)
        .map_err(|error| error.to_string())?;
    scene
        .into_live_program(OrdinaryCompositionContinuation {
            left,
            right,
            left_position,
            right_position,
            stage: 0,
        })
        .map_err(|error| error.to_string())
}

/// One ordinary transform continuation whose effective frame is completed by
/// two ordered host callbacks at every required phase.
pub struct OrdinaryCallbackContinuation {
    circle: Mobject,
    target: Mobject,
    stage: u8,
}

impl LiveContinuation for OrdinaryCallbackContinuation {
    type Error = String;

    fn resume(
        &mut self,
        live: &mut LiveSession<'_>,
    ) -> Result<crate::ContinuationStep, Self::Error> {
        match self.stage {
            0 => {
                self.stage = 1;
                live.declare_and_activate_transform_to(
                    &self.circle,
                    &self.target,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear),
                )
                .map(crate::ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            1 => {
                self.stage = 2;
                Ok(crate::ContinuationStep::Finished)
            }
            _ => Err("ordinary callback continuation resumed after it finished".to_owned()),
        }
    }
}

/// Build the paired callback-aware continuation for direct Rust hosts.
///
/// The blue circle moves to `(2, 0)` over one second. At each compiler-selected
/// phase, callback A moves the effective row to `y=1`; callback B observes A's
/// write and sets object opacity to `0.5`. The returned callable table owns only
/// opaque Rust functions; the program/session retains schedule and timeline state.
pub fn ordinary_callback_continuation_program() -> Result<
    (
        LiveProgram<OrdinaryCallbackContinuation>,
        RustHostCallbackTable,
    ),
    String,
> {
    let mut scene = Scene::new();
    let mut circle = scene.circle(0.4).map_err(|error| error.to_string())?;
    circle
        .set_fill(0.0, 0.4, 1.0, 1.0)
        .map_err(|error| error.to_string())?;
    scene.add(&circle).map_err(|error| error.to_string())?;
    let mut target = circle.target_editor().map_err(|error| error.to_string())?;
    target
        .set_translation(2.0, 0.0)
        .map_err(|error| error.to_string())?;

    let callbacks = ordered_affine_callbacks().map_err(|error| error.to_string())?;
    {
        let mut store = scene.store().borrow_mut();
        callbacks
            .add_updater(&mut store, circle.node_id(), SET_Y, 0.0, None)
            .map_err(|error| error.to_string())?;
        callbacks
            .add_updater(&mut store, circle.node_id(), SET_OPACITY, 0.0, None)
            .map_err(|error| error.to_string())?;
    }

    let program = scene
        .into_live_program(OrdinaryCallbackContinuation {
            circle,
            target,
            stage: 0,
        })
        .map_err(|error| error.to_string())?;
    Ok((program, callbacks))
}

/// Continuation for the paired sparse callback-read example.
pub struct OrdinaryCallbackSparseReadsContinuation {
    tracker: ValueTracker,
    stage: u8,
}

impl LiveContinuation for OrdinaryCallbackSparseReadsContinuation {
    type Error = String;

    fn resume(
        &mut self,
        live: &mut LiveSession<'_>,
    ) -> Result<crate::ContinuationStep, Self::Error> {
        match self.stage {
            0 => {
                self.stage = 1;
                live.wait_segment(0.25)
                    .map(crate::ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            1 => {
                self.stage = 2;
                live.declare_and_activate_value_tracker(
                    &self.tracker,
                    2.0,
                    1.0,
                    RateFunction::Linear,
                )
                .map(crate::ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            2 => {
                live.set_value(&self.tracker, 3.0)
                    .map_err(|error| error.to_string())?;
                self.stage = 3;
                live.wait_segment(0.25)
                    .map(crate::ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            3 => {
                self.stage = 4;
                Ok(crate::ContinuationStep::Finished)
            }
            _ => Err("ordinary sparse-read continuation resumed after it finished".to_owned()),
        }
    }
}

#[derive(Debug)]
struct SparseReadExampleError(String);

impl std::fmt::Display for SparseReadExampleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for SparseReadExampleError {}

/// Build the paired sparse callback-read scene for native and direct Rust/WASM hosts.
///
/// A callback on the blue circle reads an unbound, root-scoped scalar and a
/// separate static anchor through the exact pending phase. It first observes the
/// scalar before any track exists, then follows `0 -> 2` over one second and a
/// persistent hold at `3` through the final wait. The callable rejects a repeated
/// invocation at one authored phase time instead of replaying host behavior.
pub fn ordinary_callback_sparse_reads_program() -> Result<
    (
        LiveProgram<OrdinaryCallbackSparseReadsContinuation>,
        RustHostCallbackTable,
    ),
    String,
> {
    let mut scene = Scene::new();
    let mut anchor = scene.circle(0.4).map_err(|error| error.to_string())?;
    anchor
        .set_fill(0.0, 0.4, 1.0, 1.0)
        .map_err(|error| error.to_string())?;
    anchor
        .set_translation(-1.0, 1.0)
        .map_err(|error| error.to_string())?;
    let mut circle = scene.circle(0.4).map_err(|error| error.to_string())?;
    circle
        .set_fill(0.0, 0.4, 1.0, 1.0)
        .map_err(|error| error.to_string())?;
    scene.add(&anchor).map_err(|error| error.to_string())?;
    scene.add(&circle).map_err(|error| error.to_string())?;
    let tracker = scene
        .value_tracker(0.0)
        .map_err(|error| error.to_string())?;

    let tracker_id = tracker.node_id();
    let anchor_id = anchor.node_id();
    let mut observed_phase_times = Vec::new();
    let mut callbacks = RustHostCallbackTable::new();
    callbacks
        .insert(FOLLOW_SPARSE_READS, move |context| {
            if observed_phase_times.contains(&context.time()) {
                return Err(SparseReadExampleError(format!(
                    "sparse-read callback repeated authored phase {}",
                    context.time()
                )));
            }
            observed_phase_times.push(context.time());
            let scalar = context
                .scalar_signal(tracker_id)
                .map_err(|error| SparseReadExampleError(error.to_string()))?;
            let anchor = context
                .read_object(anchor_id)
                .map_err(|error| SparseReadExampleError(error.to_string()))?;
            let center = anchor.bounds.ok_or_else(|| {
                SparseReadExampleError("sparse-read anchor has no layout bounds".to_owned())
            })?;
            let mut transform = context.target_state().transform;
            let center = center.center();
            transform.translation = Vec2::new(center.x + scalar, center.y);
            context
                .set_target_transform(transform)
                .map_err(|error| SparseReadExampleError(error.to_string()))
        })
        .map_err(|error| error.to_string())?;
    callbacks
        .add_updater(
            &mut scene.store().borrow_mut(),
            circle.node_id(),
            FOLLOW_SPARSE_READS,
            0.0,
            None,
        )
        .map_err(|error| error.to_string())?;

    let program = scene
        .into_live_program(OrdinaryCallbackSparseReadsContinuation { tracker, stage: 0 })
        .map_err(|error| error.to_string())?;
    Ok((program, callbacks))
}

/// Ordinary FadeIn/FadeOut continuation used unchanged by native and direct Rust/WASM hosts.
///
/// The circle begins detached, enters and exits through the shared appearance/membership
/// publication, remains absent for one ordinary wait, then re-enters with the same semantic
/// identity. This continuation owns no membership mirror or appearance scheduler.
pub struct OrdinaryFadeContinuation {
    circle: Mobject,
    semantic_id: SemanticNodeId,
    authored_style: SemanticStyle,
    stage: u8,
}

impl LiveContinuation for OrdinaryFadeContinuation {
    type Error = String;

    fn resume(
        &mut self,
        live: &mut LiveSession<'_>,
    ) -> Result<crate::ContinuationStep, Self::Error> {
        let linear = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        match self.stage {
            0 => {
                if live
                    .contains(&self.circle)
                    .map_err(|error| error.to_string())?
                {
                    return Err("FadeIn example target must begin detached".into());
                }
                self.stage = 1;
                live.declare_and_activate_fade(&self.circle, SemanticFadeDirection::In, linear)
                    .map(crate::ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            1 => {
                let effective = live
                    .effective(&self.circle)
                    .map_err(|error| error.to_string())?;
                let authored = live
                    .authored(&self.circle)
                    .map_err(|error| error.to_string())?;
                if !live
                    .contains(&self.circle)
                    .map_err(|error| error.to_string())?
                    || effective.appearance != 1.0
                    || authored.style != self.authored_style
                {
                    return Err("FadeIn did not publish its exact shared endpoint".into());
                }
                self.stage = 2;
                live.declare_and_activate_fade(&self.circle, SemanticFadeDirection::Out, linear)
                    .map(crate::ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            2 => {
                if live
                    .contains(&self.circle)
                    .map_err(|error| error.to_string())?
                    || live.effective(&self.circle).is_ok()
                    || self.circle.node_id() != self.semantic_id
                {
                    return Err("FadeOut did not detach the original semantic handle".into());
                }
                self.stage = 3;
                live.wait_segment(0.25)
                    .map(crate::ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            3 => {
                if live
                    .contains(&self.circle)
                    .map_err(|error| error.to_string())?
                {
                    return Err("fade target re-entered before the detached wait completed".into());
                }
                live.add(&self.circle).map_err(|error| error.to_string())?;
                let effective = live
                    .effective(&self.circle)
                    .map_err(|error| error.to_string())?;
                let authored = live
                    .authored(&self.circle)
                    .map_err(|error| error.to_string())?;
                if self.circle.node_id() != self.semantic_id
                    || !live
                        .contains(&self.circle)
                        .map_err(|error| error.to_string())?
                    || effective.appearance != 1.0
                    || authored.style != self.authored_style
                {
                    return Err("ordinary re-add did not preserve fade identity and style".into());
                }
                self.stage = 4;
                live.wait_segment(0.0)
                    .map(crate::ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            4 => {
                if self.circle.node_id() != self.semantic_id
                    || !live
                        .contains(&self.circle)
                        .map_err(|error| error.to_string())?
                    || live
                        .authored(&self.circle)
                        .map_err(|error| error.to_string())?
                        .style
                        != self.authored_style
                {
                    return Err("re-added fade handle changed before final admission".into());
                }
                self.stage = 5;
                Ok(crate::ContinuationStep::Finished)
            }
            _ => Err("ordinary fade continuation resumed after it finished".into()),
        }
    }
}

/// Build the paired ordinary fade program without selecting a platform host.
///
/// The authored circle stays blue and fully opaque throughout. Only runtime appearance and exact
/// root membership change, and the final ordinary add reuses the original semantic handle.
pub fn ordinary_fade_continuation_program() -> Result<LiveProgram<OrdinaryFadeContinuation>, String>
{
    let scene = Scene::new();
    let mut circle = scene.circle(0.4).map_err(|error| error.to_string())?;
    circle
        .set_fill(0.0, 0.4, 1.0, 1.0)
        .map_err(|error| error.to_string())?;
    let semantic_id = circle.node_id();
    let authored_style = circle.state().map_err(|error| error.to_string())?.style;
    scene
        .into_live_program(OrdinaryFadeContinuation {
            circle,
            semantic_id,
            authored_style,
            stage: 0,
        })
        .map_err(|error| error.to_string())
}

/// Execute paired flat Parallel and Sequence compositions on one live runtime.
///
/// Child intervals, root timing, transaction-local declaration identity, and
/// endpoint release are all owned by the shared compiler/session. This builder
/// is used unchanged by native Rust and direct single-context Rust/WASM hosts.
pub fn ordinary_composition_play() -> Result<ExecutionSession, Box<dyn Error>> {
    let mut scene = Scene::new();
    let mut left = scene.circle(0.4)?;
    left.set_fill(1.0, 1.0, 1.0, 1.0)?;
    left.set_translation(-2.0, 0.0)?;
    let mut right = scene.circle(0.4)?;
    right.set_fill(1.0, 1.0, 1.0, 1.0)?;
    right.set_translation(2.0, 0.0)?;
    let mut left_position = left.target_editor()?;
    left_position.set_translation(-2.0, 1.0)?;
    let mut right_position = right.target_editor()?;
    right_position.set_translation(2.0, -1.0)?;
    scene.add(&left)?;
    scene.add(&right)?;
    let linear = |duration| {
        AnimationOptions::new()
            .run_time(duration)
            .rate_func(RateFunction::Linear)
    };

    let mut session = scene.execution_session()?;
    {
        let mut live = scene.live(&mut session);
        let parallel = [
            TransformToRequest::new(&left, &left_position, linear(2.0)),
            TransformToRequest::new(&right, &right_position, linear(2.0)),
        ];
        let segment = live.declare_and_activate_transform_composition(
            SemanticAnimationCompositionKind::Parallel,
            &parallel,
            AnimationOptions::new()
                .lag_ratio(0.0)
                .rate_func(RateFunction::Linear),
            AnimationOptions::new().rate_func(RateFunction::Linear),
        )?;
        assert_eq!((segment.start_time(), segment.end_time()), (0.0, 2.0));
        live.advance_segment_to(segment, 1.0)?;
        assert_eq!(live.effective_layout(&left)?.center, (-2.0, 0.5));
        assert_eq!(live.effective_layout(&right)?.center, (2.0, -0.5));
        live.advance_segment_to(segment, segment.end_time())?;
        live.complete_segment(segment)?;
        assert_eq!(live.effective_layout(&left)?.center, (-2.0, 1.0));
        assert_eq!(live.effective_layout(&right)?.center, (2.0, -1.0));

        let left_fill = live.target_editor(&left)?;
        live.set_fill(&left_fill, 1.0, 0.0, 0.0, 1.0)?;
        let right_fill = live.target_editor(&right)?;
        live.set_fill(&right_fill, 0.0, 0.0, 1.0, 1.0)?;
        let sequence = [
            TransformToRequest::new(&left, &left_fill, linear(1.0)),
            TransformToRequest::new(&right, &right_fill, linear(1.0)),
        ];
        let segment = live.declare_and_activate_transform_composition(
            SemanticAnimationCompositionKind::Sequence,
            &sequence,
            AnimationOptions::new()
                .lag_ratio(1.0)
                .rate_func(RateFunction::Linear),
            AnimationOptions::new().rate_func(RateFunction::Linear),
        )?;
        assert_eq!((segment.start_time(), segment.end_time()), (2.0, 4.0));
        live.advance_segment_to(segment, 3.0)?;
        assert_eq!(
            live.effective(&left)?.style.fill,
            Some(Color::rgb(1.0, 0.0, 0.0))
        );
        assert_eq!(
            live.effective(&right)?.style.fill,
            Some(Color::rgb(1.0, 1.0, 1.0))
        );
        live.advance_segment_to(segment, segment.end_time())?;
        assert_eq!(
            live.effective(&right)?.style.fill,
            Some(Color::rgb(0.0, 0.0, 1.0))
        );
        live.complete_segment(segment)?;

        // An ordinary edit after root completion proves every mapped driver was released.
        live.set_fill(&left, 0.0, 1.0, 0.0, 1.0)?;
        assert_eq!(
            live.effective(&left)?.style.fill,
            Some(Color::rgb(0.0, 1.0, 0.0))
        );
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
        live.set_fill(&target, 1.0, 0.0, 0.0, 0.4)?;
        live.set_object_opacity(&target, 0.5)?;
        let target_style = live.authored(&target)?.style;

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

        live.set_fill(&circle, 0.0, 1.0, 0.0, 1.0)?;
        live.set_object_opacity(&circle, 1.0)?;
        let final_style = live.authored(&circle)?.style;
        assert_eq!(
            final_style.fill,
            Some(SemanticPaint::Solid(Color::rgb(0.0, 1.0, 0.0)))
        );
        assert_eq!(final_style.fill_opacity, 1.0);
        assert_eq!(final_style.object_opacity, 1.0);
        let effective = live.effective(&circle)?.style;
        assert_eq!(effective.fill, Some(Color::rgb(0.0, 1.0, 0.0)));
        assert_eq!(effective.opacity, 1.0);
    }
    assert_eq!(session.frame().time, 2.0);
    Ok(session)
}

/// Execute ordinary Manim paint animations and a following authored edit.
///
/// Both fill and stroke use the shared color and paint-opacity operations. The
/// exact midpoint and endpoint assertions protect their independent starting
/// colors, while the final yellow edit proves completion released both drivers.
pub fn ordinary_paint_play() -> Result<ExecutionSession, Box<dyn Error>> {
    let mut scene = Scene::new();
    let mut circle = scene.circle(0.4)?;
    circle.set_fill(0.0, 0.0, 1.0, 1.0)?;
    circle.set_stroke_color(1.0, 1.0, 1.0, 1.0)?;
    scene.add(&circle)?;

    let mut session = scene.execution_session()?;
    {
        let mut live = scene.live(&mut session);

        live.set_fill(&circle, 0.0, 0.0, 1.0, 0.75)?;
        let fill_only = live.effective(&circle)?.style;
        assert_eq!(fill_only.fill, Some(Color::rgba(0.0, 0.0, 1.0, 0.75)));
        assert_eq!(fill_only.stroke, Some(Color::rgb(1.0, 1.0, 1.0)));
        live.set_stroke(&circle, 1.0, 1.0, 1.0, 0.4)?;
        let stroke_only = live.effective(&circle)?.style;
        assert_eq!(stroke_only.fill, fill_only.fill);
        assert_eq!(stroke_only.stroke, Some(Color::rgba(1.0, 1.0, 1.0, 0.4)));
        live.set_opacity(&circle, 0.2)?;
        let paint_opacity = live.effective(&circle)?.style;
        assert_eq!(paint_opacity.fill, Some(Color::rgba(0.0, 0.0, 1.0, 0.2)));
        assert_eq!(paint_opacity.stroke, Some(Color::rgba(1.0, 1.0, 1.0, 0.2)));
        assert_eq!(paint_opacity.opacity, 1.0);

        let independent_target = live.target_editor(&circle)?;
        live.set_fill(&independent_target, 1.0, 0.25, 0.75, 0.8)?;
        live.set_stroke(&independent_target, 0.1, 0.8, 0.2, 0.3)?;
        let independent = live.declare_and_activate_transform_to(
            &circle,
            &independent_target,
            AnimationOptions::new()
                .run_time(0.4)
                .rate_func(RateFunction::Linear),
        )?;
        live.advance_segment_to(independent, independent.end_time())?;
        let independent_endpoint = live.effective(&circle)?.style;
        assert_eq!(
            independent_endpoint.fill,
            Some(Color::rgba(1.0, 0.25, 0.75, 0.8))
        );
        assert_eq!(
            independent_endpoint.stroke,
            Some(Color::rgba(0.1, 0.8, 0.2, 0.3))
        );
        assert_eq!(independent_endpoint.opacity, 1.0);
        live.complete_segment(independent)?;
        let independent_authored = live.authored(&circle)?.style;
        assert_eq!(
            independent_authored.fill,
            Some(SemanticPaint::Solid(Color::rgb(1.0, 0.25, 0.75)))
        );
        assert_eq!(
            independent_authored.stroke,
            Some(SemanticPaint::Solid(Color::rgb(0.1, 0.8, 0.2)))
        );
        assert_eq!(independent_authored.fill_opacity, 0.8);
        assert_eq!(independent_authored.stroke_opacity, 0.3);
        assert_eq!(independent_authored.object_opacity, 1.0);

        live.set_fill(&circle, 0.0, 0.0, 1.0, 1.0)?;
        live.set_stroke(&circle, 1.0, 1.0, 1.0, 1.0)?;
        let target = live.target_editor(&circle)?;
        live.set_color(&target, 1.0, 0.0, 0.0, 1.0)?;
        live.set_opacity(&target, 0.5)?;

        let segment = live.declare_and_activate_transform_to(
            &circle,
            &target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )?;
        live.advance_segment_to(segment, 1.4)?;
        let midpoint = live.effective(&circle)?.style;
        assert_eq!(midpoint.fill, Some(Color::rgba(0.5, 0.0, 0.5, 0.75)));
        assert_eq!(midpoint.stroke, Some(Color::rgba(1.0, 0.5, 0.5, 0.75)));
        assert_eq!(midpoint.opacity, 1.0);

        live.advance_segment_to(segment, segment.end_time())?;
        let endpoint = live.effective(&circle)?.style;
        assert_eq!(endpoint.fill, Some(Color::rgba(1.0, 0.0, 0.0, 0.5)));
        assert_eq!(endpoint.stroke, Some(Color::rgba(1.0, 0.0, 0.0, 0.5)));
        assert_eq!(endpoint.opacity, 1.0);
        live.complete_segment(segment)?;

        live.set_color(&circle, 1.0, 1.0, 0.0, 1.0)?;
        live.set_opacity(&circle, 1.0)?;
        let final_style = live.authored(&circle)?.style;
        assert_eq!(
            final_style.fill,
            Some(SemanticPaint::Solid(Color::rgb(1.0, 1.0, 0.0)))
        );
        assert_eq!(
            final_style.stroke,
            Some(SemanticPaint::Solid(Color::rgb(1.0, 1.0, 0.0)))
        );
        assert_eq!(final_style.fill_opacity, 1.0);
        assert_eq!(final_style.stroke_opacity, 1.0);
        assert_eq!(final_style.object_opacity, 1.0);
        let effective = live.effective(&circle)?.style;
        assert_eq!(effective.fill, Some(Color::rgb(1.0, 1.0, 0.0)));
        assert_eq!(effective.stroke, Some(Color::rgb(1.0, 1.0, 0.0)));
        assert_eq!(effective.opacity, 1.0);
    }
    assert_eq!(session.frame().time, 2.4);
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

#[cfg(test)]
mod continuation_tests {
    use super::*;
    use crate::{LiveProgramStatus, TimelineWakeState};

    #[test]
    fn ordinary_affine_continuation_uses_shared_segments_and_publication_admission() {
        let mut program = ordinary_affine_continuation_program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();

        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert!(matches!(
            program.drive_to(&mut callbacks, 1.0).unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert_eq!(
            program.session().frame().objects[0].transform.translation,
            Vec2::new(1.0, -0.5)
        );

        let endpoint = program.drive_to(&mut callbacks, 2.0).unwrap();
        assert!(matches!(endpoint, LiveProgramStatus::PublicationPending(_)));
        let publication = program.take_renderer_publication().context();
        assert!(matches!(
            program.admit_publication(publication).unwrap(),
            LiveProgramStatus::ReadyToResume
        ));

        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert!(matches!(
            program.drive_to(&mut callbacks, 3.0).unwrap(),
            LiveProgramStatus::ReadyToResume
        ));
        assert!(
            !program.session().wake_state().frame_pending(),
            "an unchanged wait endpoint must not create a synthetic renderer publication"
        );

        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert!(matches!(
            program.drive_to(&mut callbacks, 3.5).unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert_eq!(
            program.session().frame().objects[0].transform.translation,
            Vec2::new(4.0, -1.0)
        );
        assert!(matches!(
            program.drive_to(&mut callbacks, 4.0).unwrap(),
            LiveProgramStatus::PublicationPending(_)
        ));
        let publication = program.take_renderer_publication().context();
        program.admit_publication(publication).unwrap();
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        assert_eq!(
            program.session().frame().objects[0].transform.translation,
            Vec2::new(5.0, -1.0)
        );
    }

    #[test]
    fn ordinary_composition_continuation_reuses_one_program_across_segments() {
        let mut program = ordinary_composition_continuation_program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();

        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert!(matches!(
            program.drive_to(&mut callbacks, 1.0).unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert_eq!(
            program.session().frame().objects[0].transform.translation,
            Vec2::new(-2.0, 0.5)
        );
        assert_eq!(
            program.session().frame().objects[1].transform.translation,
            Vec2::new(2.0, -0.5)
        );

        assert!(matches!(
            program.drive_to(&mut callbacks, 2.0).unwrap(),
            LiveProgramStatus::PublicationPending(_)
        ));
        let first_publication = program.take_renderer_publication().context();
        assert!(matches!(
            program.admit_publication(first_publication).unwrap(),
            LiveProgramStatus::ReadyToResume
        ));

        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert!(matches!(
            program.drive_to(&mut callbacks, 3.0).unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert_eq!(
            program.session().frame().objects[0].style.fill,
            Some(Color::rgb(1.0, 0.0, 0.0))
        );
        assert_eq!(
            program.session().frame().objects[1].style.fill,
            Some(Color::rgb(1.0, 1.0, 1.0))
        );

        assert!(matches!(
            program.drive_to(&mut callbacks, 4.0).unwrap(),
            LiveProgramStatus::PublicationPending(_)
        ));
        let second_publication = program.take_renderer_publication().context();
        program.admit_publication(second_publication).unwrap();
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        assert_eq!(
            program.session().frame().objects[0].style.fill,
            Some(Color::rgb(0.0, 1.0, 0.0))
        );
        assert_eq!(
            program.session().frame().objects[1].style.fill,
            Some(Color::rgb(0.0, 0.0, 1.0))
        );
    }

    #[test]
    fn ordinary_callback_continuation_uses_ordered_shared_barriers() {
        let (mut program, mut callbacks) = ordinary_callback_continuation_program().unwrap();

        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert!(matches!(
            program.drive_to(&mut callbacks, 0.5).unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        let midpoint = &program.session().frame().objects[0];
        assert_eq!(midpoint.transform.translation, Vec2::new(1.0, 1.0));
        assert_eq!(midpoint.style.opacity, 0.5);

        assert!(matches!(
            program.drive_to(&mut callbacks, 1.0).unwrap(),
            LiveProgramStatus::PublicationPending(_)
        ));
        let endpoint = &program.session().frame().objects[0];
        assert_eq!(endpoint.transform.translation, Vec2::new(2.0, 1.0));
        assert_eq!(endpoint.style.opacity, 0.5);
        let publication = program.take_renderer_publication().context();
        assert_eq!(
            program.admit_publication(publication).unwrap(),
            LiveProgramStatus::ReadyToResume
        );
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }

    #[test]
    fn ordinary_sparse_reads_cover_initial_signal_track_and_persistent_hold() {
        let (mut program, mut callbacks) = ordinary_callback_sparse_reads_program().unwrap();

        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert!(matches!(
            program.drive_to(&mut callbacks, 0.0).unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert_eq!(
            program.session().frame().objects[1].transform.translation,
            Vec2::new(-1.0, 1.0),
            "the first phase must read the scoped tracker before any track exists"
        );

        let wait_endpoint = program.drive_to(&mut callbacks, 0.25).unwrap();
        match wait_endpoint {
            LiveProgramStatus::PublicationPending(expected) => {
                let publication = program.take_renderer_publication().context();
                assert_eq!(publication, expected);
                assert_eq!(
                    program.admit_publication(publication).unwrap(),
                    LiveProgramStatus::ReadyToResume
                );
            }
            LiveProgramStatus::ReadyToResume => {}
            other => panic!("initial wait did not reach its resume barrier: {other:?}"),
        }
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));

        assert!(matches!(
            program.drive_to(&mut callbacks, 0.75).unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert_eq!(
            program.session().frame().objects[1].transform.translation,
            Vec2::new(0.0, 1.0)
        );
        assert!(matches!(
            program.drive_to(&mut callbacks, 1.25).unwrap(),
            LiveProgramStatus::PublicationPending(_)
        ));
        let track_publication = program.take_renderer_publication().context();
        program.admit_publication(track_publication).unwrap();
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));

        assert!(matches!(
            program.drive_to(&mut callbacks, 1.5).unwrap(),
            LiveProgramStatus::PublicationPending(_)
        ));
        assert_eq!(
            program.session().frame().objects[1].transform.translation,
            Vec2::new(2.0, 1.0),
            "the callback must read the persistent hold during the inactive wait"
        );
        let hold_publication = program.take_renderer_publication().context();
        program.admit_publication(hold_publication).unwrap();
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        assert_eq!(program.session().frame().time, 1.5);
    }

    #[test]
    fn ordinary_fade_continuation_exposes_absent_boundary_then_readds() {
        let mut program = ordinary_fade_continuation_program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert_eq!(program.session().frame().objects[0].appearance, 0.0);
        program.take_renderer_publication();

        assert!(matches!(
            program.drive_to(&mut callbacks, 0.5).unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert_eq!(program.session().frame().objects[0].appearance, 0.5);
        program.take_renderer_publication();

        assert!(matches!(
            program.drive_to(&mut callbacks, 1.0).unwrap(),
            LiveProgramStatus::PublicationPending(_)
        ));
        let fade_in_publication = program.take_renderer_publication().context();
        program.admit_publication(fade_in_publication).unwrap();
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        program.take_renderer_publication();

        assert!(matches!(
            program.drive_to(&mut callbacks, 1.5).unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        let fading_index = program
            .query_viewport(crate::Rect::new(Vec2::new(-1.0, -1.0), Vec2::new(1.0, 1.0)))
            .object_indices()[0];
        assert_eq!(
            program.session().frame().objects[fading_index].appearance,
            0.5
        );
        program.take_renderer_publication();

        assert!(matches!(
            program.drive_to(&mut callbacks, 2.0).unwrap(),
            LiveProgramStatus::PublicationPending(_)
        ));
        assert!(program
            .query_viewport(crate::Rect::new(Vec2::new(-1.0, -1.0), Vec2::new(1.0, 1.0),))
            .object_indices()
            .is_empty());
        let absent_publication = program.take_renderer_publication().context();
        program.admit_publication(absent_publication).unwrap();
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert_eq!(
            program.wake_state().timeline(),
            TimelineWakeState::Deadline(2.25)
        );

        assert_eq!(
            program.drive_to(&mut callbacks, 2.25).unwrap(),
            LiveProgramStatus::ReadyToResume
        );
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        let readded =
            program.query_viewport(crate::Rect::new(Vec2::new(-1.0, -1.0), Vec2::new(1.0, 1.0)));
        assert_eq!(readded.object_indices().len(), 1);
        let index = readded.object_indices()[0];
        assert_eq!(program.session().frame().objects[index].appearance, 1.0);
        program.take_renderer_publication();
        assert_eq!(
            program.drive_to(&mut callbacks, 2.25).unwrap(),
            LiveProgramStatus::ReadyToResume
        );
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }
}

#[cfg(test)]
mod line_callback_tests {
    use super::*;

    #[test]
    fn line_callback_windows_reverse_one_local_effective_transform() {
        let (mut session, mut callbacks) = live_line_callback_rotation().unwrap();
        callbacks.advance_to(&mut session, 0.0).unwrap();
        session.take_frame_changes();
        let siblings = [0, 1, 3].map(|index| session.frame().objects[index].transform);

        callbacks.advance_to(&mut session, 1.0).unwrap();
        assert!((session.frame().objects[2].transform.rotation - 1.0).abs() < 1.0e-6);
        assert_eq!(session.take_frame_changes().object_indices(), &[2]);

        callbacks.advance_to(&mut session, 3.0).unwrap();
        assert!((session.frame().objects[2].transform.rotation + 1.0).abs() < 1.0e-6);
        assert_eq!(session.take_frame_changes().object_indices(), &[2]);
        assert_eq!(
            [0, 1, 3].map(|index| session.frame().objects[index].transform),
            siblings
        );
    }
}
