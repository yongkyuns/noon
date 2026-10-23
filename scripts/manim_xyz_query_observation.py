"""Shared test authoring and observations for the pinned full-XYZ query corpus.

Geometry, traversal and bounds are always read from the supplied engine.
Only the weighted reducer is a caller-owned Python callback.
"""
import numpy as np
from manim_depth_affine_observation import POINTS
DIRECTIONS = ([0.0, 0.0, 0.0], [1.0, -1.0, 1.0], [-1.0, 1.0, -1.0])

def specs():
    return [dict(kind=kind, rows=rows, angle=angle) for kind in ('Mobject', 'VMobject', 'Group', 'VGroup') for rows in (0, 1, 4, 5, 8) for angle in (0.0, 0.37)]

def normalized(value):
    if value is None:
        return dict(type='NoneType', value=None)
    array = np.asarray(value)
    return dict(type=type(value).__name__, dtype=str(array.dtype), shape=list(array.shape), value=array.tolist())

def prepare_root(m, spec):
    kind = spec['kind']
    if kind in ('Group', 'VGroup'):
        a = m.VMobject().set_points(POINTS[:4].copy())
        b = m.VMobject().set_points(POINTS[4:].copy())
        nested = m.VGroup(a, b)
        root = getattr(m, kind)(nested, a, m.VMobject())
    else:
        root = getattr(m, kind)()
    root.set_points(POINTS[:spec['rows']].copy())
    root.shift(np.array([1.5, -2.5, 3.125]))
    if spec['angle']:
        root.rotate(spec['angle'], about_point=np.array([0.25, -0.75, 0.0]))
    return root

def collect_queries(root, spec):
    observations = []

    def capture(method, args=(), kwargs=None):
        kwargs = {} if kwargs is None else kwargs
        try:
            result = dict(kind='returned', result=normalized(getattr(root, method)(*args, **kwargs)))
        except Exception as error:
            result = dict(kind='raised', type=type(error).__name__, message=str(error))
        observations.append(dict(method=method, arguments=list(args), keywords=kwargs, output=result))
    for method in ('get_all_points', 'get_points_defining_boundary', 'get_center', 'get_zenith', 'get_nadir'):
        capture(method)
    for direction in DIRECTIONS:
        capture('get_critical_point', (direction,))
        capture('get_z', (direction,))
        for dim in range(3):
            capture('get_coord', (dim, direction))
    for dim in range(3):
        for key in (-1.0, 0.0, 1.0):
            capture('get_extremum_along_dim', kwargs=dict(dim=dim, key=key))
        for reducer in ('min', 'max', 'weighted'):
            trace = []

            def reduce(values):
                trace.append(normalized(values))
                if reducer == 'min':
                    return min(values)
                if reducer == 'max':
                    return max(values)
                return float(sum(((index + 1) * value for index, value in enumerate(values))))
            try:
                result = dict(kind='returned', result=normalized(root.reduce_across_dimension(reduce, dim)))
            except Exception as error:
                result = dict(kind='raised', type=type(error).__name__, message=str(error))
            observations.append(dict(method='reduce_across_dimension', dimension=dim, reducer=reducer, trace=trace, output=result))
    return dict(spec=spec, observations=observations)
