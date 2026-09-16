"""Analytic and coordinate-invariance checks for the single-fix teaching example."""
from dataclasses import replace
from math import sqrt
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]/'src'))
from model import add, identity, multiply, transpose
from measurement import (PositionFixExample, compare_position_fix, fix_batch,
                         fix_cholesky, fix_matvec, fix_sequential, fix_solve)


class MeasurementTests(unittest.TestCase):
    def setUp(self):
        self.example = PositionFixExample()
        self.result = compare_position_fix(self.example)

    def assertRowsAlmostEqual(self, left, right):
        self.assertEqual(len(left), len(right))
        for a, b in zip(left, right):
            self.assertEqual(len(a), len(b))
            for x, y in zip(a, b):
                self.assertAlmostEqual(x, y, places=11)

    def test_factor_reconstructs_covariance(self):
        self.assertRowsAlmostEqual(multiply(self.result.factor, transpose(self.result.factor)), self.example.noise)

    def test_noise_whitens_to_identity(self):
        w = self.result.whitening
        self.assertRowsAlmostEqual(multiply(multiply(w, self.example.noise), transpose(w)), identity(2))

    def test_closed_form_batch_result(self):
        self.assertRowsAlmostEqual([self.result.posterior], [(112/65, 8/65)])
        self.assertRowsAlmostEqual(self.result.covariance, ((116/65, 64/65), (64/65, 116/65)))

    def test_sequential_equals_batch(self):
        last = self.result.scalar_steps[-1]
        self.assertRowsAlmostEqual([last.correction], [self.result.posterior])
        self.assertRowsAlmostEqual(last.covariance, self.result.covariance)

    def test_observation_order_invariant_without_gating(self):
        reverse = fix_sequential(self.example.covariance, self.result.white_residual,
                                 self.result.white_sensitivity, order=(1, 0))[-1]
        self.assertRowsAlmostEqual([reverse.correction], [self.result.posterior])
        self.assertRowsAlmostEqual(reverse.covariance, self.result.covariance)

    def test_second_residual_conditions_on_first_row(self):
        first, second = self.result.scalar_steps
        expected = second.original_residual-sum(a*b for a, b in zip(second.sensitivity, first.correction))
        self.assertAlmostEqual(second.residual, expected)
        self.assertLess(second.original_residual, 0)
        self.assertGreater(second.residual, 0)

    def test_wrong_H_is_not_whitening(self):
        wrong = fix_sequential(self.example.covariance, self.result.white_residual, identity(2))[-1]
        self.assertGreater(abs(wrong.correction[1]-self.result.posterior[1]), .5)

    def test_whitening_preserves_innovation_distance(self):
        self.assertAlmostEqual(self.result.nis, 116/65)
        self.assertAlmostEqual(self.result.nis, self.result.white_nis)
        scalar_nis = sum(s.residual*s.residual/s.innovation_variance for s in self.result.scalar_steps)
        self.assertAlmostEqual(scalar_nis, self.result.nis)

    def test_whitened_innovation_is_not_identity(self):
        self.assertGreater(abs(self.result.white_innovation_covariance[0][1]), .5)
        squared_white_residual = sum(x*x for x in self.result.white_residual)
        self.assertAlmostEqual(squared_white_residual, 4)
        self.assertNotAlmostEqual(squared_white_residual, self.result.nis)

    def test_ignoring_correlation_changes_the_model(self):
        self.assertRowsAlmostEqual([self.result.diagonal_noise_posterior], [(16/9, 8/9)])
        self.assertGreater(abs(self.result.diagonal_noise_posterior[1]-self.result.posterior[1]), .7)

    def test_rotate_covariance_with_destination_on_left(self):
        c = ((1/sqrt(2), -1/sqrt(2)), (1/sqrt(2), 1/sqrt(2)))
        self.assertRowsAlmostEqual(multiply(multiply(c, ((9, 0), (0, 1))), transpose(c)), self.example.noise)

    def test_state_unit_conversion(self):
        unit = 100.0  # centimetres; posterior state must stay in the caller's units.
        scaled = replace(self.example, prior=tuple(unit*x for x in self.example.prior),
                         observation=tuple(unit*x for x in self.example.observation),
                         covariance=tuple(tuple(unit**2*x for x in row) for row in self.example.covariance),
                         noise=tuple(tuple(unit**2*x for x in row) for row in self.example.noise))
        result = compare_position_fix(scaled)
        self.assertRowsAlmostEqual([result.posterior], [[unit*x for x in self.result.posterior]])
        self.assertAlmostEqual(result.nis, self.result.nis)

    def test_translation_does_not_change_correction(self):
        offset = (10, -20)
        moved = replace(self.example, prior=offset,
                        observation=tuple(x+o for x, o in zip(self.example.observation, offset)))
        result = compare_position_fix(moved)
        self.assertRowsAlmostEqual([result.posterior], [[x+o for x, o in zip(self.result.posterior, offset)]])

    def test_diagonal_noise_still_works(self):
        e = replace(self.example, noise=((1, 0), (0, 9)))
        r = compare_position_fix(e)
        self.assertRowsAlmostEqual([r.scalar_steps[-1].correction], [r.posterior])

    def test_invalid_covariance_rejected(self):
        for invalid in (((1, 2), (0, 1)), ((1, 2), (2, 1)), ((0, 0), (0, 1)), ((1, 0), (0, float('nan')))):
            with self.assertRaises(ValueError):
                fix_cholesky(invalid)

    def test_input_is_unchanged(self):
        self.assertEqual(self.example, PositionFixExample())
        with self.assertRaises(ValueError):
            fix_sequential(self.example.covariance, self.result.white_residual, self.result.whitening, (0, 0))


if __name__ == '__main__':
    unittest.main()
