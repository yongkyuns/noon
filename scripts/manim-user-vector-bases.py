"""Compare vector algebra with unchanged pinned ManimCE definitions."""
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
                          run_name='_vector_base_normalization')['normalize']
NAMES = {'norm_squared', 'cross', 'z_to_vector'}


def observations(namespace):
    result = [[name, str(inspect.signature(namespace[name]))] for name in sorted(NAMES)]

    def record(name, *args):
        before = normalize(args)
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter('always')
            try:
                value = ['value', normalize(namespace[name](*args))]
            except Exception as error:
                value = ['error', type(error).__name__]
        result.append([name, before, value, normalize(args),
                       [[type(w.message).__name__, str(w.message)] for w in caught]])

    vectors = [[0, 0, 0], [1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, 0, 1],
               [0, 0, -1], [1, 2, 3], [1e-200, 0, 0], [1e200, 2, -3]]
    for dtype in (np.int64, np.float32, np.float64, np.complex128):
        for v in vectors:
            if dtype is np.int64 and max(abs(x) for x in v) > 1e18:
                continue
            with warnings.catch_warnings():
                warnings.simplefilter('ignore')
                array = np.array(v, dtype=dtype)
            for vector in (array, array.tolist(), tuple(array.tolist())):
                record('norm_squared', vector)
                record('z_to_vector', vector)
                for other in vectors[:7]:
                    record('cross', vector, other)
            array.flags.writeable = False
            record('z_to_vector', array)
    for value in ([], [1], [1, 2], [1, 2, 3, 4], [[1, 2, 3]], None, 'x',
                  [float('nan'), 0, 0], [float('inf'), 1, 0]):
        record('norm_squared', value)
        record('z_to_vector', value)
        record('cross', value, [1, 2, 3])
        record('cross', [1, 2, 3], value)
    return result


def qualify(source_root, *, installed_manim=False):
    source = source_root / 'manim/utils/space_ops.py'
    original = load_helpers(source, NAMES | {'normalize'},
                            {'np': np, 'RIGHT': np.array([1., 0., 0.]),
                             'UP': np.array([0., 1., 0.])})
    expected = observations(original)
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'web/python'))
    import _manim_user_vector_bases as helpers
    import noon
    errors = [] if observations(vars(helpers)) == expected else ['vector base observations differ']
    for name in {'z_to_vector'}:
        if getattr(noon, name) is not getattr(helpers, name):
            errors.append(f'public identity {name} differs')
    if installed_manim:
        from manim.utils import space_ops
        if Path(space_ops.__file__).read_bytes() != source.read_bytes():
            raise ValueError('installed complete source differs from pin')
        if observations(vars(space_ops)) != expected:
            errors.append('installed vector base observations differ')
    return {'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
            'functions': len(NAMES), 'observations': len(expected),
            'installed_module_checked': installed_manim, 'errors': errors}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('source_root', type=Path)
    parser.add_argument('--installed-manim', action='store_true')
    args = parser.parse_args()
    result = qualify(args.source_root, installed_manim=args.installed_manim)
    print(json.dumps(result, indent=2))
    sys.exit(bool(result['errors']))
