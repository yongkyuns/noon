//! A native host may catch an unwinding panic; it must not revive the producer.
//! Abort/trap targets are intentionally outside this in-process guarantee.
use super::*;
use std::panic::{catch_unwind, AssertUnwindSafe};

struct PanicScript {
    target: Mobject,
    calls: Rc<Cell<usize>>,
    drops: Rc<Cell<usize>>,
    panic_at: Option<usize>,
    panic_on_drop: bool,
}
impl Drop for PanicScript {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
        if self.panic_on_drop {
            panic!("continuation destructor panic");
        }
    }
}
impl LiveContinuation for PanicScript {
    type Error = LiveSessionError;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        let call = self.calls.get();
        self.calls.set(call + 1);
        if self.panic_at == Some(call) {
            // Commit real source-side work before unwinding. The immutable reader
            // must not receive this unpublished suffix, and it cannot be retried.
            live.set_fill(&self.target, 1.0, 0.0, 0.0, 1.0)?;
            panic!("continuation body panic");
        }
        if self.panic_on_drop && call == 1 {
            return Ok(ContinuationStep::Finished);
        }
        Ok(ContinuationStep::Await(live.wait_segment(1.0)?))
    }
}
fn panic_fixture(
    panic_at: Option<usize>,
    panic_on_drop: bool,
) -> (
    PreparedSource<PanicScript>,
    Rc<Cell<usize>>,
    Rc<Cell<usize>>,
) {
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
            PanicScript {
                target,
                calls: calls.clone(),
                drops: drops.clone(),
                panic_at,
                panic_on_drop,
            },
            ReplayLimits::default(),
        )
        .unwrap();
    (source, calls, drops)
}
fn assert_failed(source: &mut PreparedSource<PanicScript>) {
    assert_eq!(source.status(), PreparedSourceStatus::Failed);
    assert!(source.take_reader().is_none());
    assert!(source.take_batch().is_none());
    for _ in 0..8 {
        assert_eq!(source.prepare_next().unwrap(), PreparedSourceStatus::Failed);
        assert_eq!(
            source.prepare_until(100.0).unwrap(),
            Some(PreparedSourceStatus::Failed)
        );
    }
    source.cancel();
    assert_eq!(source.status(), PreparedSourceStatus::Failed);
}
fn expect_panic(source: &mut PreparedSource<PanicScript>, expected: &'static str) {
    // Only the host/test catches. Product code must not suppress or replace the
    // panic payload, resume user code, or claim the source completed normally.
    let payload = catch_unwind(AssertUnwindSafe(|| source.prepare_next())).unwrap_err();
    assert_eq!(payload.downcast_ref::<&str>().copied(), Some(expected));
}

#[test]
fn cooperative_source_unwind_before_bootstrap_releases_without_retry() {
    let (mut source, calls, drops) = panic_fixture(Some(0), false);
    expect_panic(&mut source, "continuation body panic");
    assert_failed(&mut source);
    assert_eq!(calls.get(), 1);
    assert_eq!(drops.get(), 1);
    drop(source);
    assert_eq!(drops.get(), 1);
}

#[test]
fn cooperative_source_unwind_preserves_displayed_effect_and_open_history() {
    let (mut source, calls, drops) = panic_fixture(Some(1), false);
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
    let midpoint = reader.frame().clone();
    let publication = reader.publication_context();
    let history = reader.replay_stats();
    assert_ne!(midpoint, baseline);

    expect_panic(&mut source, "continuation body panic");
    assert_failed(&mut source);
    assert_eq!((calls.get(), drops.get()), (2, 1));
    assert_eq!(reader.frame(), &midpoint);
    assert_eq!(reader.publication_context(), publication);
    assert_eq!(reader.replay_stats(), history);
    assert_eq!(reader.frontier(), 1.0);
    assert_eq!(reader.end_time(), None);
    assert!(reader.seek(1.0).is_err());
    assert_eq!(reader.frame(), &midpoint);
    reader.advance_interactions(10.5).unwrap();
    assert_eq!(reader.frame(), &baseline);
    reader.seek(0.25).unwrap();
    reader.seek(0.5).unwrap();
    assert_eq!(reader.frame(), &baseline);
}

#[test]
fn cooperative_source_unwind_does_not_revoke_a_previously_transferred_batch() {
    let (mut source, calls, drops) = panic_fixture(Some(2), false);
    source.prepare_next().unwrap();
    let mut reader = source.take_reader().unwrap();
    reader.seek(0.5).unwrap();
    let baseline = reader.frame().clone();
    let publication = reader.publication_context();
    source.prepare_next().unwrap();
    for _ in 0..8 {
        assert_eq!(
            source.prepare_next().unwrap(),
            PreparedSourceStatus::BatchReady
        );
    }
    assert_eq!(
        calls.get(),
        2,
        "backpressure must not invoke the panicking step"
    );
    let batch = source.take_batch().unwrap();
    expect_panic(&mut source, "continuation body panic");
    assert_failed(&mut source);
    assert_eq!((calls.get(), drops.get()), (3, 1));

    reader
        .reserve_object_capacity(batch.required_object_capacity())
        .unwrap();
    reader.preflight_append(&batch).unwrap();
    reader.append(batch).unwrap();
    assert_eq!(reader.frontier(), 2.0);
    assert_eq!(reader.end_time(), None);
    assert_eq!(reader.frame(), &baseline);
    assert_eq!(reader.publication_context(), publication);
    reader.seek(1.5).unwrap();
    assert_eq!(
        reader.frame().objects[0].style.fill,
        Some(Color::rgb(0.0, 0.0, 1.0))
    );
}

#[test]
fn cooperative_source_unwind_during_terminal_cleanup_cannot_expose_completion() {
    let (mut source, calls, drops) = panic_fixture(None, true);
    source.prepare_next().unwrap();
    let mut reader = source.take_reader().unwrap();
    reader.seek(0.5).unwrap();
    let baseline = reader.frame().clone();
    let publication = reader.publication_context();
    // A terminal suffix is staged before the completed continuation is dropped.
    // A panic from that drop must discard this unreturned suffix, not leak it as
    // BatchReady/Finished after the host observes failed preparation.
    expect_panic(&mut source, "continuation destructor panic");
    assert_failed(&mut source);
    assert_eq!((calls.get(), drops.get()), (2, 1));
    assert_eq!(reader.end_time(), None);
    assert_eq!(reader.frontier(), 1.0);
    assert_eq!(reader.frame(), &baseline);
    assert_eq!(reader.publication_context(), publication);
    drop(source);
    assert_eq!(drops.get(), 1);
}
