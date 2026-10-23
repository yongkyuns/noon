use super::*;
use crate::integration::FrameRate;
use crate::Scene;

fn options() -> ExportFrameOptions {
    ExportFrameOptions {
        frame_rate: FrameRate::new(30, 1).unwrap(),
        start_frame: 0,
        stop: ExportStop::SourceEnd,
        max_frames: 128,
        max_transitions_per_sample: 32,
        final_hold_seconds: 0.0,
    }
}

fn execute(
    options: ExportFrameOptions,
    source_end: f64,
    publication: PublicationContext,
) -> Result<ExportFrameSummary, ExportFramePolicyError> {
    let mut policy = ExportFramePolicy::new(options)?;
    loop {
        match policy.status()? {
            ExportFramePolicyStatus::NeedsSample(requested_time) => {
                policy.observe(
                    SampleObservation {
                        requested_time,
                        published_time: requested_time.min(source_end),
                        publication,
                    },
                    requested_time >= source_end,
                )?;
            }
            ExportFramePolicyStatus::SampleReady(sample) => {
                policy.acknowledge_sample(sample)?;
            }
            ExportFramePolicyStatus::Complete(summary) => return Ok(summary),
        }
    }
}

#[test]
fn finite_plan_equals_executed_policy_across_crops_stops_and_holds() {
    let publication = Scene::new()
        .execution_session()
        .unwrap()
        .publication_context();
    for (p, q) in [(30, 1), (30_000, 1_001), (60_000, 1_001)] {
        let rate = FrameRate::new(p, q).unwrap();
        let grid = FrameGrid::new(rate, 0.0).unwrap();
        let boundary = grid.sample(10).unwrap().authored_time();
        for start_frame in [0, 3, 10] {
            let start = grid.sample(start_frame).unwrap().authored_time();
            for source_end in [
                0.0,
                0.01,
                0.312,
                boundary.next_down(),
                boundary,
                boundary.next_up(),
                1.05,
            ] {
                for final_hold_seconds in [0.0, 0.1, 0.8] {
                    for stop in [
                        ExportStop::SourceEnd,
                        ExportStop::FrameCount(17),
                        ExportStop::EndTime(start + 0.115),
                        ExportStop::EndTime(grid.sample(start_frame + 19).unwrap().authored_time()),
                    ] {
                        let config = ExportFrameOptions {
                            frame_rate: rate,
                            start_frame,
                            stop,
                            final_hold_seconds,
                            ..options()
                        };
                        let policy = ExportFramePolicy::new(config).unwrap();
                        assert_eq!(
                            policy.finite_summary(source_end),
                            execute(config, source_end, publication),
                            "config={config:?}, source_end={source_end:?}"
                        );
                        assert_eq!(
                            policy.status(),
                            Ok(ExportFramePolicyStatus::NeedsSample(0.0))
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn explicit_stop_does_not_claim_an_unobserved_source_end() {
    let policy = ExportFramePolicy::new(ExportFrameOptions {
        stop: ExportStop::EndTime(0.115),
        ..options()
    })
    .unwrap();
    let plan = policy.finite_summary(2.0).unwrap();
    assert_eq!(plan.frames, 4);
    assert_eq!(plan.start_time, 0.0);
    assert_eq!(plan.end_time, 0.115);
    assert_eq!(plan.scheduled_duration, 4.0 / 30.0);
    assert_eq!(plan.source_end, None);
    assert_eq!(plan.reason, ExportEndReason::RequestedStop);

    // An unusable hold is irrelevant if the source is never allowed to end.
    let policy = ExportFramePolicy::new(ExportFrameOptions {
        final_hold_seconds: f64::MAX,
        stop: ExportStop::FrameCount(1),
        ..options()
    })
    .unwrap();
    assert_eq!(policy.finite_summary(f64::MAX).unwrap().source_end, None);
}

#[test]
fn natural_endpoint_and_crop_keep_the_original_fractional_grid() {
    let policy = ExportFramePolicy::new(ExportFrameOptions {
        frame_rate: FrameRate::new(60_000, 1_001).unwrap(),
        start_frame: 17,
        ..options()
    })
    .unwrap();
    let plan = policy.finite_summary(2.0).unwrap();
    assert_eq!(plan.frames, 103);
    assert_eq!(plan.start_time, 17_017.0 / 60_000.0);
    assert_eq!(plan.end_time, 2.0);
    assert_eq!(plan.source_end, Some(2.0));
    assert_eq!(plan.scheduled_duration, 103_103.0 / 60_000.0);
    assert_eq!(plan.reason, ExportEndReason::SourceEnd);
}

#[test]
fn crop_after_source_end_can_still_include_an_explicit_hold() {
    let policy = ExportFramePolicy::new(ExportFrameOptions {
        start_frame: 15,
        final_hold_seconds: 0.6,
        ..options()
    })
    .unwrap();
    let plan = policy.finite_summary(0.25).unwrap();
    assert_eq!(plan.frames, 11);
    assert_eq!(plan.start_time, 0.5);
    assert_eq!(plan.end_time, 0.85);
    assert_eq!(plan.source_end, Some(0.25));
    assert_eq!(plan.scheduled_duration, 11.0 / 30.0);
    assert_eq!(plan.reason, ExportEndReason::SourceEnd);
}

#[test]
fn endpoint_ties_and_stops_inside_holds_keep_the_shared_reason() {
    for (hold, expected) in [
        (0.0, ExportEndReason::SourceEnd),
        (0.25, ExportEndReason::RequestedStop),
    ] {
        let policy = ExportFramePolicy::new(ExportFrameOptions {
            stop: ExportStop::EndTime(0.5),
            final_hold_seconds: hold,
            ..options()
        })
        .unwrap();
        let plan = policy.finite_summary(0.5).unwrap();
        assert_eq!(plan.frames, 15);
        assert_eq!(plan.source_end, Some(0.5));
        assert_eq!(plan.reason, expected);
    }
}

#[test]
fn finite_plan_rejects_empty_capped_invalid_and_collapsed_hold_ranges() {
    let policy = ExportFramePolicy::new(ExportFrameOptions {
        max_frames: 30,
        ..options()
    })
    .unwrap();
    assert_eq!(policy.finite_summary(1.0).unwrap().frames, 30);
    assert_eq!(
        policy.finite_summary(1.0_f64.next_up()),
        Err(ExportFramePolicyError::FrameLimit)
    );
    assert_eq!(
        policy.finite_summary(0.0),
        Err(ExportFramePolicyError::EmptyInterval)
    );
    for end in [-1.0, f64::NEG_INFINITY, f64::INFINITY, f64::NAN] {
        assert_eq!(
            policy.finite_summary(end),
            Err(ExportFramePolicyError::InvalidSourceEnd)
        );
    }
    assert_eq!(
        policy.status(),
        Ok(ExportFramePolicyStatus::NeedsSample(0.0))
    );

    let policy = ExportFramePolicy::new(ExportFrameOptions {
        final_hold_seconds: f64::EPSILON / 2.0,
        ..options()
    })
    .unwrap();
    assert_eq!(
        policy.finite_summary(1.0),
        Err(ExportFramePolicyError::InvalidHold)
    );
}

#[test]
fn finite_planning_requires_admission_before_any_observation() {
    let publication = Scene::new()
        .execution_session()
        .unwrap()
        .publication_context();
    let mut policy = ExportFramePolicy::new(options()).unwrap();
    let sample = policy
        .observe(
            SampleObservation {
                requested_time: 0.0,
                published_time: 0.0,
                publication,
            },
            false,
        )
        .unwrap();
    assert_eq!(
        policy.finite_summary(1.0),
        Err(ExportFramePolicyError::NotFresh)
    );
    policy.acknowledge_sample(sample).unwrap();
    assert_eq!(
        policy.finite_summary(1.0),
        Err(ExportFramePolicyError::NotFresh)
    );
    let mut policy = ExportFramePolicy::new(options()).unwrap();
    policy.cancel();
    assert_eq!(
        policy.finite_summary(1.0),
        Err(ExportFramePolicyError::NotFresh)
    );
}
