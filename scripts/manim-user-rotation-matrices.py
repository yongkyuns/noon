"""Compare explicit user rotation matrices with unchanged pinned definitions."""
import argparse
from decimal import Decimal
from fractions import Fraction
import hashlib
import inspect
import json
from pathlib import Path
import runpy
import sys
import warnings

import numpy as np
from scipy.spatial.transform import Rotation
from pinned_python_helpers import load_helpers

NAMES = ('rotation_matrix',)
normalize_value = runpy.run_path(str(Path(__file__).with_name('manim-simple-functions.py')),
                               run_name='_rotation_data_normalization')['normalize']


def observations(namespace):
    result = [[name, 'signature', str(inspect.signature(namespace[name]))] for name in NAMES]

    def record(name, *args):
        before = normalize_value(args)
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter('always')
            try:
                value = namespace[name](*args)
                outcome = ['value', type(value).__name__, normalize_value(value)]
                if isinstance(value, np.ndarray):
                    outcome.append([np.shares_memory(value, arg) if isinstance(arg, np.ndarray)
                                    else False for arg in args])
            except Exception as error:
                outcome = ['error', type(error).__name__]
        result.append([name, normalize_value(args), outcome,
                       [type(item.message).__name__ for item in caught],
                       normalize_value(args) == before])

    vectors = [[], [0], [1, 0], [0, 0, 0], [1, 0, 0], [-1, 0, 0], [0, 1, 0],
               [0, 0, -1], [1, 2, 3], [1, 2, 3, 4], [float('inf'), 0, 1],
               [float('nan'), 1, 2], ['x', 1, 2], [1j, 2, 3],
               np.array([1, 2, 3], dtype=np.float32), np.array([1, 2, 3], dtype=np.int64),
               np.arange(6, dtype=float)[::2], np.array([[1, 0, 0], [0, 1, 0]]),
               [Fraction(1, 3), Fraction(2, 3), 1], [Decimal('0.1'), Decimal('0.2'), 1]]
    for vector in vectors:
        for angle in (0, 0.2, np.pi, -np.pi / 3, np.float32(0.2), Fraction(1, 3),
                      Decimal('0.2'), True, float('inf'), float('nan'), 'bad'):
            for homogeneous in (False, True, 0, 1, 'yes', None):
                record('rotation_matrix', angle, vector, homogeneous)
    return result


def qualify(source_root, *, installed_manim=False):
    source = source_root / 'manim/utils/space_ops.py'
    original = load_helpers(source, {*NAMES, 'normalize'},
                            {'np': np, 'Rotation': Rotation,
                             'RIGHT': np.array([1., 0., 0.]), 'UP': np.array([0., 1., 0.])})
    expected = observations(original)
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'web/python'))
    import _manim_user_vectors
    import noon
    actual = observations(vars(_manim_user_vectors))
    errors = [] if actual == expected else ['rotation matrix user-data observations differ']
    differences = [[i, a, b] for i, (a, b) in enumerate(zip(expected, actual)) if a != b]
    for name in NAMES:
        if name not in noon.__all__ or getattr(noon, name) is not getattr(_manim_user_vectors, name):
            errors.append(f'public user-data helper identity differs: {name}')
    if installed_manim:
        from manim.utils import space_ops
        if Path(space_ops.__file__).read_bytes() != source.read_bytes():
            raise ValueError('installed complete space-ops source differs from pin')
        if observations(vars(space_ops)) != expected:
            errors.append('installed rotation matrix observations differ')
    return {'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
            'functions': len(NAMES), 'observations': len(expected),
            'installed_module_checked': installed_manim, 'errors': errors,
            'differences': differences}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('source_root', type=Path)
    parser.add_argument('--installed-manim', action='store_true')
    args = parser.parse_args()
    result = qualify(args.source_root, installed_manim=args.installed_manim)
    print(json.dumps(result, indent=2))
    sys.exit(bool(result['errors']))
