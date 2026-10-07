use super::*;
use crate::{
    resolve_composition_schedule, CompositionTimeMap, CompositionTimeMapStep, RateFunction,
};

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
}

#[test]
fn definition_defaults_and_empty_patch_are_different() {
    let glow = Glow::default();
    assert_eq!(glow.color(), Color::WHITE);
    assert_eq!(glow.radius(), GlowRadius::Scene(0.15));
    assert_eq!(glow.intensity(), 0.35);
    assert_eq!(glow.source(), GlowSource::Painted);
    assert!(!glow.is_neutral());
    let empty = GlowUpdate::default();
    assert_eq!(empty.apply_to(glow).unwrap(), glow);
    for parameter in [
        GlowParameter::Color,
        GlowParameter::Radius,
        GlowParameter::Intensity,
        GlowParameter::Source,
    ] {
        assert!(!empty.writes(parameter));
        assert!(!empty.prepare(glow).unwrap().writes(parameter));
    }
}

#[test]
fn partial_updates_preserve_every_omitted_parameter() {
    let glow = Glow::new(
        GlowUpdate::default()
            .color(Color::rgba(0.1, 0.2, 0.3, 0.4))
            .radius(Pixels(12.0))
            .source(GlowSource::Silhouette),
    )
    .unwrap();
    let changed = GlowUpdate::default().intensity(1.5).apply_to(glow).unwrap();
    assert_eq!(changed.color(), glow.color());
    assert_eq!(changed.radius(), glow.radius());
    assert_eq!(changed.source(), glow.source());
    assert_eq!(changed.intensity(), 1.5);
    assert_eq!(glow.intensity(), 0.35);
}

#[test]
fn invalid_multi_parameter_request_returns_no_replacement() {
    let glow = Glow::default();
    let invalid = GlowUpdate::default().color(Color::RED).intensity(-1.0);
    assert_eq!(
        invalid.apply_to(glow),
        Err(GlowParameterError::InvalidIntensity)
    );
    assert_eq!(glow, Glow::default());
    assert!(invalid.prepare(glow).is_err());
}

#[test]
fn invalid_numeric_inputs_fail_in_shared_rust() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        assert_eq!(
            Glow::new(GlowUpdate::default().radius(value)),
            Err(GlowParameterError::InvalidRadius)
        );
        assert_eq!(
            Glow::new(GlowUpdate::default().radius(Pixels(value))),
            Err(GlowParameterError::InvalidRadius)
        );
        assert_eq!(
            Glow::new(GlowUpdate::default().intensity(value)),
            Err(GlowParameterError::InvalidIntensity)
        );
    }
    assert!(Glow::new(GlowUpdate::default().intensity(8.0)).is_ok());
    assert!(Glow::new(GlowUpdate::default().intensity(8.0001)).is_err());
}

#[test]
fn all_four_color_channels_are_checked() {
    for channel in 0..4 {
        for value in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            let mut rgba = [0.5; 4];
            rgba[channel] = value;
            let color = Color::rgba(rgba[0], rgba[1], rgba[2], rgba[3]);
            assert_eq!(
                Glow::new(GlowUpdate::default().color(color)),
                Err(GlowParameterError::InvalidColor)
            );
        }
    }
}

#[test]
fn neutral_is_not_attachment_removal() {
    for update in [
        GlowUpdate::default().intensity(0.0),
        GlowUpdate::default().radius(0.0),
        GlowUpdate::default().color(Color::rgba(1.0, 0.5, 0.0, 0.0)),
    ] {
        let neutral = Glow::new(update).unwrap();
        assert!(neutral.is_neutral());
        assert_eq!(GlowUpdate::default().apply_to(neutral).unwrap(), neutral);
    }
}

#[test]
fn scene_and_output_pixels_have_different_view_dependencies() {
    let scene = GlowRadius::Scene(0.15);
    close(scene.to_output_pixels(600, 6.0).unwrap(), 15.0);
    close(scene.to_output_pixels(1200, 6.0).unwrap(), 30.0);
    close(scene.to_output_pixels(600, 3.0).unwrap(), 30.0);
    for (height, view) in [(600, 6.0), (1200, 6.0), (600, 3.0)] {
        assert_eq!(
            GlowRadius::Pixels(12.0)
                .to_output_pixels(height, view)
                .unwrap(),
            12.0
        );
    }
}

#[test]
fn invalid_view_and_unrepresentable_projection_fail_closed() {
    for radius in [GlowRadius::Scene(0.15), GlowRadius::Pixels(12.0)] {
        assert_eq!(
            radius.to_output_pixels(0, 6.0),
            Err(GlowParameterError::InvalidView)
        );
        for view in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                radius.to_output_pixels(600, view),
                Err(GlowParameterError::InvalidView)
            );
        }
    }
    assert_eq!(
        GlowRadius::Scene(f64::MAX).to_output_pixels(600, 1.0),
        Err(GlowParameterError::InvalidRadius)
    );
}

#[test]
fn static_discrete_edits_are_allowed_but_interpolation_is_not() {
    let glow = Glow::new(GlowUpdate::default().intensity(0.0)).unwrap();
    for (update, parameter) in [
        (
            GlowUpdate::default().radius(Pixels(12.0)),
            GlowParameter::Radius,
        ),
        (
            GlowUpdate::default().source(GlowSource::Silhouette),
            GlowParameter::Source,
        ),
    ] {
        assert!(update.apply_to(glow).is_ok());
        assert_eq!(
            update.prepare(glow),
            Err(GlowParameterError::DiscreteTransition(parameter))
        );
    }
}

#[test]
fn literal_linear_vectors_and_exact_endpoints() {
    let prepared = GlowUpdate::default()
        .intensity(1.2)
        .prepare(Glow::default())
        .unwrap();
    for (alpha, expected) in [(0.0, 0.35), (0.5, 0.775), (1.0, 1.2)] {
        close(prepared.sample(alpha).unwrap().intensity.unwrap(), expected);
    }
    assert_eq!(prepared.sample(0.0).unwrap().intensity, Some(0.35));
    assert_eq!(prepared.sample(1.0).unwrap().intensity, Some(1.2));
}

#[test]
fn capture_is_explicit_not_taken_when_request_is_constructed() {
    let request = GlowUpdate::default().intensity(1.2);
    let edited = Glow::new(GlowUpdate::default().intensity(0.9)).unwrap();
    let prepared = request.prepare(edited).unwrap();
    assert_eq!(prepared.sample(0.0).unwrap().intensity, Some(0.9));
    close(prepared.sample(0.5).unwrap().intensity.unwrap(), 1.05);
}

#[test]
fn samples_are_partial_writes_not_stale_whole_appearance_snapshots() {
    let captured = Glow::default();
    let prepared = GlowUpdate::default()
        .intensity(1.2)
        .prepare(captured)
        .unwrap();
    let current = GlowUpdate::default()
        .radius(Pixels(40.0))
        .color(Color::RED)
        .source(GlowSource::Silhouette)
        .apply_to(captured)
        .unwrap();
    let sampled = prepared.sample(0.5).unwrap();
    assert_eq!(sampled.radius, None);
    assert_eq!(sampled.color, None);
    assert_eq!(sampled.source, None);
    let current = sampled.apply_to(current).unwrap();
    assert_eq!(current.radius(), GlowRadius::Pixels(40.0));
    assert_eq!(current.color(), Color::RED);
    assert_eq!(current.source(), GlowSource::Silhouette);
    close(current.intensity(), 0.775);
}

#[test]
fn explicitly_requested_equal_value_still_has_a_write_channel() {
    let update = GlowUpdate::default().intensity(0.35);
    let prepared = update.prepare(Glow::default()).unwrap();
    assert!(prepared.writes(GlowParameter::Intensity));
    assert!(!prepared.writes(GlowParameter::Radius));
    assert_eq!(prepared.sample(0.5).unwrap().intensity, Some(0.35));
}

#[test]
fn color_and_radius_interpolate_with_their_declared_units() {
    let captured = Glow::new(
        GlowUpdate::default()
            .radius(Pixels(4.0))
            .color(Color::rgba(0.0, 0.5, 1.0, 0.0)),
    )
    .unwrap();
    let prepared = GlowUpdate::default()
        .radius(Pixels(12.0))
        .color(Color::rgba(1.0, 0.0, 0.5, 1.0))
        .prepare(captured)
        .unwrap();
    let middle = prepared.sample(0.5).unwrap();
    assert_eq!(middle.radius, Some(GlowRadius::Pixels(8.0)));
    assert_eq!(middle.color, Some(Color::rgba(0.5, 0.25, 0.75, 0.5)));
    assert_eq!(middle.intensity, None);
}

#[test]
fn invalid_progress_is_never_silently_clamped_or_published() {
    let prepared = GlowUpdate::default().prepare(Glow::default()).unwrap();
    for alpha in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        assert_eq!(
            prepared.sample(alpha),
            Err(GlowParameterError::InvalidProgress)
        );
    }
}

#[test]
fn shared_there_and_back_matches_independent_literal_pulse_vectors() {
    let prepared = GlowUpdate::default()
        .intensity(2.0)
        .prepare(Glow::default())
        .unwrap();
    for (time, expected) in [
        (0.0, 0.35),
        (0.25, 1.175),
        (0.5, 2.0),
        (0.75, 1.175),
        (1.0, 0.35),
    ] {
        let alpha = RateFunction::ThereAndBack.evaluate_f64(time);
        close(prepared.sample(alpha).unwrap().intensity.unwrap(), expected);
    }
}

#[test]
fn existing_lag_and_duration_rescaling_feed_parameter_sampling() {
    let schedule = resolve_composition_schedule(&[2.0, 4.0], 0.5, Some(10.0)).unwrap();
    assert_eq!(schedule.intrinsic_run_time, 5.0);
    let child = schedule.intervals[1];
    assert_eq!(child.start_time, 2.0);
    assert_eq!(child.duration, 8.0);
    let map = CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
        child.start_time / schedule.run_time,
        child.duration / schedule.run_time,
        RateFunction::Linear,
    )]);
    map.validate().unwrap();
    assert!(!map.evaluate_f64(0.1).begun);
    let prepared = GlowUpdate::default()
        .intensity(1.2)
        .prepare(Glow::default())
        .unwrap();
    for (time, expected) in [(2.0, 0.35), (6.0, 0.775), (10.0, 1.2)] {
        let sample = map.evaluate_f64(time / schedule.run_time);
        assert!(sample.begun);
        close(prepared.sample(sample.alpha).unwrap().intensity.unwrap(), expected);
    }
}

#[test]
fn nested_shared_time_maps_and_nonmonotone_rate_need_no_effect_clock() {
    let map = CompositionTimeMap::from_steps(vec![
        CompositionTimeMapStep::new(0.25, 0.5, RateFunction::Linear),
        CompositionTimeMapStep::new(0.5, 0.5, RateFunction::Linear),
    ]);
    map.validate().unwrap();
    assert!(!map.evaluate_f64(0.49).begun);
    let prepared = GlowUpdate::default()
        .intensity(2.0)
        .prepare(Glow::default())
        .unwrap();
    for (root_alpha, expected) in [(0.5, 0.35), (0.625, 2.0), (0.75, 0.35)] {
        let sample = map.evaluate_f64(root_alpha);
        assert!(sample.begun);
        let alpha = RateFunction::ThereAndBack.evaluate_f64(sample.alpha);
        close(prepared.sample(alpha).unwrap().intensity.unwrap(), expected);
    }
}

#[test]
fn existing_succession_rescaling_keeps_exact_child_boundaries() {
    let schedule = resolve_composition_schedule(&[0.6, 0.4], 1.0, Some(2.0)).unwrap();
    assert_eq!(schedule.intervals[0].start_time, 0.0);
    close(schedule.intervals[0].duration, 1.2);
    close(schedule.intervals[1].start_time, 1.2);
    close(schedule.intervals[1].end_time(), 2.0);
}

#[test]
fn parameter_samples_do_not_depend_on_call_order() {
    let prepared = GlowUpdate::default()
        .intensity(2.0)
        .prepare(Glow::default())
        .unwrap();
    let at = |time| {
        prepared
            .sample(RateFunction::ThereAndBack.evaluate_f64(time))
            .unwrap()
    };
    let expected = at(0.3);
    for time in [1.0, 0.0, 0.5, 0.3, 0.9] {
        let _ = at(time);
    }
    assert_eq!(at(0.3), expected);
}
