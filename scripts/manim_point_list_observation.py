"""Public caller-owned value observations and a pinned, scene-free CE oracle."""
import __future__
from types import SimpleNamespace
import warnings

import numpy as np


def pinned_point_list_oracle(witness):
    """Run the six pinned pure functions on this interpreter's NumPy platform.

    This test-only container has no scene, runtime, rendering or mutation engine.
    Native integer-list coercion belongs to NumPy, including on wasm32.
    """
    scope = {"np": np, "SUBDIVISION_MATRICES": [
        {int(key): np.array(value["values"], dtype=value["dtype"])
         for key, value in table.items()}
        for table in witness["subdivisionTables"]
    ]}
    for name, source in witness["sources"].items():
        exec(compile(source, "<pinned-ce-" + name + ">", "exec",
                     flags=__future__.annotations.compiler_flag), scope)

    class PurePointList:
        n_points_per_cubic_curve = 4

        def __init__(self):
            self.points = np.zeros((0, 3))

        def get_num_points(self):
            return len(self.points)

    for name in ("insert_n_curves_to_point_list",
                 "get_cubic_bezier_tuples_from_points",
                 "gen_cubic_bezier_tuples_from_points"):
        setattr(PurePointList, name, scope[name])
    return SimpleNamespace(VMobject=PurePointList)


def observe_point_list(module, spec):
    value = module.VMobject()
    value.n_points_per_cubic_curve = spec['stride']
    points = np.arange(spec['rows'] * spec['dimensions'], dtype=spec['dtype']).reshape(spec['rows'], spec['dimensions'])
    if spec['container'] == 'list':
        points = points.tolist()
    before = np.array(points, copy=True)
    n = {'float': 1.5, 'bool': True, 'numpy-int': np.int64(2)}.get(spec['n'], spec['n'])
    query_observation = []
    original_query = value.get_cubic_bezier_tuples_from_points

    def capture_query(argument):
        assert argument is points
        try:
            tuples = original_query(argument)
        except Exception as error:
            query_observation.append({'kind': 'raised', 'type': type(error).__name__, 'message': str(error)})
            raise
        query_observation.append({'kind': 'returned', 'shape': list(tuples.shape),
                                  'dtype': str(tuples.dtype), 'values': tuples.tolist()})
        return tuples

    value.get_cubic_bezier_tuples_from_points = capture_query
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter('always')
        try:
            result = value.insert_n_curves_to_point_list(n, points)
            output = {'kind': 'returned', 'type': type(result).__name__, 'shape': list(result.shape),
                      'dtype': str(result.dtype), 'points': result.tolist(),
                      'sharesInput': bool(np.shares_memory(result, points)), 'isInput': result is points}
        except Exception as error:
            output = {'kind': 'raised', 'type': type(error).__name__, 'message': str(error)}
    return {'spec': spec, 'output': output,
            'inputUnchanged': bool(np.array_equal(np.asarray(points), before)),
            'ownPointsUnchanged': value.get_num_points() == 0,
            'warnings': [str(w.message) for w in caught], 'tupleQuery': query_observation}


def assert_point_list_observation(actual, expected, path="observation"):
    """Keep types, dtype, errors, shapes, ownership and hook order exact."""
    if isinstance(expected, dict):
        assert isinstance(actual, dict) and actual.keys() == expected.keys(), path
        for key in expected:
            assert_point_list_observation(actual[key], expected[key], path + "." + key)
    elif isinstance(expected, list):
        assert isinstance(actual, list) and len(actual) == len(expected), path
        for index, item in enumerate(expected):
            assert_point_list_observation(actual[index], item, f"{path}[{index}]")
    elif isinstance(expected, float):
        assert isinstance(actual, (float, int)) and not isinstance(actual, bool), path
        np.testing.assert_allclose(actual, expected, rtol=1e-12, atol=1e-12, err_msg=path)
    else:
        assert type(actual) is type(expected) and actual == expected, (path, actual, expected)
