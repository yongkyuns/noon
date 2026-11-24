use super::*;
use noon::integration::ReplayError;

fn host(limits: BlockingSourceLimits) -> Rc<BlockingExportHost> {
    BlockingExportHost::new(CaptureOptions::new(64, 64), limits).unwrap()
}

fn present(scene: &SceneExecution) -> usize {
    let frame = scene.session().frame();
    (0..frame.objects.len())
        .filter(|&index| frame.is_present(index))
        .count()
}

#[test]
fn no_boundary_source_transfers_its_original_runtime_and_time_zero_membership() {
    let host = host(BlockingSourceLimits::default());
    let source = host.capability();
    let identity = host
        .state
        .borrow()
        .program
        .as_ref()
        .unwrap()
        .session()
        .runtime_identity();
    let square = source.square(0.8).unwrap();
    source.add(&square).unwrap();
    let mut scene = host.finish().unwrap();
    assert_eq!(scene.session().runtime_identity(), identity);
    assert_eq!(scene.session().replay_end_time(), Some(0.0));
    scene.seek(0.0).unwrap();
    assert_eq!(present(&scene), 1);
    assert!(source.time().is_err());
    assert!(source.wait(0.0).is_err());
}

#[test]
fn final_late_resource_edit_is_retained_and_failed_preparation_is_catchable() {
    let host = host(BlockingSourceLimits::default());
    let source = host.capability();
    let square = source.square(0.8).unwrap();
    source.add(&square).unwrap();
    source.wait(0.25).unwrap();
    let before = host
        .state
        .borrow()
        .program
        .as_ref()
        .unwrap()
        .session()
        .publication_context();
    assert!(source.image_rgba8(2, 1, vec![255; 4]).is_err());
    assert_eq!(
        host.state
            .borrow()
            .program
            .as_ref()
            .unwrap()
            .session()
            .publication_context(),
        before
    );
    let image = source.image_rgba8(1, 1, vec![255; 4]).unwrap();
    source.add(&image).unwrap();
    // No extra wait is needed to retain the final structural/resource edit.
    let mut scene = host.finish().unwrap();
    scene.seek(0.0).unwrap();
    assert_eq!(present(&scene), 1);
    scene.seek(0.25).unwrap();
    assert_eq!(present(&scene), 2);
}

#[cfg(all(feature = "native-text", feature = "typst", feature = "bundled-fonts"))]
#[test]
fn exact_first_lesson_retains_all_time_zero_resources_and_the_late_caption() {
    let host = host(BlockingSourceLimits::default());
    let source = host.capability();
    noon::example_scenes::first_lesson::construct(&source).unwrap();
    assert_eq!(source.time().unwrap(), 2.0);
    let mut scene = host.finish().unwrap();
    for (time, count) in [(0.0, 4), (0.5, 4), (1.49, 4), (1.5, 5), (2.0, 5), (0.0, 4)] {
        scene.seek(time).unwrap();
        assert_eq!(present(&scene), count, "time={time}");
    }
}

#[cfg(all(feature = "native-text", feature = "typst", feature = "bundled-fonts"))]
#[test]
fn text_after_the_last_boundary_is_retained_without_an_extra_wait() {
    let host = host(BlockingSourceLimits::default());
    let source = host.capability();
    source.wait(0.25).unwrap();
    let before = host
        .state
        .borrow()
        .program
        .as_ref()
        .unwrap()
        .session()
        .publication_context();
    assert!(source.math_typst(noon::MathTypst::new("frac(")).is_err());
    assert_eq!(
        host.state
            .borrow()
            .program
            .as_ref()
            .unwrap()
            .session()
            .publication_context(),
        before
    );
    let text = source.text("last source operation").unwrap();
    source.add(&text).unwrap();
    let mut scene = host.finish().unwrap();
    scene.seek(0.0).unwrap();
    assert_eq!(present(&scene), 0);
    scene.seek(0.25).unwrap();
    assert_eq!(present(&scene), 1);
}

#[test]
fn rejected_callback_registration_never_executes_and_preserves_the_scene() {
    let host = host(BlockingSourceLimits::default());
    let source = host.capability();
    let square = source.square(0.8).unwrap();
    source.add(&square).unwrap();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    assert!(source
        .add_updater(&square, move |_, _| {
            observed.set(observed.get() + 1);
            Ok(())
        })
        .is_err());
    assert!(host
        .state
        .borrow()
        .program
        .as_ref()
        .unwrap()
        .session()
        .source_execution_requirement()
        .is_none());
    source.wait(0.25).unwrap();
    let scene = host.finish().unwrap();
    assert_eq!(present(&scene), 1);
    assert_eq!(calls.get(), 0);
    assert!(host.take_failure().is_none());
}

#[test]
fn operation_and_zero_duration_boundary_limits_are_separate_and_terminal() {
    for boundaries in [false, true] {
        let limits = if boundaries {
            BlockingSourceLimits {
                max_boundaries: 2,
                ..Default::default()
            }
        } else {
            BlockingSourceLimits {
                max_operations: 2,
                ..Default::default()
            }
        };
        let host = host(limits);
        let source = host.capability();
        for _ in 0..2 {
            if boundaries {
                source.wait(0.0).unwrap();
            } else {
                source.time().unwrap();
            }
        }
        assert!(if boundaries {
            source.wait(0.0).is_err()
        } else {
            source.time().is_err()
        });
        assert!(source.square(0.1).is_err());
        assert!(matches!(
            host.take_failure(),
            Some(BlockingFileExportError::SourceLimit { limit: 2, .. })
        ));
    }
}

#[test]
fn retention_limit_reports_the_original_journal_failure_immediately() {
    let host = host(BlockingSourceLimits {
        history: ReplayLimits {
            revisions: 0,
            payloads: 0,
        },
        ..Default::default()
    });
    let source = host.capability();
    let result = source.square(0.8).and_then(|square| source.add(&square));
    assert!(result.is_err());
    assert!(matches!(
        host.take_failure(),
        Some(BlockingFileExportError::Preparation(
            BlockingReplayError::Replay(ReplayError::RetentionLimit)
        ))
    ));
    assert!(source.wait(1.0).is_err());
}

#[test]
fn invalid_wait_is_recoverable_but_cancellation_cannot_be_swallowed() {
    let host = host(BlockingSourceLimits::default());
    let source = host.capability();
    assert!(source.wait(f64::NAN).is_err());
    source.wait(0.25).unwrap();
    assert_eq!(source.time().unwrap(), 0.25);
    host.capture.cancellation.cancel();
    assert!(source.time().is_err());
    assert!(source.wait(0.0).is_err());
    assert!(matches!(
        host.take_failure(),
        Some(BlockingFileExportError::Cancelled)
    ));
}

#[test]
fn caught_host_call_panic_retires_the_source() {
    struct PanickingPixels;
    impl From<PanickingPixels> for std::sync::Arc<[u8]> {
        fn from(_: PanickingPixels) -> Self {
            panic!("pixel preparation panic")
        }
    }
    let host = host(BlockingSourceLimits::default());
    let source = host.capability();
    assert!(catch_unwind(AssertUnwindSafe(|| source.image_rgba8(
        1,
        1,
        PanickingPixels
    )))
    .is_err());
    assert!(source.square(0.1).is_err());
    assert!(matches!(
        host.take_failure(),
        Some(BlockingFileExportError::SourcePanicked)
    ));
}

#[test]
fn source_capture_destructors_run_before_final_owner_validation() {
    struct MutateOnDrop(noon::Mobject);
    impl Drop for MutateOnDrop {
        fn drop(&mut self) {
            self.0.set_translation(5.0, 0.0).unwrap();
        }
    }
    let host = host(BlockingSourceLimits::default());
    let source = host.capability();
    let square = source.square(0.8).unwrap();
    source.add(&square).unwrap();
    let mutation = MutateOnDrop(square);
    // An integration host can own a callable that has no scheduled invocation.
    // Its arbitrary teardown still cannot invalidate a successfully handed-off pair.
    host.with_scene(&mut {
        let mut mutation = Some(mutation);
        move |_, callbacks| {
            let mutation = mutation.take().unwrap();
            callbacks.insert(noon_core::HostCallbackId::new(731), move |_| {
                let _ = &mutation;
                Ok::<(), ScriptError>(())
            })?;
            Ok(())
        }
    })
    .unwrap();
    assert!(host.finish().is_err());
    assert!(source.time().is_err());
}
