"""Truth-known 1D teaching experiment. SI units; no Noon or third-party imports.

This is a position/velocity/physical-accelerometer-bias Kalman filter, NOT the
reference's 24-error-state ECEF filter. White acceleration noise is independent
per sample (sample standard deviation, not a continuous-time noise density).
"""
from dataclasses import dataclass
from math import isfinite, sqrt
from random import Random


@dataclass(frozen=True)
class Experiment:
    duration: float = 120.0
    imu_hz: int = 100
    gnss_hz: int = 1
    bias: float = 0.02
    acceleration_sigma: float = 0.04
    position_sigma: float = 3.0
    outage_start: float = 45.0
    outage_end: float = 75.0
    seed: int = 20260915
    initial_position_sigma: float = 3.0
    initial_velocity_sigma: float = 1.0
    initial_bias_sigma: float = 0.05
    outlier_time: float = 90.0
    outlier_metres: float = 0.0
    gate_sigma: float = 3.0
    gate_enabled: bool = True

    def __post_init__(self):
        if self.imu_hz <= 0 or self.gnss_hz <= 0 or self.imu_hz % self.gnss_hz:
            raise ValueError("IMU rate must be a positive multiple of GNSS rate")
        if not 0 <= self.outage_start <= self.outage_end <= self.duration:
            raise ValueError("Outage must lie within the experiment")
        if self.position_sigma <= 0 or self.acceleration_sigma < 0:
            raise ValueError("Invalid sensor standard deviation")
        if not all(isfinite(v) for v in self.__dict__.values()):
            raise ValueError("Experiment parameters must be finite")


@dataclass(frozen=True)
class Sample:
    time: float
    truth: float
    truth_velocity: float
    inertial: float
    position: float
    velocity: float
    bias: float
    covariance: tuple
    observation: float | None = None
    innovation: float | None = None
    innovation_variance: float | None = None
    accepted: bool | None = None
    prior_state: tuple | None = None
    prior_covariance: tuple | None = None

    @property
    def state(self):
        return (self.position, self.velocity, self.bias)

    @property
    def prior_position(self):
        return self.prior_state[0] if self.prior_state is not None else None

    @property
    def gain(self):
        """Candidate gain for this position observation, even if gated out."""
        if self.prior_covariance is None:
            return None
        return tuple(row[0] / self.innovation_variance for row in self.prior_covariance)

    @property
    def correction(self):
        """Actual injected correction; zero for a rejected observation."""
        if self.prior_state is None:
            return None
        return tuple(post - prior for post, prior in zip(self.state, self.prior_state))

    @property
    def error(self):
        return self.position - self.truth

    @property
    def inertial_error(self):
        return self.inertial - self.truth

    @property
    def sigma(self):
        return sqrt(max(0.0, self.covariance[0][0]))


def transpose(matrix):
    return [list(column) for column in zip(*matrix)]


def multiply(left, right):
    columns = transpose(right)
    return [[sum(a * b for a, b in zip(row, col)) for col in columns] for row in left]


def add(left, right):
    return [[a + b for a, b in zip(lrow, rrow)] for lrow, rrow in zip(left, right)]


def identity(size):
    return [[float(i == j) for j in range(size)] for i in range(size)]


def acceleration(time):
    """Piecewise-constant truth; all switching times are sensor-grid aligned."""
    if 5 <= time < 20:
        return 0.5
    if 35 <= time < 50:
        return -0.25
    if 80 <= time < 95:
        return 0.25
    if 100 <= time < 115:
        return -0.5
    return 0.0


def predict(state, covariance, measured_acceleration, dt, sample_sigma):
    """Exact constant-acceleration/constant-bias transition for one sample."""
    position, velocity, bias = state
    corrected = measured_acceleration - bias
    predicted = [position + velocity * dt + 0.5 * corrected * dt**2,
                 velocity + corrected * dt, bias]
    phi = [[1.0, dt, -0.5 * dt**2], [0.0, 1.0, -dt], [0.0, 0.0, 1.0]]
    input_map = [0.5 * dt**2, dt, 0.0]
    process_noise = [[sample_sigma**2 * a * b for b in input_map] for a in input_map]
    propagated = add(multiply(multiply(phi, covariance), transpose(phi)), process_noise)
    return predicted, propagated


def correct(state, covariance, observation, variance, gate_sigma=3.0, gate_enabled=True):
    """Scalar position observation H=[1,0,0]; Joseph covariance update."""
    residual = observation - state[0]
    innovation_variance = covariance[0][0] + variance
    accepted = not gate_enabled or residual**2 <= gate_sigma**2 * innovation_variance
    if not accepted:
        return state, covariance, residual, innovation_variance, False
    gain = [row[0] / innovation_variance for row in covariance]
    posterior = [value + k * residual for value, k in zip(state, gain)]
    residual_map = identity(3)
    for i in range(3):
        residual_map[i][0] -= gain[i]
    noise_term = [[variance * a * b for b in gain] for a in gain]
    joseph = add(multiply(multiply(residual_map, covariance), transpose(residual_map)), noise_term)
    # Remove round-off asymmetry without changing the underlying update.
    joseph = [[0.5 * (joseph[i][j] + joseph[j][i]) for j in range(3)] for i in range(3)]
    return posterior, joseph, residual, innovation_variance, True


def simulate(config=Experiment()):
    """Run deterministically on the integer sensor clock. Truth never enters KF."""
    imu_rng, gnss_rng = Random(config.seed), Random(config.seed + 1)
    dt = 1.0 / config.imu_hz
    gnss_stride = config.imu_hz // config.gnss_hz
    count = round(config.duration * config.imu_hz)
    state = [0.0, 0.0, 0.0]
    initial_sigmas = (config.initial_position_sigma, config.initial_velocity_sigma,
                      config.initial_bias_sigma)
    covariance = [[s**2 if i == j else 0.0 for j in range(3)]
                  for i, s in enumerate(initial_sigmas)]
    position = velocity = inertial_position = inertial_velocity = 0.0
    samples = [Sample(0.0, position, velocity, inertial_position, *state,
                      tuple(map(tuple, covariance)))]
    for tick in range(1, count + 1):
        time = tick / config.imu_hz
        true_acceleration = acceleration((tick - 1) / config.imu_hz)
        measured = true_acceleration + config.bias + imu_rng.gauss(0, config.acceleration_sigma)
        position += velocity * dt + 0.5 * true_acceleration * dt**2
        velocity += true_acceleration * dt
        inertial_position += inertial_velocity * dt + 0.5 * measured * dt**2
        inertial_velocity += measured * dt
        state, covariance = predict(state, covariance, measured, dt, config.acceleration_sigma)
        observation = residual = innovation_variance = accepted = prior = prior_covariance = None
        if tick % gnss_stride == 0:
            # Draw even during outage, so comparisons use identical noise streams.
            candidate = position + gnss_rng.gauss(0, config.position_sigma)
            if not config.outage_start <= time < config.outage_end:
                observation = candidate
                if abs(time - config.outlier_time) < dt / 2:
                    observation += config.outlier_metres
                prior = tuple(state)
                prior_covariance = tuple(map(tuple, covariance))
                state, covariance, residual, innovation_variance, accepted = correct(
                    state, covariance, observation, config.position_sigma**2,
                    config.gate_sigma, config.gate_enabled)
        samples.append(Sample(time, position, velocity, inertial_position, *state,
                              tuple(map(tuple, covariance)), observation, residual,
                              innovation_variance, accepted, prior, prior_covariance))
    return samples


def metrics(samples, config):
    before = samples[round(config.outage_end * config.imu_hz) - 1]
    rmse = lambda values: sqrt(sum(x*x for x in values) / len(values))
    return {
        "samples": len(samples),
        "inertial_rmse_m": rmse([s.inertial_error for s in samples]),
        "filter_rmse_m": rmse([s.error for s in samples]),
        "before_reacquisition_s": before.time,
        "inertial_error_before_reacquisition_m": before.inertial_error,
        "filter_error_before_reacquisition_m": before.error,
        "final_bias_m_s2": samples[-1].bias,
        "accepted_fixes": sum(s.accepted is True for s in samples),
        "rejected_fixes": sum(s.accepted is False for s in samples),
    }


def display_points(samples, field, stride=50):
    """Decimate smooth propagation, but retain every position-update jump."""
    if not isinstance(stride, int) or stride < 1:
        raise ValueError("Display stride must be a positive integer")
    state_fields = {"position": 0, "error": 0, "velocity": 1, "bias": 2}
    points=[]
    for i,s in enumerate(samples):
        if i % stride and s.observation is None and i != len(samples)-1:
            continue
        if field in state_fields and s.prior_state is not None:
            prior = s.prior_state[state_fields[field]]
            points.append((s.time, prior - s.truth if field == "error" else prior))
        points.append((s.time,getattr(s,field)))
    return points
