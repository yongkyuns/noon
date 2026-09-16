"""Closed-form checks for the teaching model's measurement-free propagation.

This is analysis of the existing 3-state KF, not a second estimator. Acceleration
noise is independent per held sample. It is not a continuous-time noise density.
"""
from math import isfinite
from model import multiply, transpose


def coast_covariance(covariance, dt, steps, sample_sigma):
    """Return (inherited covariance, new-noise covariance) after integer steps.

For state [p,v,b], Phi(T) is exact when physical bias is constant. A noise
sample i intervals before the end contributes [(i+1/2)dt², dt, 0]. Summing
these outer products gives Q below, independently of the stepwise predictor.
"""
    if isinstance(steps, bool) or not isinstance(steps, int) or steps < 0:
        raise ValueError('steps must be a nonnegative integer')
    if not isfinite(dt) or dt <= 0 or not isfinite(sample_sigma) or sample_sigma < 0:
        raise ValueError('Invalid sample interval or noise standard deviation')
    elapsed = steps * dt
    phi = ((1., elapsed, -.5 * elapsed**2), (0., 1., -elapsed), (0., 0., 1.))
    inherited = multiply(multiply(phi, covariance), transpose(phi))
    variance = sample_sigma**2
    q_pp = variance * dt**4 * steps * (4 * steps**2 - 1) / 12
    q_pv = variance * dt**3 * steps**2 / 2
    q_vv = variance * dt**2 * steps
    process = [[q_pp, q_pv, 0.], [q_pv, q_vv, 0.], [0., 0., 0.]]
    return inherited, process


def position_variance_terms(covariance, elapsed, noise_variance):
    """Signed terms in h P hᵀ + Qpp, h=[1,T,-T²/2]; all in m².

The cross terms are not independent uncertainty sources and may be negative.
"""
    p, t = covariance, elapsed
    return {
        'position': p[0][0],
        'velocity': t**2 * p[1][1],
        'bias': .25 * t**4 * p[2][2],
        'position_velocity': 2 * t * p[0][1],
        'position_bias': -t**2 * p[0][2],
        'velocity_bias': -t**3 * p[1][2],
        'new_noise': noise_variance,
    }


def outage_prediction(samples, config):
    """Analyze the last accepted pre-outage fix to the next observed epoch.

The start is the last accepted fix, not the start of the outage label. The end
is a retained pre-update snapshot, not the posterior after reacquisition.
"""
    starts = [s for s in samples if s.time < config.outage_start and s.accepted is True]
    ends = [s for s in samples if s.time >= config.outage_end and s.prior_state is not None]
    if not starts or not ends:
        raise ValueError('Need accepted pre-outage and observed post-outage endpoints')
    start, end = starts[-1], ends[0]
    if any(s.accepted is True for s in samples if start.time < s.time < end.time):
        raise ValueError('Prediction span must contain no accepted updates')
    steps = round((end.time - start.time) * config.imu_hz)
    inherited, process = coast_covariance(start.covariance, 1 / config.imu_hz,
                                          steps, config.acceleration_sigma)
    terms = position_variance_terms(start.covariance, end.time - start.time, process[0][0])
    return start, end, inherited, process, terms
