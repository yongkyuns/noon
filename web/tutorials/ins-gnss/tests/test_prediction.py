"""Independent closed-form versus actual samplewise propagation contracts."""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'src'))
from model import Experiment, add, predict, simulate
from prediction import coast_covariance, outage_prediction, position_variance_terms


class PredictionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.config = Experiment()
        cls.samples = simulate(cls.config)

    def assertMatrixClose(self, first, second, places=9):
        for a, b in zip(first, second):
            for x, y in zip(a, b):
                self.assertAlmostEqual(x, y, places=places)

    def test_zero_steps_preserve_covariance(self):
        p = self.samples[4400].covariance
        old, new = coast_covariance(p, .01, 0, .04)
        self.assertMatrixClose(old, p)
        self.assertMatrixClose(new, [[0.] * 3 for _ in range(3)])

    def test_one_step_noise_and_full_covariance(self):
        p = self.samples[4400].covariance
        old, new = coast_covariance(p, .01, 1, .04)
        _, expected = predict([0., 0., 0.], p, .5, .01, .04)
        self.assertMatrixClose(add(old, new), expected)
        self.assertAlmostEqual(new[0][0], .04**2 * (.5 * .01**2)**2)

    def test_many_steps_against_original_predictor(self):
        for dt, n in ((.01, 137), (.02, 250), (.2, 37)):
            initial = self.samples[4400].covariance
            state, p = [0., 0., 0.], initial
            for _ in range(n):
                state, p = predict(state, p, .5, dt, .04)
            self.assertMatrixClose(add(*coast_covariance(initial, dt, n, .04)), p)

    def test_prediction_begins_at_last_accepted_fix(self):
        start, end, *_ = outage_prediction(self.samples, self.config)
        self.assertEqual((start.time, end.time), (44., 75.))
        self.assertEqual(end.time - start.time, 31.)
        self.assertEqual(self.config.outage_end - self.config.outage_start, 30.)

    def test_closed_form_matches_retained_prior_not_posterior(self):
        _, event, old, new, terms = outage_prediction(self.samples, self.config)
        self.assertMatrixClose(add(old, new), event.prior_covariance)
        self.assertAlmostEqual(sum(terms.values()), event.prior_covariance[0][0], places=9)
        self.assertGreater(event.prior_covariance[0][0], event.covariance[0][0])

    def test_all_prediction_checkpoints(self):
        start = self.samples[4400]
        for t in (45., 50., 60., 70., 74.99):
            old, new = coast_covariance(start.covariance, .01, round((t - start.time) * 100), .04)
            self.assertMatrixClose(add(old, new), self.samples[round(t * 100)].covariance)

    def test_zero_new_noise_does_not_remove_inherited_uncertainty(self):
        start, _, old, new, _ = outage_prediction(self.samples, self.config)
        no_noise, zeros = coast_covariance(start.covariance, .01, 3100, 0.)
        self.assertMatrixClose(old, no_noise)
        self.assertMatrixClose(zeros, [[0.] * 3 for _ in range(3)])
        self.assertGreater(no_noise[0][0], start.covariance[0][0])
        self.assertGreater(new[0][0], 0.)

    def test_cross_terms_can_be_negative(self):
        # PSD covariance of positively correlated position and physical bias.
        p = ((1., 0., .5), (0., 1., 0.), (.5, 0., 1.))
        terms = position_variance_terms(p, 2., 0.)
        self.assertLess(terms['position_bias'], 0.)
        self.assertAlmostEqual(sum(terms.values()), coast_covariance(p, 1., 2, 0.)[0][0][0])

    def test_bias_variance_is_constant_in_this_model(self):
        window = self.samples[4400:7500]
        self.assertTrue(all(s.bias == window[0].bias for s in window))
        self.assertTrue(all(s.covariance[2][2] == window[0].covariance[2][2] for s in window))

    def test_noise_variance_scales_quadratically_with_sample_sigma(self):
        p = self.samples[4400].covariance
        a = coast_covariance(p, .01, 100, .04)[1]
        b = coast_covariance(p, .01, 100, .08)[1]
        self.assertMatrixClose([[4 * x for x in row] for row in a], b)

    def test_missing_endpoints_and_invalid_inputs(self):
        with self.assertRaises(ValueError):
            outage_prediction([], self.config)
        for dt, steps, sigma in ((0., 1, .04), (.01, -1, .04), (.01, 1.5, .04),
                                 (.01, True, .04), (.01, 1, -.1), (float('nan'), 1, .1)):
            with self.assertRaises(ValueError):
                coast_covariance([[0.] * 3 for _ in range(3)], dt, steps, sigma)


if __name__ == '__main__':
    unittest.main()
