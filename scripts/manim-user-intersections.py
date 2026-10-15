"""Compare explicit user-array intersections with unchanged pinned definitions."""
import argparse
from collections.abc import Sequence
from decimal import Decimal
from fractions import Fraction
from functools import wraps
import hashlib
import inspect
import json
from pathlib import Path
import runpy
import sys
import warnings
import numpy as np
from pinned_python_helpers import load_helpers

normalize = runpy.run_path(str(Path(__file__).with_name('manim-simple-functions.py')), run_name='_intersection_normalization')['normalize']
NAMES = ('line_intersection', 'find_intersection')


def observations(namespace):
    result = [[name, str(inspect.signature(namespace[name]))] for name in NAMES]
    def record(name, *args):
        before = normalize(args)
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter('always')
            try:
                value = namespace[name](*args)
                outcome = ['value', type(value).__name__, normalize(value)]
            except Exception as error:
                outcome = ['error', type(error).__name__, str(error)]
        result.append([name, outcome, [type(x.message).__name__ for x in caught], before == normalize(args)])
    lines = [[], [[0, 0], [1, 1]], [[0, 0, 0], [2, 2, 0]], [[0, 2, 0], [2, 0, 0]],
             [[0, 0, 1], [2, 2, 0]], [[0, 0, 0], [0, 0, 0]], [[float('nan'), 0, 0], [2, 2, 0]],
             np.array([[0, 0, 0], [2, 2, 0]], dtype=np.float32), np.array([[0, 2, 0], [2, 0, 0]], dtype=np.int64),
             np.ones((2, 4)), np.zeros((3, 3)), np.ones((1, 3)), np.array([['x', 0, 0], [2, 2, 0]],dtype=object)]
    for first in lines:
        for second in lines: record('line_intersection', first, second)
    origin = np.array([[0.,0.,0.], [1.,0.,0.]])
    x_direction = np.array([[1.,0.,0.], [1.,0.,0.]])
    other = np.array([[1.,2.,3.], [2.,2.,0.]])
    y_direction = np.array([[0.,1.,0.], [0.,1.,0.]])
    batches = [(origin, x_direction, other, y_direction), (origin, x_direction, other, x_direction),
               (origin[:1], x_direction, other, y_direction), ([], [], [], []),
               (origin.tolist(),x_direction.tolist(),other.tolist(),y_direction.tolist()),
               (origin.astype(np.float32),x_direction.astype(np.float32),other.astype(np.float32),y_direction.astype(np.float32)),
               (origin.astype(np.int64),x_direction.astype(np.int64),other.astype(np.int64),y_direction.astype(np.int64)),
               (np.full((1,3),np.nan),x_direction[:1],other[:1],y_direction[:1])]
    for batch in batches:
        record('find_intersection', *batch)
        for threshold in [-1., 0., 1e-5, 1., np.float32(1e-5), Fraction(1,10), Decimal('0.1'), float('nan'), 'x']:
            record('find_intersection', *batch, threshold)
    for short in [False, True]:
        events = []
        def stream(index, values):
            for item in values:
                events.append(index)
                yield item
        args = [stream(i, value[:1] if short and i==2 else value) for i,value in enumerate((origin,x_direction,other,y_direction))]
        try: outcome = ['value',normalize(namespace['find_intersection'](*args))]
        except Exception as error: outcome = ['error',type(error).__name__,str(error)]
        result.append(['strict_iteration', short, events, outcome])
    return result


def qualify(source_root, *, installed_manim=False):
    source = source_root/'manim/utils/space_ops.py'
    original = load_helpers(source, set(NAMES)|{'cross'}, {'np':np,'Sequence':Sequence})
    expected = observations(original)
    sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'web/python'))
    import _manim_user_vectors as helpers
    import noon
    errors = [] if observations(vars(helpers)) == expected else ['user intersection observations differ']
    for name in NAMES:
        if getattr(noon,name) is not getattr(helpers,name): errors.append(f'public identity {name} differs')
    if installed_manim:
        from manim.utils import space_ops
        if Path(space_ops.__file__).read_bytes()!=source.read_bytes(): raise ValueError('installed complete space-ops module differs from pin')
        if observations(vars(space_ops)) != expected: errors.append('installed intersection observations differ')
    return {'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'functions':len(NAMES),
            'observations':len(expected),'installed_module_checked':installed_manim,'errors':errors}


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('source_root',type=Path);parser.add_argument('--installed-manim',action='store_true')
    args=parser.parse_args();result=qualify(args.source_root,installed_manim=args.installed_manim);print(json.dumps(result,indent=2));sys.exit(bool(result['errors']))
