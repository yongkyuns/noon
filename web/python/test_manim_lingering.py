import subprocess
import sys
import unittest
from pathlib import Path

import _manim_rate_functions as rates
import noon


class LingeringRate(unittest.TestCase):
    def test_public_identity_and_shared_rate_mapping(self):
        self.assertIs(noon.lingering, rates.lingering)
        self.assertIs(noon.rate_functions.lingering, rates.lingering)
        self.assertEqual(rates.easing_from_rate_func(rates.lingering), 'lingering')
        subprocess.run([sys.executable, '-S', '-c',
                        "import sys; from noon import lingering; assert lingering(0.4) == 0.5; assert 'numpy' not in sys.modules"],
                       cwd=Path(__file__).parent, check=True)

    def test_original_linear_rise_and_early_endpoint_hold(self):
        for progress, expected in [(0, 0.0), (0.4, 0.5), (0.8, 1.0), (0.9, 1.0), (1, 1.0)]:
            self.assertEqual(rates.lingering(progress), expected)
        self.assertEqual(rates.lingering(-1), 0)
        self.assertEqual(rates.lingering(2), 1)
        self.assertEqual(rates.lingering(float('nan')), 1)


if __name__ == '__main__':
    unittest.main()
