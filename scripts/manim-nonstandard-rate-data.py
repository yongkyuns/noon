"""Compare explicit user data with complete pinned nonstandard rate functions."""
import argparse
from decimal import Decimal
from fractions import Fraction
from functools import wraps
import hashlib
import inspect
import json
import math
from pathlib import Path
import runpy
import sys
import warnings

import numpy as np
from pinned_python_helpers import load_helpers

REJECTED_NAMES = ('there_and_back_with_pause', 'running_start', 'wiggle', 'exponential_decay')
NAMES = ('smooth', 'rush_into', 'rush_from', 'there_and_back', *REJECTED_NAMES)
normalize = runpy.run_path(str(Path(__file__).with_name('manim-simple-functions.py')),
                          run_name='_rate_data_normalization')['normalize']


def observations(namespace):
    result = []
    values = [-math.inf, -1, *[i / 256 for i in range(257)], 2, math.inf, math.nan,
              Fraction(1, 4), Decimal('0.25'), True, np.float32(0.25),
              np.float64(0.25), '0.5', np.array([0.25]), np.array([0.25, 0.5])]

    def record(name, *args):
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter('always')
            try:
                value = namespace[name](*args)
                outcome = ['value', normalize(value), type(value).__name__]
            except Exception as error:
                outcome = ['error', type(error).__name__]
        result.append([name, normalize(args), outcome,
                       [type(w.message).__name__ for w in caught]])

    for name in NAMES:
        result.append([name, 'signature', str(inspect.signature(namespace[name]))])
        for value in values:
            record(name, value)
        for value in (-1, 0, 0.25, 0.5, 1, 2, math.nan, Fraction(1, 4),
                      np.float32(0.25), np.array([0.25])):
            for parameter in (-1, 0, 0.1, 0.5, 1, 2, math.inf, math.nan,
                              Fraction(1, 3), Decimal('0.2'), 'bad', np.float32(0.2)):
                record(name, value, parameter)
    return result


def qualify(source_root, *, installed_manim=False):
    source = source_root / 'manim/utils/rate_functions.py'
    simple_source = source_root / 'manim/utils/simple_functions.py'
    sigmoid = load_helpers(simple_source, {'sigmoid'}, {'np': np})['sigmoid']
    original = load_helpers(source, set(NAMES) | {'unit_interval', 'zero', 'smooth',
                                                'there_and_back'},
                            {'wraps': wraps, 'np': np, 'sigmoid': sigmoid})
    expected = observations(original)
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'web/python'))
    import noon
    import _manim_rate_functions as rates
    actual = observations(vars(rates))
    errors = [] if actual == expected else ['explicit rate-data observations differ']
    differences = [[i, a, b] for i, (a, b) in enumerate(zip(expected, actual)) if a != b]
    for name in NAMES:
        function = getattr(noon, name)
        if name not in noon.__all__ or function is not getattr(noon.rate_functions, name):
            errors.append(f'public callable identity differs: {name}')
        if name in REJECTED_NAMES:
            try:
                rates.easing_from_rate_func(function)
            except NotImplementedError:
                pass
            else:
                errors.append(f'unimplemented playback admitted: {name}')
        elif rates.easing_from_rate_func(function) != name:
            errors.append(f'existing Rust playback identity differs: {name}')
    if installed_manim:
        import manim
        from manim.utils import rate_functions
        if Path(rate_functions.__file__).read_bytes() != source.read_bytes():
            raise ValueError('installed complete rate module differs from pin')
        if observations(vars(rate_functions)) != expected:
            errors.append('installed rate-data observations differ')
        for name in NAMES:
            if getattr(manim, name) is not getattr(rate_functions, name):
                errors.append(f'installed public identity differs: {name}')
    return dict(source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),
                functions=len(NAMES), observations=len(expected),
                installed_module_checked=installed_manim, errors=errors,
                differences=differences)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('source_root', type=Path)
    parser.add_argument('--installed-manim', action='store_true')
    args = parser.parse_args()
    result = qualify(args.source_root, installed_manim=args.installed_manim)
    print(json.dumps(result, indent=2))
    sys.exit(bool(result['errors']))
