//! Small device-gain automation values for transport and viewer transitions.
//!
//! The caller supplies observations from the existing audio device clock. This
//! module owns no clock or timer, and these ramps do not change authored fades.

pub(super) const ATTACK_SECONDS: f64 = 0.040;
pub(super) const RELEASE_SECONDS: f64 = 0.060;
pub(super) const VIEWER_RAMP_SECONDS: f64 = 0.020;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct GainRamp {
    from: f32,
    target: f32,
    start: f64,
    end: f64,
}

impl GainRamp {
    /// Hold an already admitted internal gain without scheduling an interval.
    pub(super) const fn constant(value: f32) -> Self {
        Self {
            from: value,
            target: value,
            start: 0.0,
            end: 0.0,
        }
    }

    pub(super) fn linear(
        from: f32,
        to: f32,
        start: f64,
        duration: f64,
    ) -> Result<Self, &'static str> {
        if !from.is_finite()
            || !to.is_finite()
            || !(0.0..=1.0).contains(&from)
            || !(0.0..=1.0).contains(&to)
        {
            return Err("device transition gains must be finite and between zero and one");
        }
        if !start.is_finite() || start < 0.0 {
            return Err("device transition start must be finite and nonnegative");
        }
        if !duration.is_finite() || duration <= 0.0 {
            return Err("device transition duration must be finite and positive");
        }
        let end = start + duration;
        if !end.is_finite() || end <= start {
            return Err("device transition end must be representable and later than its start");
        }
        Ok(Self {
            from,
            target: to,
            start,
            end,
        })
    }

    /// Observe an admitted device time, holding the endpoints outside the ramp.
    pub(super) fn value_at(self, time: f64) -> f32 {
        if self.end == self.start || time >= self.end {
            return self.target;
        }
        if time <= self.start {
            return self.from;
        }
        let fraction = (time - self.start) / (self.end - self.start);
        let from = f64::from(self.from);
        (from + (f64::from(self.target) - from) * fraction) as f32
    }

    /// Start a new ramp at the existing ramp's current value, without a jump.
    pub(super) fn retarget(self, to: f32, now: f64, duration: f64) -> Result<Self, &'static str> {
        Self::linear(self.value_at(now), to, now, duration)
    }

    pub(super) const fn from(self) -> f32 {
        self.from
    }

    pub(super) const fn target(self) -> f32 {
        self.target
    }

    pub(super) const fn start(self) -> f64 {
        self.start
    }

    pub(super) const fn end(self) -> f64 {
        self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= f32::EPSILON,
            "{actual} != {expected}"
        );
    }

    #[test]
    fn delayed_attack_holds_zero_then_reaches_exact_endpoint() {
        let attack = GainRamp::linear(0.0, 1.0, 3.0, ATTACK_SECONDS).unwrap();
        assert_eq!(attack.from(), 0.0);
        assert_eq!(attack.target(), 1.0);
        assert_eq!(attack.start(), 3.0);
        assert_eq!(attack.end(), 3.0 + ATTACK_SECONDS);
        assert_eq!(attack.value_at(f64::NEG_INFINITY), 0.0);
        assert_eq!(attack.value_at(0.0), 0.0);
        assert_eq!(attack.value_at(attack.start()), 0.0);
        close(attack.value_at(3.0 + ATTACK_SECONDS / 2.0), 0.5);
        assert_eq!(attack.value_at(attack.end()), 1.0);
        assert_eq!(attack.value_at(f64::INFINITY), 1.0);
    }

    #[test]
    fn release_and_constant_have_exact_zero_and_held_boundaries() {
        let release = GainRamp::linear(0.75, 0.0, 1.0, RELEASE_SECONDS).unwrap();
        assert_eq!(release.value_at(0.0), 0.75);
        assert_eq!(release.value_at(release.start()), 0.75);
        close(release.value_at(1.0 + RELEASE_SECONDS / 2.0), 0.375);
        assert_eq!(release.value_at(release.end()), 0.0);
        assert_eq!(release.value_at(5.0), 0.0);

        let held = GainRamp::constant(0.625);
        assert_eq!(held.start(), held.end());
        for time in [f64::NEG_INFINITY, -1.0, 0.0, 1.0, f64::INFINITY] {
            assert_eq!(held.value_at(time), 0.625);
        }
    }

    #[test]
    fn reversing_mid_attack_and_mid_release_preserves_current_gain() {
        let attack = GainRamp::linear(0.0, 1.0, 0.0, ATTACK_SECONDS).unwrap();
        let stop = ATTACK_SECONDS / 4.0;
        let release = attack.retarget(0.0, stop, RELEASE_SECONDS).unwrap();
        assert_eq!(release.from(), attack.value_at(stop));
        assert_eq!(release.value_at(stop), attack.value_at(stop));
        close(release.from(), 0.25);

        let resume = stop + RELEASE_SECONDS / 2.0;
        let resumed = release.retarget(1.0, resume, ATTACK_SECONDS).unwrap();
        assert_eq!(resumed.from(), release.value_at(resume));
        assert_eq!(resumed.value_at(resume), release.value_at(resume));
        close(resumed.from(), 0.125);
        close(resumed.value_at(resume + ATTACK_SECONDS / 2.0), 0.5625);
        assert_eq!(resumed.value_at(resumed.end()), 1.0);
    }

    #[test]
    fn retargeting_before_start_and_after_completion_uses_held_value() {
        let delayed = GainRamp::linear(0.25, 0.75, 2.0, ATTACK_SECONDS).unwrap();
        let before = delayed.retarget(0.5, 1.0, VIEWER_RAMP_SECONDS).unwrap();
        assert_eq!(before.from(), 0.25);
        assert_eq!(before.value_at(before.end()), 0.5);

        let after = delayed.retarget(0.0, 3.0, VIEWER_RAMP_SECONDS).unwrap();
        assert_eq!(after.from(), 0.75);
        assert_eq!(after.value_at(after.end()), 0.0);
        let from_constant = GainRamp::constant(0.5)
            .retarget(1.0, 1.0, VIEWER_RAMP_SECONDS)
            .unwrap();
        assert_eq!(from_constant.from(), 0.5);
    }

    #[test]
    fn rapid_retargets_remain_continuous_and_final_release_reaches_zero() {
        let mut ramp = GainRamp::constant(0.0);
        let targets = [1.0, 0.0, 0.75, 0.25];
        for index in 0..100 {
            let now = f64::from(index) * 0.001;
            let observed = ramp.value_at(now);
            assert!((0.0..=1.0).contains(&observed));
            ramp = ramp
                .retarget(
                    targets[index as usize % targets.len()],
                    now,
                    VIEWER_RAMP_SECONDS,
                )
                .unwrap();
            assert_eq!(ramp.value_at(now), observed);
        }
        let now = 0.100;
        let observed = ramp.value_at(now);
        ramp = ramp.retarget(0.0, now, RELEASE_SECONDS).unwrap();
        assert_eq!(ramp.from(), observed);
        assert_eq!(ramp.value_at(now), observed);
        assert_eq!(ramp.value_at(ramp.end()), 0.0);
        assert_eq!(ramp.value_at(1.0), 0.0);
    }

    #[test]
    fn invalid_gains_and_times_reject_before_an_automation_value_is_created() {
        for gain in [f32::NAN, f32::NEG_INFINITY, f32::INFINITY, -0.01, 1.01] {
            assert!(GainRamp::linear(gain, 0.5, 0.0, ATTACK_SECONDS).is_err());
            assert!(GainRamp::linear(0.5, gain, 0.0, ATTACK_SECONDS).is_err());
        }
        for start in [f64::NAN, f64::NEG_INFINITY, f64::INFINITY, -0.01] {
            assert!(GainRamp::linear(0.0, 1.0, start, ATTACK_SECONDS).is_err());
        }
        for duration in [f64::NAN, f64::NEG_INFINITY, f64::INFINITY, -1.0, 0.0] {
            assert!(GainRamp::linear(0.0, 1.0, 0.0, duration).is_err());
        }
    }

    #[test]
    fn overflow_and_collapsed_intervals_reject_but_small_representable_ramp_works() {
        assert!(GainRamp::linear(0.0, 1.0, f64::MAX, f64::MAX).is_err());
        assert!(GainRamp::linear(0.0, 1.0, 1.0e20, ATTACK_SECONDS).is_err());
        assert!(GainRamp::linear(0.0, 1.0, 1.0, f64::MIN_POSITIVE).is_err());
        let tiny = GainRamp::linear(0.0, 1.0, 0.0, f64::MIN_POSITIVE).unwrap();
        assert_eq!(tiny.value_at(0.0), 0.0);
        close(tiny.value_at(f64::MIN_POSITIVE / 2.0), 0.5);
        assert_eq!(tiny.value_at(tiny.end()), 1.0);
    }

    #[test]
    fn invalid_retarget_leaves_the_original_ramp_available() {
        let original = GainRamp::linear(0.0, 1.0, 0.0, ATTACK_SECONDS).unwrap();
        let midpoint = ATTACK_SECONDS / 2.0;
        for (to, now, duration) in [
            (f32::NAN, midpoint, RELEASE_SECONDS),
            (0.0, f64::NAN, RELEASE_SECONDS),
            (0.0, -1.0, RELEASE_SECONDS),
            (0.0, midpoint, 0.0),
            (0.0, f64::MAX, f64::MAX),
        ] {
            assert!(original.retarget(to, now, duration).is_err());
            close(original.value_at(midpoint), 0.5);
            assert_eq!(original.value_at(original.end()), 1.0);
        }
    }
}
