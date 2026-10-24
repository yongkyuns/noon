use super::*;
use crate::{
    AnimationOptions, ContinuationStep, IndicateOptions, LiveSession, LiveSessionError, Mobject,
};
use noon_core::{Color, RateFunction, Vec2};
use std::{cell::Cell, rc::Rc};

struct Script {
    target: Mobject,
    calls: Rc<Cell<usize>>,
    drops: Rc<Cell<usize>>,
    fail_at: Option<usize>,
    zero_first: bool,
    animate: bool,
}
impl Drop for Script {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
impl LiveContinuation for Script {
    type Error = LiveSessionError;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        let step = self.calls.get();
        self.calls.set(step + 1);
        if self.fail_at == Some(step) {
            // Produce a genuine source error, not a prepared-runtime exception.
            return live.wait_segment(-1.0).map(ContinuationStep::Await);
        }
        if self.zero_first && step == 0 {
            return Ok(ContinuationStep::Await(live.wait_segment(0.0)?));
        }
        let stage = step - usize::from(self.zero_first);
        match stage {
            0 => Ok(ContinuationStep::Await(live.wait_segment(1.0)?)),
            1 => {
                if self.animate {
                    let target = live.target_editor(&self.target)?;
                    live.set_translation(&target, 2.0, 0.0)?;
                    Ok(ContinuationStep::Await(
                        live.declare_and_activate_transform_to(
                            &self.target,
                            &target,
                            AnimationOptions::new()
                                .run_time(1.0)
                                .rate_func(RateFunction::Linear),
                        )?,
                    ))
                } else {
                    live.set_fill(&self.target, 1.0, 0.0, 0.0, 1.0)?;
                    Ok(ContinuationStep::Await(live.wait_segment(1.0)?))
                }
            }
            2 => {
                if self.animate {
                    assert_eq!(
                        live.effective(&self.target)?.transform.translation,
                        Vec2::new(2.0, 0.0)
                    );
                }
                // Same-timestamp final source write must be part of the final batch.
                live.set_fill(&self.target, 0.0, 1.0, 0.0, 1.0)?;
                Ok(ContinuationStep::Finished)
            }
            _ => panic!("finished source was resumed"),
        }
    }
}
fn fixture(
    zero_first: bool,
    animate: bool,
    fail_at: Option<usize>,
    limits: ReplayLimits,
) -> (PreparedSource<Script>, Rc<Cell<usize>>, Rc<Cell<usize>>) {
    let mut scene = Scene::new();
    let mut target = scene.circle(0.5).unwrap();
    target.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
    scene.add(&target).unwrap();
    scene
        .on_click_indicate(
            &target,
            IndicateOptions::default(),
            AnimationOptions::new().run_time(0.5),
        )
        .unwrap();
    let calls = Rc::new(Cell::new(0));
    let drops = Rc::new(Cell::new(0));
    let source = scene
        .into_prepared_source(
            Script {
                target,
                calls: calls.clone(),
                drops: drops.clone(),
                fail_at,
                zero_first,
                animate,
            },
            limits,
        )
        .unwrap();
    (source, calls, drops)
}
fn deliver(source: &mut PreparedSource<Script>, reader: &mut PrefixReader) {
    let batch = source.pending_batch().expect("prepared batch");
    reader
        .reserve_object_capacity(batch.required_object_capacity())
        .unwrap();
    reader.preflight_append(batch).unwrap();
    reader.append(source.take_batch().unwrap()).unwrap();
}
#[test]
fn cooperative_source_yields_once_and_pending_output_applies_backpressure() {
    let (mut source, calls, drops) = fixture(true, false, None, ReplayLimits::default());
    assert_eq!(calls.get(), 0);
    assert_eq!(source.prepare_next().unwrap(), PreparedSourceStatus::Ready);
    assert_eq!(
        calls.get(),
        1,
        "zero wait does not run a busy continuation loop"
    );
    assert_eq!(
        source.prepare_next().unwrap(),
        PreparedSourceStatus::ReaderReady
    );
    for _ in 0..8 {
        assert_eq!(
            source.prepare_next().unwrap(),
            PreparedSourceStatus::ReaderReady
        );
    }
    assert_eq!(calls.get(), 2);
    assert!(source.take_batch().is_none());
    let mut reader = source.take_reader().unwrap();
    assert!(source.take_reader().is_none());
    reader.seek(0.5).unwrap();
    let before = reader.frame().clone();
    let publication = reader.publication_context();
    assert_eq!(
        source.prepare_next().unwrap(),
        PreparedSourceStatus::BatchReady
    );
    for _ in 0..8 {
        assert_eq!(
            source.prepare_next().unwrap(),
            PreparedSourceStatus::BatchReady
        );
    }
    assert_eq!(calls.get(), 3);
    assert!(source.take_reader().is_none());
    deliver(&mut source, &mut reader);
    assert_eq!(reader.frame(), &before);
    assert_eq!(reader.publication_context(), publication);
    assert_eq!(reader.frontier(), 2.0);
    assert_eq!(
        source.prepare_next().unwrap(),
        PreparedSourceStatus::BatchReady
    );
    assert!(source.pending_batch().unwrap().is_terminal());
    assert_eq!(
        drops.get(),
        1,
        "terminal extraction releases the producer immediately"
    );
    deliver(&mut source, &mut reader);
    assert_eq!(source.status(), PreparedSourceStatus::Finished);
    assert_eq!(
        source.prepare_next().unwrap(),
        PreparedSourceStatus::Finished
    );
    assert_eq!(calls.get(), 4);
    reader.seek(2.0).unwrap();
    assert_eq!(
        reader.frame().objects[0].style.fill,
        Some(Color::rgb(0.0, 1.0, 0.0))
    );
    reader.seek(0.5).unwrap();
    assert_eq!(reader.frame(), &before);
}
#[test]
fn cooperative_source_uses_ordinary_animation_completion_before_next_source_read() {
    let (mut source, _, _) = fixture(false, true, None, ReplayLimits::default());
    source.prepare_next().unwrap();
    let mut reader = source.take_reader().unwrap();
    source.prepare_next().unwrap();
    deliver(&mut source, &mut reader);
    reader.seek(1.5).unwrap();
    assert!((reader.frame().objects[0].transform.translation.x - 1.0).abs() < 1e-6);
    source.prepare_next().unwrap();
    deliver(&mut source, &mut reader);
    reader.seek(2.0).unwrap();
    assert_eq!(
        reader.frame().objects[0].transform.translation,
        Vec2::new(2.0, 0.0)
    );
    reader.seek(0.5).unwrap();
    assert_eq!(reader.frame().objects[0].transform.translation, Vec2::ZERO);
}
#[test]
fn cooperative_source_failure_never_retries_and_preserves_delivered_history() {
    let (mut source, calls, drops) = fixture(false, false, Some(1), ReplayLimits::default());
    source.prepare_next().unwrap();
    let mut reader = source.take_reader().unwrap();
    reader.seek(0.5).unwrap();
    let before = reader.frame().clone();
    assert!(matches!(
        source.prepare_next(),
        Err(PreparedSourceError::Program(_))
    ));
    assert_eq!(source.status(), PreparedSourceStatus::Failed);
    assert_eq!(drops.get(), 1);
    for _ in 0..8 {
        assert_eq!(source.prepare_next().unwrap(), PreparedSourceStatus::Failed);
    }
    assert_eq!(calls.get(), 2);
    assert_eq!(reader.end_time(), None);
    assert_eq!(reader.frame(), &before);
    let object = reader.frame().objects[0].id;
    let action = reader
        .prepare_bound_click_indicate(object)
        .unwrap()
        .unwrap();
    reader.start_transient_animation(action).unwrap();
    reader.advance_interactions(10.0).unwrap();
    reader.advance_interactions(10.25).unwrap();
    assert_ne!(reader.frame(), &before);
    reader.advance_interactions(10.5).unwrap();
    assert_eq!(reader.frame(), &before);
}
#[test]
fn cooperative_source_cancel_discards_pending_only_and_is_idempotent() {
    let (mut source, calls, drops) = fixture(false, false, None, ReplayLimits::default());
    source.prepare_next().unwrap();
    let mut reader = source.take_reader().unwrap();
    source.prepare_next().unwrap();
    assert!(source.pending_batch().is_some());
    source.cancel();
    source.cancel();
    assert_eq!(
        source.prepare_next().unwrap(),
        PreparedSourceStatus::Cancelled
    );
    assert_eq!(calls.get(), 2);
    assert_eq!(drops.get(), 1);
    assert!(source.pending_batch().is_none());
    assert_eq!(reader.frontier(), 1.0);
    assert_eq!(reader.end_time(), None);
    reader.seek(0.5).unwrap();
}
#[test]
fn cooperative_source_foreign_delivery_retains_the_same_suffix_for_correct_reader() {
    let (mut source, calls, _) = fixture(false, false, None, ReplayLimits::default());
    let (mut other, _, _) = fixture(false, false, None, ReplayLimits::default());
    source.prepare_next().unwrap();
    let mut reader = source.take_reader().unwrap();
    other.prepare_next().unwrap();
    let foreign = other.take_reader().unwrap();
    source.prepare_next().unwrap();
    assert!(matches!(
        foreign.preflight_append(source.pending_batch().unwrap()),
        Err(PrefixError::UnexpectedBatch)
    ));
    assert_eq!(
        source.prepare_next().unwrap(),
        PreparedSourceStatus::BatchReady
    );
    assert_eq!(calls.get(), 2);
    deliver(&mut source, &mut reader);
    assert_eq!(reader.frontier(), 2.0);
}
#[test]
fn cooperative_source_retention_failure_does_not_publish_a_false_final_range() {
    let (mut source, calls, drops) = fixture(
        false,
        false,
        None,
        ReplayLimits {
            revisions: 0,
            payloads: 0,
        },
    );
    source.prepare_next().unwrap();
    let reader = source.take_reader().unwrap();
    assert!(source.prepare_next().is_err());
    assert_eq!(source.status(), PreparedSourceStatus::Failed);
    assert_eq!(calls.get(), 2);
    assert_eq!(drops.get(), 1);
    assert_eq!(reader.end_time(), None);
}
#[test]
fn cooperative_source_rejects_reactive_input_before_invoking_source() {
    let mut scene = Scene::new();
    let target = scene.circle(0.5).unwrap();
    scene.add(&target).unwrap();
    let signal = scene.control_signal("opacity", 1.0).unwrap();
    scene.bind_opacity(&target, &signal).unwrap();
    let calls = Rc::new(Cell::new(0));
    let drops = Rc::new(Cell::new(0));
    let result = scene.into_prepared_source(
        Script {
            target,
            calls: calls.clone(),
            drops: drops.clone(),
            fail_at: None,
            zero_first: false,
            animate: false,
        },
        ReplayLimits::default(),
    );
    assert!(matches!(result, Err(PreparedSourceError::Prefix(_))));
    assert_eq!(calls.get(), 0);
    assert_eq!(drops.get(), 1);
}
#[test]
fn cooperative_source_cancel_before_start_does_not_call_user_code() {
    let (mut source, calls, drops) = fixture(false, false, None, ReplayLimits::default());
    source.cancel();
    assert_eq!(
        source.prepare_next().unwrap(),
        PreparedSourceStatus::Cancelled
    );
    assert_eq!(calls.get(), 0);
    assert_eq!(drops.get(), 1);
}

#[test]
fn cooperative_source_demand_sleeps_at_frontier_and_admits_only_one_unknown_segment() {
    let (mut source, calls, drops) = fixture(false, false, None, ReplayLimits::default());
    assert_eq!(
        source.prepare_until(0.0).unwrap(),
        Some(PreparedSourceStatus::ReaderReady)
    );
    let mut reader = source.take_reader().unwrap();
    reader.seek(0.5).unwrap();
    let before = reader.frame().clone();
    let publication = reader.publication_context();
    for target in [1.0, 0.5, 0.0, 1.0] {
        assert_eq!(source.prepare_until(target).unwrap(), None);
        assert_eq!(source.status(), PreparedSourceStatus::Ready);
    }
    assert_eq!(calls.get(), 1);
    assert_eq!(drops.get(), 0);
    assert!(source.pending_batch().is_none());
    assert_eq!(reader.end_time(), None);
    assert_eq!(reader.frame(), &before);
    assert_eq!(reader.publication_context(), publication);
    // A segment is indivisible: a 1.1 demand can admit the next endpoint at 2.0.
    assert_eq!(
        source.prepare_until(1.1).unwrap(),
        Some(PreparedSourceStatus::BatchReady)
    );
    assert_eq!(calls.get(), 2);
    // Pending output is not hidden or recomputed when demand decreases.
    assert_eq!(
        source.prepare_until(0.0).unwrap(),
        Some(PreparedSourceStatus::BatchReady)
    );
    assert_eq!(calls.get(), 2);
    deliver(&mut source, &mut reader);
    assert_eq!(reader.frontier(), 2.0);
    assert_eq!(source.prepare_until(2.0).unwrap(), None);
    assert_eq!(
        reader.end_time(),
        None,
        "throttling must not invent completion"
    );
    assert_eq!(
        source.prepare_until(2.1).unwrap(),
        Some(PreparedSourceStatus::BatchReady)
    );
    assert!(source.pending_batch().unwrap().is_terminal());
    deliver(&mut source, &mut reader);
    assert_eq!(
        source.prepare_until(0.0).unwrap(),
        Some(PreparedSourceStatus::Finished)
    );
    assert_eq!(calls.get(), 3);
    assert_eq!(drops.get(), 1);
    assert_eq!(reader.end_time(), Some(2.0));
    assert_eq!(reader.frame(), &before);
}

#[test]
fn cooperative_source_invalid_demand_is_atomic_before_and_after_output() {
    let (mut source, calls, _) = fixture(false, false, None, ReplayLimits::default());
    for target in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        assert!(matches!(
            source.prepare_until(target),
            Err(PreparedSourceError::InvalidPreparationTarget(_))
        ));
        assert_eq!(source.status(), PreparedSourceStatus::Ready);
    }
    assert_eq!(calls.get(), 0);
    source.prepare_next().unwrap();
    let mut reader = source.take_reader().unwrap();
    source.prepare_until(1.5).unwrap();
    let batch = std::ptr::from_ref(source.pending_batch().unwrap());
    for target in [f64::NAN, f64::INFINITY, -1.0] {
        assert!(source.prepare_until(target).is_err());
        assert_eq!(std::ptr::from_ref(source.pending_batch().unwrap()), batch);
        assert_eq!(source.status(), PreparedSourceStatus::BatchReady);
    }
    assert_eq!(calls.get(), 2);
    deliver(&mut source, &mut reader);
    assert_eq!(reader.frontier(), 2.0);
}

#[test]
fn cooperative_source_waiting_does_not_touch_effects_or_retry_a_later_failure() {
    let (mut source, calls, drops) = fixture(false, false, Some(1), ReplayLimits::default());
    source.prepare_next().unwrap();
    let mut reader = source.take_reader().unwrap();
    reader.seek(0.5).unwrap();
    let baseline = reader.frame().clone();
    let action = reader
        .prepare_bound_click_indicate(reader.frame().objects[0].id)
        .unwrap()
        .unwrap();
    reader.start_transient_animation(action).unwrap();
    reader.advance_interactions(10.0).unwrap();
    reader.advance_interactions(10.25).unwrap();
    let middle = reader.frame().clone();
    let publication = reader.publication_context();
    let history = reader.replay_stats();
    assert_ne!(middle, baseline);
    for _ in 0..32 {
        assert_eq!(source.prepare_until(1.0).unwrap(), None);
    }
    assert_eq!(calls.get(), 1);
    assert_eq!(reader.frame(), &middle);
    assert_eq!(reader.publication_context(), publication);
    assert_eq!(reader.replay_stats(), history);
    assert!(source.prepare_until(1.1).is_err());
    assert_eq!(
        source.prepare_until(100.0).unwrap(),
        Some(PreparedSourceStatus::Failed)
    );
    assert_eq!(calls.get(), 2);
    assert_eq!(drops.get(), 1);
    reader.advance_interactions(10.5).unwrap();
    assert_eq!(reader.frame(), &baseline);
    assert_eq!(reader.end_time(), None);
}

#[test]
fn cooperative_source_demand_keeps_zero_bootstrap_cooperative_and_cancel_terminal() {
    let (mut source, calls, drops) = fixture(true, false, None, ReplayLimits::default());
    assert_eq!(
        source.prepare_until(0.0).unwrap(),
        Some(PreparedSourceStatus::Ready)
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(
        source.prepare_until(0.0).unwrap(),
        Some(PreparedSourceStatus::ReaderReady)
    );
    assert_eq!(calls.get(), 2);
    let reader = source.take_reader().unwrap();
    assert_eq!(source.prepare_until(0.0).unwrap(), None);
    source.cancel();
    assert_eq!(
        source.prepare_until(999.0).unwrap(),
        Some(PreparedSourceStatus::Cancelled)
    );
    assert_eq!(calls.get(), 2);
    assert_eq!(drops.get(), 1);
    assert_eq!(reader.frontier(), 1.0);
    assert_eq!(reader.end_time(), None);
}

#[cfg(all(panic = "unwind", not(target_arch = "wasm32")))]
mod unwind;
