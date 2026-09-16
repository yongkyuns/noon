"""Convention, geometry and reset checks without Noon or third-party packages."""
from dataclasses import replace
from math import cos, degrees, pi, radians, sin, sqrt
import unittest
from attitude import (AttitudeExample, attitude_example, inject_left, left_error,
                      left_reset_jacobian, quat_conjugate, quat_from_rotvec,
                      quat_product, quat_rotate, quat_to_rotvec, quat_unit,
                      reset_left_error)
from model import multiply, transpose


class AttitudeTests(unittest.TestCase):
    def assertVector(self, actual, expected, places=10):
        self.assertEqual(len(actual), len(expected))
        for a, b in zip(actual, expected):
            self.assertAlmostEqual(a, b, places=places)

    def test_identity_and_small_angle(self):
        self.assertEqual(quat_from_rotvec((0, 0, 0)), (1, 0, 0, 0))
        self.assertVector(quat_to_rotvec(quat_from_rotvec((1e-10, -2e-10, 3e-10))), (1e-10, -2e-10, 3e-10), 15)

    def test_hamilton_product(self):
        self.assertEqual(quat_product((0,1,0,0), (0,0,1,0)), (0,0,0,1))
        self.assertEqual(quat_product((0,0,1,0), (0,1,0,0)), (0,0,0,-1))

    def test_ned_axis_rotation(self):
        self.assertVector(quat_rotate(quat_from_rotvec((0,0,pi/2)), (1,0,0)), (0,1,0))
        self.assertVector(quat_rotate(quat_from_rotvec((0,pi/2,0)), (1,0,0)), (0,0,-1))

    def test_inverse_and_norm(self):
        q = quat_from_rotvec((.2,-.1,.3)); v = (2., -4., 7.)
        rotated = quat_rotate(q, v)
        self.assertAlmostEqual(sum(x*x for x in rotated), sum(x*x for x in v))
        self.assertVector(quat_rotate(quat_conjugate(q), rotated), v)

    def test_double_cover(self):
        q = quat_from_rotvec((.2,.3,-.4)); minus = tuple(-v for v in q)
        self.assertVector(quat_rotate(q, (1,2,3)), quat_rotate(minus, (1,2,3)))
        self.assertVector(quat_to_rotvec(q), quat_to_rotvec(minus))

    def test_rotvec_roundtrip(self):
        for v in ((.1,.2,-.3), (pi-1e-6,0,0), (0,0,-pi+1e-6)):
            self.assertVector(quat_to_rotvec(quat_from_rotvec(v)), v)

    def test_prescribed_pitch_correction(self):
        r = attitude_example()
        self.assertVector(r['post_error_deg'], (0,-2,0))
        self.assertVector(r['posterior_q'], quat_from_rotvec((0,radians(14),0)))
        self.assertAlmostEqual(sum(x*x for x in r['posterior_q']), 1.)

    def test_stationary_specific_force_and_gravity(self):
        c = AttitudeExample(); r = attitude_example(c)
        self.assertAlmostEqual(sqrt(sum(v*v for v in r['sensor_specific_force_m_s2'])), c.gravity_m_s2)
        recovered = quat_rotate(r['truth_q'], r['sensor_specific_force_m_s2'])
        self.assertVector(recovered, (0,0,-c.gravity_m_s2))

    def test_gravity_leakage_against_trigonometry(self):
        c = AttitudeExample(); r = attitude_example(c)
        for field, difference in [('prior',8), ('post',2)]:
            a = r[field+'_acceleration_m_s2']
            self.assertVector(a, (-c.gravity_m_s2*sin(radians(difference)), 0,
                                  c.gravity_m_s2*(1-cos(radians(difference)))))
            self.assertAlmostEqual(r[field+'_coast_north_error_m'], .5*a[0]*c.coast_seconds**2)

    def test_perfect_attitude_has_zero_leakage(self):
        c = replace(AttitudeExample(), prior_pitch_deg=12, correction_pitch_deg=0)
        self.assertVector(attitude_example(c)['prior_acceleration_m_s2'], (0,0,0))

    def test_wrong_order_changes_axis(self):
        r = attitude_example()
        self.assertVector(r['order_left_vector_ned'], (0,sqrt(3)/2,.5))
        self.assertVector(r['order_right_vector_ned'], (0,1,0))

    def test_body_and_navigation_corrections_agree_when_transformed(self):
        r = attitude_example(); q = quat_from_rotvec((0,0,pi/2))
        corrected = quat_product(q, quat_from_rotvec(r['equivalent_body_correction_rad']))
        self.assertVector(quat_rotate(corrected, (1,0,0)), r['order_left_vector_ned'])

    def test_normalizing_component_addition_is_not_composition(self):
        r = attitude_example()
        wrong = quat_unit(tuple(a+b for a,b in zip(r['prior_q'], r['delta_q'])))
        self.assertGreater(abs(degrees(quat_to_rotvec(wrong)[1])-14), 1)

    def test_reset_zero_at_injected_mean(self):
        c = (.04,-.1,.03)
        self.assertVector(reset_left_error(c, c), (0,0,0))
        self.assertVector(left_reset_jacobian((0,0,0))[0], (1,0,0))

    def test_reset_preserves_physical_orientation(self):
        old, c = (.06,-.11,.02), (.04,-.1,.03)
        q = quat_from_rotvec((.3,-.2,.4))
        before = inject_left(q, old)
        after = inject_left(inject_left(q, c), reset_left_error(old, c))
        self.assertVector(quat_rotate(before, (1,2,3)), quat_rotate(after, (1,2,3)))

    def test_reset_jacobian_finite_difference(self):
        c, step = (.1,-.2,.07), 1e-6
        jacobian = left_reset_jacobian(c)
        for j in range(3):
            plus, minus = list(c), list(c)
            plus[j] += step; minus[j] -= step
            fp, fm = reset_left_error(plus,c), reset_left_error(minus,c)
            for i in range(3):
                self.assertAlmostEqual(jacobian[i][j], (fp[i]-fm[i])/(2*step), places=8)

    def test_reset_sign_is_positive_for_left_error(self):
        jac = left_reset_jacobian((0,0,1e-5))
        self.assertAlmostEqual(jac[1][0], 5e-6, places=12)
        self.assertAlmostEqual(jac[0][1], -5e-6, places=12)

    def test_reset_covariance_is_symmetric_psd(self):
        r = attitude_example(); p = r['reset_after_rad2']
        for i in range(3):
            for j in range(3): self.assertAlmostEqual(p[i][j], p[j][i])
        for v in ((1,0,0),(0,1,0),(0,0,1),(1,-2,3)):
            self.assertGreater(sum(v[i]*p[i][j]*v[j] for i in range(3) for j in range(3)), 0)
        self.assertLess(r['reset_after_deg2'][0][2], 0)
        self.assertAlmostEqual(r['reset_before_deg2'][0][0], 4.)

    def test_reset_linearization_predicts_nearby_error(self):
        c, e = (.1,-.2,.07), (1e-7,-2e-7,3e-7)
        actual = reset_left_error(tuple(a+b for a,b in zip(c,e)), c)
        linearized = [sum(a*b for a,b in zip(row,e)) for row in left_reset_jacobian(c)]
        self.assertVector(actual, linearized, 12)

    def test_invalid_inputs(self):
        for q in ((0,0,0,0), (1,0,0), (float('nan'),0,0,0)):
            with self.assertRaises(ValueError): quat_unit(q)
        for v in ((1,2), (0,float('inf'),0)):
            with self.assertRaises(ValueError): quat_from_rotvec(v)
        for options in ({'gravity_m_s2':0}, {'coast_seconds':-1}, {'reset_sigma_deg':(1,0,2)}):
            with self.assertRaises(ValueError): AttitudeExample(**options)
