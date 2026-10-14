import subprocess
import runpy
import sys
import unittest
from pathlib import Path
from types import SimpleNamespace

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

    def test_create_passes_reverse_to_shared_options_without_admitting_it_to_affine(self):
        resolved = SimpleNamespace(lag_ratio=0.0, path_arc=0.0, reverse_rate_function=True)
        animation = SimpleNamespace(anim_args={'reverse_rate_function': True})
        # Execute the actual coercion helpers without unrelated browser imports.
        loader = runpy.run_path(str(Path(__file__).resolve().parents[2] / 'scripts/pinned_python_helpers.py'))['load_helpers']
        helpers = loader(Path(__file__).with_name('_manim_scene.py'),
                         {'_canonical_create_options', '_canonical_affine_options'},
                         {'_options': SimpleNamespace(resolve=lambda **kwargs: resolved,
                          builder_args=lambda value: value.anim_args)})
        self.assertIs(helpers['_canonical_create_options'](animation, {}), resolved)
        self.assertIsNone(helpers['_canonical_affine_options'](animation, {}))
        animation.anim_args['introducer'] = False
        self.assertIsNone(helpers['_canonical_create_options'](animation, {}))


if __name__ == '__main__':
    unittest.main()
