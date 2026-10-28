//! #2300 isolated contract prototype. Not linked into Noon or an audio device.
//!
//! An already-correlated command samples in the worker after Window send and
//! before Window receipt. The Window supplies three readings of the SAME clock.
//! No worker epoch, renderer timestamp, retry, wait, or new playhead is used.
//! Identity checks here are necessary, not sufficient: the actual host must use
//! its existing pending-request owner and generations, never trust a wire claim.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    pub owner: u64,
    pub lifecycle: u64,
    pub session: u64,
    pub request: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    /// A host-local opaque identity, not performance.timeOrigin's numeric value.
    pub clock: u64,
    pub ms: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interval {
    lower_ms: f64,
    upper_ms: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Estimate {
    pub midpoint_ms: f64,
    pub worst_case_radius_ms: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReadErrorEvidence {
    /// The browser standard alone supplies no finite accuracy certificate.
    Unknown,
    /// Externally justified absolute read-error bounds, NOT observed resolution.
    /// This is a mathematical assumption, not a browser capability claim.
    Certified { sent_ms: f64, received_ms: f64, applied_ms: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    WrongOwnerOrRequest,
    WrongClock,
    NonFiniteReading,
    NegativeReading,
    ReversedReading,
    InvalidErrorBound,
    UnknownClockError,
    ArithmeticOverflow,
    AlreadyConsumed,
    InvalidAuthoredTime,
    InvalidBudget,
    UncertaintyOverBudget,
}

/// One extra field in an EXISTING pending request, not a second queue/registry.
#[derive(Debug)]
pub struct IssuedObservation {
    identity: Identity,
    sent: Reading,
    consumed: bool,
}

#[derive(Debug)]
pub struct CorrelatedObservation {
    sent: Reading,
    received: Reading,
    identity: Identity,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Evidence {
    /// Bounds in the Window's reported clock coordinate, not physical seconds.
    pub observable_age: Interval,
    /// Physical transport-age bounds ONLY conditional on supplied read errors.
    pub conditional_physical_age: Option<Interval>,
}

fn finite_nonnegative(value: f64) -> Result<(), Error> {
    if !value.is_finite() { return Err(Error::NonFiniteReading); }
    if value < 0.0 { return Err(Error::NegativeReading); }
    Ok(())
}

fn finite(value: f64) -> Result<f64, Error> {
    if value.is_finite() { Ok(value) } else { Err(Error::ArithmeticOverflow) }
}

// Directed rounding encloses arithmetic error. This is not an acceptance epsilon
// and never changes the original observations. Exact equality stays exact zero.
fn sub_down(a: f64, b: f64) -> Result<f64, Error> {
    if a == b { return Ok(0.0); }
    finite(finite(a - b)?.next_down())
}
fn sub_up(a: f64, b: f64) -> Result<f64, Error> {
    if a == b { return Ok(0.0); }
    finite(finite(a - b)?.next_up())
}
fn add_up(a: f64, b: f64) -> Result<f64, Error> {
    if a == 0.0 { return finite(b); }
    if b == 0.0 { return finite(a); }
    finite(finite(a + b)?.next_up())
}
fn nonnegative_lower(value: f64) -> f64 {
    // Intersection of a proved causal interval with age >= 0, NOT correction
    // of a reversed timestamp. Reversed host readings are rejected beforehand.
    if value <= 0.0 { 0.0 } else { value }
}

impl IssuedObservation {
    pub fn new(identity: Identity, sent: Reading) -> Result<Self, Error> {
        finite_nonnegative(sent.ms)?;
        Ok(Self { identity, sent, consumed: false })
    }

    // `current.request` is this correlated pending ID, NOT the newest issued ID.
    // Unrelated input or viewer gain must not invalidate a committed audio control.
    pub fn receive(&mut self, current: Identity, response: Identity, received: Reading)
        -> Result<CorrelatedObservation, Error>
    {
        if self.consumed { return Err(Error::AlreadyConsumed); }
        // An unrelated response does not consume the legitimate pending request.
        if response != self.identity { return Err(Error::WrongOwnerOrRequest); }
        // A matched completion is one-shot, including errors or retired ownership.
        self.consumed = true;
        if current != self.identity { return Err(Error::WrongOwnerOrRequest); }
        if received.clock != self.sent.clock { return Err(Error::WrongClock); }
        finite_nonnegative(received.ms)?;
        if received.ms < self.sent.ms { return Err(Error::ReversedReading); }
        Ok(CorrelatedObservation { sent: self.sent, received, identity: self.identity })
    }
}

impl CorrelatedObservation {
    /// Consumes the receipt; an apply cannot be repeated with the same Rust value.
    /// The host must recheck its actual worker and audio-control generations too.
    pub fn apply(self, current: Identity, applied: Reading, error: ReadErrorEvidence)
        -> Result<Evidence, Error>
    {
        if current != self.identity { return Err(Error::WrongOwnerOrRequest); }
        if applied.clock != self.sent.clock { return Err(Error::WrongClock); }
        finite_nonnegative(applied.ms)?;
        if applied.ms < self.received.ms { return Err(Error::ReversedReading); }
        let observable_age = Interval {
            lower_ms: nonnegative_lower(sub_down(applied.ms, self.received.ms)?),
            upper_ms: sub_up(applied.ms, self.sent.ms)?,
        };
        let conditional_physical_age = match error {
            ReadErrorEvidence::Unknown => None,
            ReadErrorEvidence::Certified { sent_ms, received_ms, applied_ms } => {
                for bound in [sent_ms, received_ms, applied_ms] {
                    if !bound.is_finite() || bound < 0.0 { return Err(Error::InvalidErrorBound); }
                }
                let low = sub_down(sub_down(observable_age.lower_ms, applied_ms)?, received_ms)?;
                let high = add_up(add_up(observable_age.upper_ms, applied_ms)?, sent_ms)?;
                Some(Interval { lower_ms: nonnegative_lower(low), upper_ms: high })
            }
        };
        Ok(Evidence { observable_age, conditional_physical_age })
    }
}

impl Interval {
    pub fn lower_ms(self) -> f64 { self.lower_ms }
    pub fn upper_ms(self) -> f64 { self.upper_ms }
    /// Minimax for the supplied interval, NOT a claim of symmetric transport.
    pub fn midpoint(self) -> Result<Estimate, Error> {
        let midpoint = finite(self.lower_ms + (self.upper_ms - self.lower_ms) / 2.0)?;
        let radius = sub_up(midpoint, self.lower_ms)?.max(sub_up(self.upper_ms, midpoint)?);
        Ok(Estimate { midpoint_ms: midpoint, worst_case_radius_ms: radius })
    }
}

impl Evidence {
    /// Deliberately unavailable without an externally justified clock-error model.
    /// There is no built-in/browser-default budget. Test budgets are synthetic.
    /// This qualifies only transport age, not worker clock rate, audio or display.
    pub fn project_with_certified_budget(self, authored_seconds: f64, budget_ms: f64)
        -> Result<(f64, Estimate), Error>
    {
        if !authored_seconds.is_finite() || authored_seconds < 0.0 {
            return Err(Error::InvalidAuthoredTime);
        }
        if !budget_ms.is_finite() || budget_ms < 0.0 { return Err(Error::InvalidBudget); }
        let interval = self.conditional_physical_age.ok_or(Error::UnknownClockError)?;
        let estimate = interval.midpoint()?;
        if estimate.worst_case_radius_ms > budget_ms { return Err(Error::UncertaintyOverBudget); }
        let projected = finite(authored_seconds + estimate.midpoint_ms / 1000.0)?;
        Ok((projected, estimate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: Identity = Identity { owner: 4, lifecycle: 2, session: 7, request: 11 };
    fn read(ms: f64) -> Reading { Reading { clock: 1, ms } }
    fn observe(s: f64, r: f64, a: f64, error: ReadErrorEvidence) -> Result<Evidence, Error> {
        IssuedObservation::new(ID, read(s))?.receive(ID, ID, read(r))?.apply(ID, read(a), error)
    }
    fn exact() -> ReadErrorEvidence {
        ReadErrorEvidence::Certified { sent_ms: 0.0, received_ms: 0.0, applied_ms: 0.0 }
    }
    #[test]
    fn observed_bounds_include_post_receipt_delay() {
        let e = observe(100.0, 106.0, 110.0, ReadErrorEvidence::Unknown).unwrap();
        assert!(e.observable_age.lower_ms <= 4.0);
        assert!(e.observable_age.lower_ms >= 4.0_f64.next_down());
        assert!(e.observable_age.upper_ms >= 10.0);
        assert!(e.observable_age.upper_ms <= 10.0_f64.next_up());
        assert_eq!(e.conditional_physical_age, None);
    }
    #[test]
    fn unknown_error_is_not_an_exact_physical_bound() {
        let e = observe(100.0, 100.0, 100.0, ReadErrorEvidence::Unknown).unwrap();
        assert_eq!(e.observable_age, Interval { lower_ms: 0.0, upper_ms: 0.0 });
        assert_eq!(e.project_with_certified_budget(2.1, 1.0), Err(Error::UnknownClockError));
    }
    #[test]
    fn equal_coarsened_readings_may_hide_positive_elapsed_time() {
        // Same 1 ms bucket: sent at 10.1, sample 10.2, receipt 10.7, apply 10.9.
        let e = observe(10.0, 10.0, 10.0, ReadErrorEvidence::Certified {
            sent_ms: 1.0, received_ms: 1.0, applied_ms: 1.0,
        }).unwrap();
        let interval = e.conditional_physical_age.unwrap();
        assert!(interval.lower_ms <= 0.7 && interval.upper_ms >= 0.7);
        assert!(interval.upper_ms >= 2.0);
    }
    #[test]
    fn midpoint_is_minimax_not_a_symmetric_network_assertion() {
        let i = Interval { lower_ms: 4.0, upper_ms: 10.0 };
        let m = i.midpoint().unwrap();
        assert_eq!(m.midpoint_ms, 7.0);
        assert!(m.worst_case_radius_ms >= 3.0);
        for step in 0..=60 {
            let age = 4.0 + f64::from(step) / 10.0;
            assert!((m.midpoint_ms - age).abs() <= m.worst_case_radius_ms);
        }
    }
    #[test]
    fn all_causal_sample_positions_fit_without_any_worker_epoch() {
        let mut count = 0_u64;
        for sent in 0..=8 {
            for received in sent..=20 {
                for applied in received..=24 {
                    let e = observe(f64::from(sent), f64::from(received), f64::from(applied), exact()).unwrap();
                    let i = e.conditional_physical_age.unwrap();
                    for sample in sent..=received {
                        let age = f64::from(applied - sample);
                        assert!(i.lower_ms <= age && age <= i.upper_ms);
                        count += 1;
                    }
                }
            }
        }
        println!("causal_sample_cases={count}");
        assert!(count > 10_000);
    }
    #[test]
    fn exhaustive_floor_clock_models_preserve_conditional_enclosure() {
        let mut count = 0_u64;
        for quantum in [1_i32, 2, 4, 8] {
            for sent in 0..=8 {
                for received in sent..=16 {
                    for applied in received..=20 {
                        let floor = |n| f64::from((n / quantum) * quantum);
                        let q = f64::from(quantum);
                        let e = observe(floor(sent), floor(received), floor(applied),
                            ReadErrorEvidence::Certified { sent_ms: q, received_ms: q, applied_ms: q }).unwrap();
                        let i = e.conditional_physical_age.unwrap();
                        for sample in sent..=received {
                            let age = f64::from(applied - sample);
                            assert!(i.lower_ms <= age && age <= i.upper_ms);
                            count += 1;
                        }
                    }
                }
            }
        }
        println!("coarsened_sample_cases={count}");
        assert!(count > 30_000);
    }
    #[test]
    fn exact_historical_reversal_is_not_used_as_ownership_evidence() {
        let sampled = 1_793_152_307_172.3_f64;
        let received = 1_793_152_307_172.2998_f64;
        assert_eq!(sampled.to_bits(), received.to_bits() + 1);
        assert_eq!(received - sampled, -0.000244140625);
        // Neither value is fed into the new model. No correction is applied.
        let e = observe(10.0, 11.0, 12.0, ReadErrorEvidence::Unknown).unwrap();
        assert!(e.observable_age.lower_ms <= 1.0 && e.observable_age.upper_ms >= 2.0);
    }
    #[test]
    fn wrong_response_cannot_steal_a_pending_slot() {
        let mut p = IssuedObservation::new(ID, read(10.0)).unwrap();
        let mut wrong = ID; wrong.request += 1;
        assert!(matches!(p.receive(ID, wrong, read(11.0)), Err(Error::WrongOwnerOrRequest)));
        assert!(p.receive(ID, ID, read(11.0)).is_ok());
    }
    #[test]
    fn matched_receipt_is_one_shot() {
        let mut p = IssuedObservation::new(ID, read(10.0)).unwrap();
        let _receipt = p.receive(ID, ID, read(11.0)).unwrap();
        assert!(matches!(p.receive(ID, ID, read(11.0)), Err(Error::AlreadyConsumed)));
    }
    #[test]
    fn stale_owner_lifecycle_session_and_request_each_fail() {
        let mut variants = [ID; 4];
        variants[0].owner += 1; variants[1].lifecycle += 1;
        variants[2].session += 1; variants[3].request += 1;
        for current in variants {
            let mut p = IssuedObservation::new(ID, read(10.0)).unwrap();
            assert!(matches!(p.receive(current, ID, read(11.0)), Err(Error::WrongOwnerOrRequest)));
            assert!(matches!(p.receive(ID, ID, read(11.0)), Err(Error::AlreadyConsumed)));
            let r = IssuedObservation::new(ID, read(10.0)).unwrap().receive(ID, ID, read(11.0)).unwrap();
            assert_eq!(r.apply(current, read(12.0), exact()), Err(Error::WrongOwnerOrRequest));
        }
    }
    #[test]
    fn mixed_clocks_fail_even_when_numeric_readings_are_equal() {
        let other = Reading { clock: 2, ms: 10.0 };
        let mut p = IssuedObservation::new(ID, read(10.0)).unwrap();
        assert!(matches!(p.receive(ID, ID, other), Err(Error::WrongClock)));
        let r = IssuedObservation::new(ID, read(10.0)).unwrap().receive(ID, ID, read(10.0)).unwrap();
        assert_eq!(r.apply(ID, other, exact()), Err(Error::WrongClock));
    }
    #[test]
    fn invalid_readings_fail_at_every_position() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
            assert!(observe(value, 2.0, 3.0, exact()).is_err());
            assert!(observe(1.0, value, 3.0, exact()).is_err());
            assert!(observe(1.0, 2.0, value, exact()).is_err());
        }
    }
    #[test]
    fn reversed_same_clock_readings_fail_without_tolerance() {
        assert_eq!(observe(2.0, 2.0_f64.next_down(), 3.0, exact()), Err(Error::ReversedReading));
        assert_eq!(observe(1.0, 2.0, 2.0_f64.next_down(), exact()), Err(Error::ReversedReading));
    }
    #[test]
    fn unknown_or_invalid_certificates_are_not_inferred() {
        for value in [f64::NAN, f64::INFINITY, -1.0] {
            for bounds in [[value,0.0,0.0], [0.0,value,0.0], [0.0,0.0,value]] {
                assert_eq!(observe(1.0, 2.0, 3.0, ReadErrorEvidence::Certified {
                    sent_ms: bounds[0], received_ms: bounds[1], applied_ms: bounds[2],
                }), Err(Error::InvalidErrorBound));
            }
        }
    }
    #[test]
    fn wide_interval_fails_explicit_budget_without_replay() {
        let e = observe(10.0, 210.0, 215.0, exact()).unwrap();
        assert_eq!(e.project_with_certified_budget(2.0, 1.0), Err(Error::UncertaintyOverBudget));
    }
    #[test]
    fn synthetic_certified_projection_keeps_authored_time() {
        let e = observe(100.0, 106.0, 110.0, exact()).unwrap();
        let (time, m) = e.project_with_certified_budget(2.0, 4.0).unwrap();
        assert_eq!(time, 2.0 + m.midpoint_ms / 1000.0);
        assert!(m.worst_case_radius_ms < 4.0);
    }
    #[test]
    fn malformed_authored_times_and_budgets_fail() {
        let e = observe(1.0, 2.0, 3.0, exact()).unwrap();
        for value in [f64::NAN, f64::INFINITY, -1.0] {
            assert_eq!(e.project_with_certified_budget(value, 10.0), Err(Error::InvalidAuthoredTime));
            assert_eq!(e.project_with_certified_budget(1.0, value), Err(Error::InvalidBudget));
        }
    }
    #[test]
    fn interval_overflow_fails_closed() {
        assert_eq!(observe(0.0, 0.0, f64::MAX, exact()), Err(Error::ArithmeticOverflow));
        assert_eq!(observe(1.0, 2.0, 3.0, ReadErrorEvidence::Certified {
            sent_ms: f64::MAX, received_ms: 0.0, applied_ms: f64::MAX,
        }), Err(Error::ArithmeticOverflow));
    }
}
