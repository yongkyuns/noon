use super::*;
use crate::integration::FrameRate;
use crate::Scene;

fn options() -> ExportFrameOptions {
    ExportFrameOptions {
        frame_rate: FrameRate::new(30, 1).unwrap(),
        start_frame: 0,
        stop: ExportStop::SourceEnd,
        max_frames: 1_000,
        max_transitions_per_sample: 32,
        final_hold_seconds: 0.0,
    }
}

fn publication() -> PublicationContext {
    Scene::new()
        .execution_session()
        .unwrap()
        .publication_context()
}

fn observation(time: f64, published: f64, publication: PublicationContext) -> SampleObservation {
    SampleObservation {
        requested_time: time,
        published_time: published,
        publication,
    }
}

fn run(
    options: ExportFrameOptions,
    source_end: f64,
) -> Result<(Vec<ExportFrame>, Vec<f64>, ExportFrameSummary), ExportFramePolicyError> {
    let mut policy = ExportFramePolicy::new(options)?;
    let context = publication();
    let mut frames = Vec::new();
    let mut requests = Vec::new();
    for _ in 0..10_000 {
        match policy.status()? {
            ExportFramePolicyStatus::NeedsSample(time) => {
                requests.push(time);
                policy.observe(
                    observation(time, time.min(source_end), context),
                    time >= source_end,
                )?;
            }
            ExportFramePolicyStatus::SampleReady(sample) => {
                for _ in 0..7 {
                    assert_eq!(
                        policy.status()?,
                        ExportFramePolicyStatus::SampleReady(sample)
                    );
                }
                if let Some(frame) = sample.frame {
                    frames.push(frame);
                }
                policy.acknowledge_sample(sample)?;
            }
            ExportFramePolicyStatus::Complete(summary) => {
                assert_eq!(frames.len() as u64, summary.frames);
                assert_eq!(policy.status()?, ExportFramePolicyStatus::Complete(summary));
                return Ok((frames, requests, summary));
            }
        }
    }
    panic!("policy test exceeded its bounded request count");
}

#[test]
fn source_end_and_frozen_hold_use_one_global_grid() {
    let (full, requests, summary) = run(options(), 0.312).unwrap();
    assert_eq!(full.len(), 10);
    assert_eq!(summary.source_end, Some(0.312));
    assert_eq!(summary.reason, ExportEndReason::SourceEnd);
    assert_eq!(summary.scheduled_duration, 10.0 / 30.0);
    assert!(full.iter().all(|frame| !frame.held));
    let (held, hold_requests, held_summary) = run(
        ExportFrameOptions {
            final_hold_seconds: 0.1,
            ..options()
        },
        0.312,
    )
    .unwrap();
    assert_eq!(held.len(), 13);
    assert_eq!(&held[..10], &full);
    assert_eq!(&hold_requests[..requests.len()], &requests);
    assert!(held[10..].iter().all(|frame| frame.held));
    assert_eq!(held_summary.end_time, 0.312 + 0.1);
}

#[test]
fn cropped_range_replays_prefix_and_rebases_only_video_pts() {
    let (full, requests, _) = run(options(), 0.312).unwrap();
    let (crop, crop_requests, summary) = run(
        ExportFrameOptions {
            start_frame: 4,
            ..options()
        },
        0.312,
    )
    .unwrap();
    assert_eq!(requests, crop_requests);
    assert_eq!(crop.len(), 6);
    assert_eq!(summary.start_time, 4.0 / 30.0);
    for (index, (actual, expected)) in crop.iter().zip(&full[4..]).enumerate() {
        assert_eq!(actual.source_sample, expected.source_sample);
        assert_eq!(actual.pts, index as u64);
    }
}

#[test]
fn off_grid_stop_is_drained_at_the_cutoff_not_the_next_frame() {
    let (frames, requests, summary) = run(
        ExportFrameOptions {
            stop: ExportStop::EndTime(0.115),
            ..options()
        },
        2.0,
    )
    .unwrap();
    assert_eq!(frames.len(), 4);
    assert_eq!(requests.last(), Some(&0.115));
    assert!(requests.iter().all(|&time| time <= 0.115));
    assert_eq!(summary.reason, ExportEndReason::RequestedStop);
    assert_eq!(summary.source_end, None);
}

#[test]
fn fractional_rate_and_frame_count_keep_exact_coordinates() {
    for (p, q) in [(30, 1), (30_000, 1_001), (60_000, 1_001)] {
        let rate = FrameRate::new(p, q).unwrap();
        let (frames, _, summary) = run(
            ExportFrameOptions {
                frame_rate: rate,
                start_frame: 3,
                stop: ExportStop::FrameCount(17),
                ..options()
            },
            100.0,
        )
        .unwrap();
        let grid = FrameGrid::new(rate, 0.0).unwrap();
        assert_eq!(frames.len(), 17);
        assert_eq!(summary.end_time, grid.sample(20).unwrap().authored_time());
        for (index, frame) in frames.iter().enumerate() {
            assert_eq!(frame.pts, index as u64);
            assert_eq!(frame.source_sample, grid.sample(index as u64 + 3).unwrap());
        }
    }
}

#[test]
fn cap_completion_and_empty_interval_do_not_invent_output() {
    let config = ExportFrameOptions {
        max_frames: 30,
        ..options()
    };
    assert_eq!(run(config, 1.0).unwrap().0.len(), 30);
    assert!(matches!(
        run(config, 1.1),
        Err(ExportFramePolicyError::FrameLimit)
    ));
    assert!(matches!(
        run(options(), 0.0),
        Err(ExportFramePolicyError::EmptyInterval)
    ));
    let (frames, _, _) = run(
        ExportFrameOptions {
            final_hold_seconds: 0.1,
            ..options()
        },
        0.0,
    )
    .unwrap();
    assert_eq!(frames.len(), 3);
    assert!(frames.iter().all(|frame| frame.held));
}

#[test]
fn backpressure_and_wrong_acknowledgement_preserve_the_pending_sample() {
    let mut policy = ExportFramePolicy::new(options()).unwrap();
    let offered = policy
        .observe(observation(0.0, 0.0, publication()), false)
        .unwrap();
    let mut wrong = offered;
    wrong.observation.requested_time = 1.0;
    assert_eq!(
        policy.acknowledge_sample(wrong),
        Err(ExportFramePolicyError::WrongSample)
    );
    assert_eq!(
        policy.observe(offered.observation, false),
        Err(ExportFramePolicyError::WrongSample)
    );
    assert_eq!(
        policy.status(),
        Ok(ExportFramePolicyStatus::SampleReady(offered))
    );
    policy.acknowledge_sample(offered).unwrap();
    assert_eq!(
        policy.status(),
        Ok(ExportFramePolicyStatus::NeedsSample(1.0 / 30.0))
    );
    assert_eq!(
        policy.acknowledge_sample(offered),
        Err(ExportFramePolicyError::WrongSample)
    );
}

#[test]
fn stale_shifted_nonfinite_and_future_observations_poison_the_policy() {
    for (request, published, finished) in [
        (1.0, 1.0, false),
        (f64::NAN, 0.0, false),
        (0.0, f64::NAN, true),
        (0.0, f64::INFINITY, true),
        (0.0, -1.0, true),
        (0.0, 0.1, true),
    ] {
        let mut policy = ExportFramePolicy::new(options()).unwrap();
        assert_eq!(
            policy.observe(observation(request, published, publication()), finished),
            Err(ExportFramePolicyError::InvalidObservation)
        );
        assert_eq!(policy.status(), Err(ExportFramePolicyError::Inactive));
    }
    let mut policy = ExportFramePolicy::new(options()).unwrap();
    let first = policy
        .observe(observation(0.0, 0.0, publication()), false)
        .unwrap();
    policy.acknowledge_sample(first).unwrap();
    assert_eq!(
        policy.observe(observation(1.0 / 30.0, 0.0, publication()), false),
        Err(ExportFramePolicyError::InvalidObservation)
    );
}

#[test]
fn terminal_hold_rejects_resumed_source_time_or_changed_publication() {
    let mut scene = Scene::new();
    let marker = scene.square(0.5).unwrap();
    scene.add(&marker).unwrap();
    let mut session = scene.execution_session().unwrap();
    let initial = session.publication_context();
    scene
        .live(&mut session)
        .set_translation(&marker, 1.0, 0.0)
        .unwrap();
    let changed = session.publication_context();
    assert_ne!(initial, changed);
    for (published, context, finished) in [
        (1.0 / 30.0, initial, true),
        (0.0, changed, true),
        (1.0 / 30.0, initial, false),
    ] {
        let mut policy = ExportFramePolicy::new(ExportFrameOptions {
            final_hold_seconds: 1.0,
            ..options()
        })
        .unwrap();
        let first = policy
            .observe(observation(0.0, 0.0, initial), true)
            .unwrap();
        policy.acknowledge_sample(first).unwrap();
        assert_eq!(
            policy.observe(observation(1.0 / 30.0, published, context), finished),
            Err(ExportFramePolicyError::InvalidObservation)
        );
        assert_eq!(policy.status(), Err(ExportFramePolicyError::Inactive));
    }
}

#[test]
fn cancellation_cannot_be_restarted_by_acknowledging_a_previous_offer() {
    let mut policy = ExportFramePolicy::new(options()).unwrap();
    let sample = policy
        .observe(observation(0.0, 0.0, publication()), false)
        .unwrap();
    policy.cancel();
    assert_eq!(policy.status(), Err(ExportFramePolicyError::Inactive));
    assert_eq!(
        policy.acknowledge_sample(sample),
        Err(ExportFramePolicyError::Inactive)
    );
    assert_eq!(
        policy.observe(sample.observation, false),
        Err(ExportFramePolicyError::Inactive)
    );
}
