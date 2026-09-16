"""Numerical contracts; can also execute under the pinned Pyodide interpreter."""
from dataclasses import replace
from math import isclose, log, sqrt
from pathlib import Path
import sys
import unittest

sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'src'))
from model import Experiment, correct, predict, simulate, metrics, display_points
from uncertainty import confidence_contour


class ModelTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.config=Experiment()
        cls.rows=simulate(cls.config)

    def test_deterministic(self):
        short=replace(self.config,duration=2,outage_start=0,outage_end=0)
        self.assertEqual(simulate(short),simulate(short))

    def test_sensor_clock(self):
        self.assertEqual(len(self.rows),12001)
        self.assertEqual(self.rows[-1].time,120)
        self.assertTrue(all(b.time>a.time for a,b in zip(self.rows,self.rows[1:])))

    def test_outage_has_no_observations(self):
        self.assertTrue(all(s.observation is None for s in self.rows if 45<=s.time<75))
        self.assertIsNotNone(self.rows[7500].observation)

    def test_position_fix_frequency(self):
        self.assertTrue(all(isclose(s.time,round(s.time)) for s in self.rows if s.observation is not None))

    def test_scalar_update(self):
        x,P,*_=correct([10,0,0],[[9,0,0],[0,1,0],[0,0,1]],14,4)
        self.assertAlmostEqual(x[0],10+9/13*4)
        self.assertAlmostEqual(P[0][0],36/13)

    def test_bias_cross_covariance_is_information_path(self):
        state,cov=predict([0,0,0],[[0,0,0],[0,0,0],[0,0,.01]],0,1,0)
        self.assertLess(cov[0][2],0)
        updated,*_=correct(state,cov,-1,1)
        self.assertGreater(updated[2],0)

    def test_prediction_constant_bias(self):
        x,_=predict([0,0,.02],[[0]*3 for _ in range(3)],.52,2,0)
        self.assertEqual(x,[1,1,.02])

    def test_constant_bias_integrates_quadratically(self):
        c=replace(self.config,duration=4,outage_start=0,outage_end=4,
                  acceleration_sigma=0,position_sigma=.001)
        s=simulate(c)[-2]
        self.assertAlmostEqual(s.inertial_error,.5*c.bias*s.time**2,places=10)

    def test_covariance_symmetric_positive(self):
        for s in self.rows[::100]:
            p=s.covariance
            self.assertTrue(all(p[i][i]>=0 for i in range(3)))
            self.assertLess(max(abs(p[i][j]-p[j][i]) for i in range(3) for j in range(3)),1e-10)
            self.assertGreaterEqual(p[0][0]*p[1][1]-p[0][1]**2,-1e-10)
            determinant=(p[0][0]*(p[1][1]*p[2][2]-p[1][2]**2)
                         -p[0][1]*(p[0][1]*p[2][2]-p[1][2]*p[0][2])
                         +p[0][2]*(p[0][1]*p[1][2]-p[1][1]*p[0][2]))
            self.assertGreaterEqual(determinant,-1e-10)

    def test_known_bias_estimation(self):
        self.assertLess(abs(self.rows[-1].bias-self.config.bias),.005)
        m=metrics(self.rows,self.config)
        self.assertLess(m['filter_rmse_m'],m['inertial_rmse_m']/10)

    def test_rejected_outlier_changes_nothing(self):
        state,P=[0,0,0],[[1,0,0],[0,1,0],[0,0,1]]
        x,newP,r,S,ok=correct(state,P,100,1)
        self.assertFalse(ok)
        self.assertEqual(x,state);self.assertEqual(P,newP)

    def test_reacquisition_keeps_prior(self):
        s=self.rows[7500]
        self.assertIsNotNone(s.prior_position)
        self.assertNotEqual(s.prior_position,s.position)
        self.assertAlmostEqual(s.innovation,s.observation-s.prior_position)

    def test_invalid_configuration(self):
        for kwargs in ({'imu_hz':0},{'imu_hz':99,'gnss_hz':2},
                       {'outage_start':80,'outage_end':20},{'position_sigma':0}):
            with self.assertRaises(ValueError): Experiment(**kwargs)


    def test_retained_update_replays_exactly(self):
        event = self.rows[7500]
        state, covariance, residual, variance, accepted = correct(
            event.prior_state, event.prior_covariance, event.observation,
            self.config.position_sigma**2)
        self.assertEqual(tuple(state), event.state)
        self.assertEqual(tuple(map(tuple, covariance)), event.covariance)
        self.assertEqual((residual, variance, accepted),
                         (event.innovation, event.innovation_variance, event.accepted))
        for actual, gain in zip(event.correction, event.gain):
            self.assertAlmostEqual(actual, gain * event.innovation, places=12)

    def test_joseph_matches_independent_scalar_covariance_formula(self):
        event = self.rows[7500]
        p = event.prior_covariance
        for i in range(3):
            for j in range(3):
                expected = p[i][j] - p[i][0] * p[0][j] / event.innovation_variance
                self.assertAlmostEqual(event.covariance[i][j], expected, places=12)

    def test_single_update_cross_covariance_ablation(self):
        event = self.rows[7500]
        diagonal = [[event.prior_covariance[i][i] if i == j else 0.0
                     for j in range(3)] for i in range(3)]
        state, *_ = correct(event.prior_state, diagonal, event.observation,
                            self.config.position_sigma**2)
        self.assertEqual(state[0], event.position)
        self.assertEqual(state[1:], list(event.prior_state[1:]))
        self.assertNotEqual(state[1:], list(event.state[1:]))

    def test_display_retains_each_state_update_discontinuity(self):
        rows = self.rows[7400:7601]
        event = self.rows[7500]
        for field, index in (('position', 0), ('velocity', 1), ('bias', 2), ('error', 0)):
            at_event = [value for time, value in display_points(rows, field, 77)
                        if time == event.time]
            offset = event.truth if field == 'error' else 0
            self.assertEqual(at_event, [event.prior_state[index] - offset,
                                        event.state[index] - offset])

    def test_prediction_only_sample_has_no_update_snapshot(self):
        sample = self.rows[7499]
        self.assertIsNone(sample.prior_state)
        self.assertIsNone(sample.prior_covariance)
        self.assertIsNone(sample.gain)
        self.assertIsNone(sample.correction)

    def test_rejected_event_retains_candidate_but_injects_zero(self):
        config = replace(self.config, duration=3, outage_start=0, outage_end=0,
                         outlier_time=2, outlier_metres=1000)
        event = simulate(config)[200]
        self.assertIs(event.accepted, False)
        self.assertEqual(event.correction, (0., 0., 0.))
        self.assertGreater(event.gain[0], 0)
        self.assertEqual(event.covariance, event.prior_covariance)

    def test_actual_joint_contour_has_expected_mahalanobis_radius(self):
        event = self.rows[7500]
        for full, mean in ((event.prior_covariance, event.prior_state),
                           (event.covariance, event.state)):
            pair = ((full[0][0], full[0][2]), (full[2][0], full[2][2]))
            centre = (mean[0], mean[2])
            contour = confidence_contour(centre, pair)
            self.assertEqual(contour[0], contour[-1])
            a, b, d = pair[0][0], pair[0][1], pair[1][1]
            for point in contour:
                x, y = point[0] - centre[0], point[1] - centre[1]
                distance = (d*x*x - 2*b*x*y + a*y*y)/(a*d-b*b)
                self.assertAlmostEqual(distance, -2*log(.05), places=6)

    def test_contour_rejects_invalid_covariance_and_probability(self):
        for matrix in (((1,2),(2,1)), ((1,.1),(.2,1)), ((float('nan'),0),(0,1))):
            with self.assertRaises(ValueError): confidence_contour((0,0), matrix)
        for probability in (0, 1, float('nan')):
            with self.assertRaises(ValueError):
                confidence_contour((0,0), ((1,0),(0,1)), probability)

    def test_display_stride_validation(self):
        for stride in (0, -1, .5):
            with self.assertRaises(ValueError): display_points(self.rows, 'bias', stride)


from test_prediction import PredictionTests

if __name__=='__main__':unittest.main()
