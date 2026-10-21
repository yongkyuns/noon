from pathlib import Path
import argparse
import hashlib
import inspect
import itertools
import json
import warnings
import manim as m
import numpy as np
assert m.__version__ == '0.21.0'
parser = argparse.ArgumentParser(description='Observe pure point-list insertion and public tuple-hook contracts')
parser.add_argument('output', type=Path)
args = parser.parse_args()
rows = []

def observe(spec):
    value = m.VMobject()
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
            'ownPointsUnchanged': value.points.shape == (0, 3),
            'warnings': [str(w.message) for w in caught], 'tupleQuery': query_observation}

for count, n in itertools.product([0, 1, 2, 3, 4, 5, 8], [-2, -1, 0, 1, 2, 5, 'float', 'bool', 'numpy-int']):
    rows.append(observe({'rows': count, 'n': n, 'stride': 4, 'dimensions': 3, 'dtype': 'float64', 'container': 'array'}))
for count, stride, dimensions, dtype, container in itertools.product([1, 4, 8], [2, 3, 5], [2, 3], ['float32', 'int64'], ['array', 'list']):
    rows.append(observe({'rows': count, 'n': 2, 'stride': stride, 'dimensions': dimensions, 'dtype': dtype, 'container': container}))
record = {'schemaVersion': 1, 'manimVersion': m.__version__, 'methodSource': inspect.getsource(m.VMobject.insert_n_curves_to_point_list),
          'methodSourceSha256': hashlib.sha256(inspect.getsource(m.VMobject.insert_n_curves_to_point_list).encode()).hexdigest(),
          'probeSha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), 'cases': rows,
          'scope': 'Actual pinned CE value/coercion/mutation contract for the missing pure point-list method; no Noon implementation or support claim.'}
assert all(row['inputUnchanged'] and row['ownPointsUnchanged'] for row in rows)
args.output.write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps({'cases': len(rows), 'returns': sum(r['output']['kind']=='returned' for r in rows),
                  'raises': sum(r['output']['kind']=='raised' for r in rows),
                  'methodSourceSha256': record['methodSourceSha256'], 'allInputsAndOwnPointsUnchanged': True}))
