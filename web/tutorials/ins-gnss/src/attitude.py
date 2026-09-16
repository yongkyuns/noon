"""Quaternion correction examples, not an attitude estimator or full INS.

Hamilton, scalar-first unit quaternions map sensor vectors into local NED.
Left errors are navigation-frame rotation vectors in radians:
q_true = Q(delta_theta) * q_nominal. The correction is prescribed, not inferred
from truth or from a new sensor update. Gravity leakage is evaluated at rest.
"""
from dataclasses import dataclass
from math import atan2, cos, degrees, isfinite, radians, sin, sqrt
from model import identity, multiply, transpose


def _finite_components(values, size):
    values = tuple(values)
    if len(values) != size or not all(isfinite(x) for x in values):
        raise ValueError(f'Expected {size} finite components')
    return values


def quat_unit(values):
    q = _finite_components(values, 4)
    length = sqrt(sum(x*x for x in q))
    if length < 1e-15:
        raise ValueError('Quaternion norm must be nonzero')
    return tuple(x/length for x in q)


def quat_product(left, right):
    """Hamilton product; no normalization (also used for pure vectors)."""
    w, x, y, z = _finite_components(left, 4)
    a, b, c, d = _finite_components(right, 4)
    return (w*a-x*b-y*c-z*d, w*b+x*a+y*d-z*c,
            w*c-x*d+y*a+z*b, w*d+x*c-y*b+z*a)


def quat_conjugate(q):
    w, x, y, z = _finite_components(q, 4)
    return (w, -x, -y, -z)


def quat_from_rotvec(vector):
    v = _finite_components(vector, 3)
    angle = sqrt(sum(x*x for x in v))
    factor = .5-angle*angle/48 if angle < 1e-8 else sin(angle/2)/angle
    return quat_unit((cos(angle/2), *(factor*x for x in v)))


def quat_to_rotvec(quaternion):
    q = quat_unit(quaternion)
    if q[0] < 0:
        q = tuple(-x for x in q)
    length = sqrt(sum(x*x for x in q[1:]))
    factor = 2.0 if length < 1e-12 else 2*atan2(length, q[0])/length
    return tuple(factor*x for x in q[1:])


def quat_rotate(quaternion, vector):
    q = quat_unit(quaternion)
    v = _finite_components(vector, 3)
    return quat_product(quat_product(q, (0.0, *v)), quat_conjugate(q))[1:]


def inject_left(nominal, correction):
    return quat_unit(quat_product(quat_from_rotvec(correction), quat_unit(nominal)))


def left_error(truth, nominal):
    return quat_to_rotvec(quat_product(quat_unit(truth), quat_conjugate(quat_unit(nominal))))


def reset_left_error(old_error, correction):
    """Same physical orientation, recentered on the corrected nominal."""
    return quat_to_rotvec(quat_product(quat_from_rotvec(old_error),
                                      quat_conjugate(quat_from_rotvec(correction))))


def left_reset_jacobian(correction):
    """Exact derivative of Log(Exp(c+e) Exp(-c)) at e=0: SO(3) left Jacobian."""
    x, y, z = _finite_components(correction, 3)
    angle2 = x*x+y*y+z*z
    skew = [[0., -z, y], [z, 0., -x], [-y, x, 0.]]
    if angle2 < 1e-8:
        a, b = .5-angle2/24, 1/6-angle2/120
    else:
        angle = sqrt(angle2)
        a, b = (1-cos(angle))/angle2, (angle-sin(angle))/(angle2*angle)
    squared, unit = multiply(skew, skew), identity(3)
    return [[unit[i][j]+a*skew[i][j]+b*squared[i][j] for j in range(3)] for i in range(3)]


@dataclass(frozen=True)
class AttitudeExample:
    true_pitch_deg: float = 12.0
    prior_pitch_deg: float = 20.0
    correction_pitch_deg: float = -6.0
    gravity_m_s2: float = 9.81
    coast_seconds: float = 10.0
    order_yaw_deg: float = 90.0
    order_correction_deg: float = 30.0
    reset_sigma_deg: tuple = (2.0, 1.0, 3.0)

    def __post_init__(self):
        scalars = (self.true_pitch_deg, self.prior_pitch_deg, self.correction_pitch_deg,
                   self.gravity_m_s2, self.coast_seconds, self.order_yaw_deg, self.order_correction_deg)
        if not all(isfinite(v) for v in scalars) or self.gravity_m_s2 <= 0 or self.coast_seconds <= 0:
            raise ValueError('Finite angles and positive gravity/time are required')
        if any(v <= 0 for v in _finite_components(self.reset_sigma_deg, 3)):
            raise ValueError('Reset standard deviations must be positive')


def attitude_example(config=AttitudeExample()):
    """All chapter readouts and endpoint geometry derive from this one result."""
    truth = quat_from_rotvec((0, radians(config.true_pitch_deg), 0))
    prior = quat_from_rotvec((0, radians(config.prior_pitch_deg), 0))
    correction = (0, radians(config.correction_pitch_deg), 0)
    posterior = inject_left(prior, correction)
    gravity = (0, 0, config.gravity_m_s2)
    measured = quat_rotate(quat_conjugate(truth), tuple(-v for v in gravity))
    acceleration = lambda q: tuple(f+g for f, g in zip(quat_rotate(q, measured), gravity))
    prior_accel, post_accel = acceleration(prior), acceleration(posterior)
    yaw = quat_from_rotvec((0, 0, radians(config.order_yaw_deg)))
    turn = quat_from_rotvec((radians(config.order_correction_deg), 0, 0))
    left = quat_rotate(quat_product(turn, yaw), (1, 0, 0))
    right = quat_rotate(quat_product(yaw, turn), (1, 0, 0))
    mapped_body_turn = quat_rotate(quat_conjugate(yaw), (radians(config.order_correction_deg), 0, 0))
    jacobian = left_reset_jacobian(correction)
    covariance = [[radians(s)**2 if i == j else 0.0 for j in range(3)]
                  for i, s in enumerate(config.reset_sigma_deg)]
    reset_covariance = multiply(multiply(jacobian, covariance), transpose(jacobian))
    return {
        'scope': 'Prescribed rotation correction; no attitude EKF or ECEF replay',
        'convention': 'Hamilton wxyz, sensor-to-NED, left/navigation error, radians internally',
        'truth_q': truth, 'prior_q': prior, 'correction_rad': correction,
        'delta_q': quat_from_rotvec(correction), 'posterior_q': posterior,
        'prior_error_deg': tuple(map(degrees, left_error(truth, prior))),
        'post_error_deg': tuple(map(degrees, left_error(truth, posterior))),
        'sensor_specific_force_m_s2': measured,
        'prior_acceleration_m_s2': prior_accel, 'post_acceleration_m_s2': post_accel,
        'prior_coast_north_error_m': .5*prior_accel[0]*config.coast_seconds**2,
        'post_coast_north_error_m': .5*post_accel[0]*config.coast_seconds**2,
        'order_left_vector_ned': left, 'order_right_vector_ned': right,
        'equivalent_body_correction_rad': mapped_body_turn,
        'reset_jacobian': jacobian, 'reset_before_rad2': covariance,
        'reset_after_rad2': reset_covariance,
        'reset_before_deg2': [[degrees(1)**2*x for x in row] for row in covariance],
        'reset_after_deg2': [[degrees(1)**2*x for x in row] for row in reset_covariance],
    }
