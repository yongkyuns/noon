"""Qualify explicit user-vector normals against unchanged pinned definitions."""
import argparse
import hashlib
import inspect
import json
from pathlib import Path
import runpy
import sys
import warnings

import numpy as np
from pinned_python_helpers import load_helpers

normalize = runpy.run_path(str(Path(__file__).with_name('manim-simple-functions.py')),
                          run_name='_normal_observation_values')['normalize']


def observations(namespace):
    function = namespace['get_unit_normal']
    result = [['signature', str(inspect.signature(function))]]
    vectors = [[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1],
               [0, 0, -2], [-1, 2, 3], [1e-200, 2e-200, 0],
               [1e200, -2e200, 3e200], [1, 0, 1e-7]]

    def record(first, second, **kwargs):
        before = normalize((first, second))
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter('always')
            try:
                value = function(first, second, **kwargs)
                outcome = ['value', type(value).__name__, normalize(value),
                           value is first, value is second]
            except Exception as error:
                outcome = ['error', type(error).__name__]
        result.append([before, normalize(kwargs.get('tol', 1e-6)), outcome,
                       normalize((first, second)),
                       [[type(w.message).__name__, str(w.message)] for w in caught]])

    for dtype in (np.int64, np.float32, np.float64, np.complex128):
        for vector in vectors:
            if dtype is np.int64 and max(abs(x) for x in vector) > 1e18:
                continue
            with warnings.catch_warnings():
                warnings.simplefilter('ignore')
                array = np.array(vector, dtype=dtype)
            for first in (array, array.tolist(), tuple(array.tolist())):
                for second in vectors[:6]:
                    record(first, second)
            array.flags.writeable = False
            record(array, array)
    for value in ([], [1], [1, 2], [1, 2, 3, 4], [[1, 2, 3]], None, 'x',
                  [float('nan'), 0, 0], [float('inf'), 1, 0]):
        record(value, [1, 2, 3])
        record([1, 2, 3], value)
    for tolerance in (0, 1e-8, 1e-6, 1, -1, float('nan'), 'x', np.float32(1e-6)):
        record([1, 0, 0], [1, 0, 1e-7], tol=tolerance)
    first = function([0, 0, 0], [0, 0, 0])
    second = function([0, 0, 0], [0, 0, 1])
    result.append(['fallback_identity', first is second])
    original = first.copy()
    try:
        first[0] = 7
        result.append(['fallback_mutation', normalize(function([0, 0, 0], [0, 0, 1]))])
    finally:
        first[:] = original
    return result


def qualify(source_root, *, installed_manim=False):
    source = source_root / 'manim/utils/space_ops.py'
    original = load_helpers(source, {'get_unit_normal', 'cross', 'norm_squared'},
                            {'np': np, 'DOWN': np.array([0., -1., 0.])})
    expected = observations(original)
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'web/python'))
    import _manim_user_vector_bases as helpers
    import noon
    errors = [] if observations(vars(helpers)) == expected else ['normal observations differ']
    if noon.get_unit_normal is not helpers.get_unit_normal:
        errors.append('public normal identity differs')
    if installed_manim:
        from manim.utils import space_ops
        if Path(space_ops.__file__).read_bytes() != source.read_bytes():
            raise ValueError('installed complete space-ops source differs from pin')
        if observations(vars(space_ops)) != expected:
            errors.append('installed normal observations differ')
    return {'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
            'observations': len(expected), 'installed_module_checked': installed_manim,
            'errors': errors}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('source_root', type=Path)
    parser.add_argument('--installed-manim', action='store_true')
    args = parser.parse_args()
    result = qualify(args.source_root, installed_manim=args.installed_manim)
    print(json.dumps(result, indent=2))
    sys.exit(bool(result['errors']))
