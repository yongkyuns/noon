"""Caller-owned array protocol, independent of persistent scene mutation."""
from pathlib import Path
from types import SimpleNamespace
import hashlib
import json
import sys
import unittest
import warnings

try:
    import numpy as np
except ImportError:
    np = None

import _manim_compat as compat
import _manim_namespace as namespace

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = json.loads((ROOT / 'fixtures/manim-v0.21/point-list-insertion.json').read_text())
if np is not None:
    sys.path.insert(0, str(ROOT / 'scripts'))
    from manim_point_list_observation import (
        assert_point_list_observation, observe_point_list, pinned_point_list_oracle,
    )


class PointListInsertionTests(unittest.TestCase):
    @unittest.skipIf(np is None, 'NumPy runs in the pinned array-oracle lane')
    def test_exact_extracted_ce_reproduces_every_original_desktop_observation(self):
        witness = json.loads((ROOT / 'fixtures/manim-v0.21/point-list-insertion-oracle.json').read_text())
        self.assertEqual(witness['rawMethodSha256'], FIXTURE['methodSourceSha256'])
        self.assertEqual(np.dtype(int).name, 'int64', 'Original desktop fixture requires its captured integer platform')
        oracle = pinned_point_list_oracle(witness)
        for expected in FIXTURE['cases']:
            with self.subTest(spec=expected['spec']):
                assert_point_list_observation(observe_point_list(oracle, expected['spec']), expected)

    @unittest.skipIf(np is None, 'NumPy runs in the pinned array-oracle lane')
    def test_observation_comparator_preserves_metadata_and_value_failures(self):
        for field, actual, expected in [('dtype', 'int32', 'int64'),
                                        ('message', 'wrong error', 'pinned error'),
                                        ('values', [0.1], [0.10001]),
                                        ('ownership', True, False),
                                        ('shape', [4, 3], [3, 4])]:
            with self.subTest(field=field), self.assertRaises(AssertionError):
                assert_point_list_observation({field: actual}, {field: expected})

    def test_numpy_is_discovered_for_plain_list_arguments(self):
        self.assertEqual(namespace.required_packages(
            'value.insert_n_curves_to_point_list(n=2, points=[[1, 2, 3]])'), ('numpy',))

    @unittest.skipIf(np is None, 'NumPy runs in the pinned array-oracle lane')
    def test_pinned_value_error_and_public_tuple_hook_contracts(self):
        self.assertEqual(FIXTURE['manimVersion'], '0.21.0')
        self.assertEqual(len(FIXTURE['cases']), 135)
        generator = ROOT / 'scripts/manim-point-list-insertion-reference.py'
        self.assertEqual(FIXTURE['probeSha256'], hashlib.sha256(generator.read_bytes()).hexdigest())
        for expected in FIXTURE['cases']:
            spec = expected['spec']
            with self.subTest(spec=spec):
                points = np.arange(spec['rows'] * spec['dimensions'], dtype=spec['dtype']).reshape(spec['rows'], spec['dimensions'])
                if spec['container'] == 'list':
                    points = points.tolist()
                before = np.array(points, copy=True)
                calls = []
                def tuple_query(argument):
                    self.assertIs(argument, points)
                    calls.append(argument)
                    self.assertEqual(len(expected['tupleQuery']), 1)
                    result = expected['tupleQuery'][0]
                    self.assertEqual(result['kind'], 'returned')
                    return np.array(result['values'], dtype=result['dtype']).reshape(result['shape'])
                value = SimpleNamespace(n_points_per_cubic_curve=spec['stride'],
                                        get_cubic_bezier_tuples_from_points=tuple_query)
                attributes = dict(vars(value))
                n = {'float': 1.5, 'bool': True, 'numpy-int': np.int64(2)}.get(spec['n'], spec['n'])
                with warnings.catch_warnings(record=True) as caught:
                    warnings.simplefilter('always')
                    try:
                        result = compat.VMobject.insert_n_curves_to_point_list(value, n, points)
                        actual = {'kind': 'returned', 'type': type(result).__name__,
                                  'shape': list(result.shape), 'dtype': str(result.dtype),
                                  'points': result.tolist(), 'sharesInput': bool(np.shares_memory(result, points)),
                                  'isInput': result is points}
                    except Exception as error:
                        actual = {'kind': 'raised', 'type': type(error).__name__, 'message': str(error)}
                self.assertEqual(actual, expected['output'])
                self.assertEqual([str(w.message) for w in caught], expected['warnings'])
                self.assertEqual(len(calls), len(expected['tupleQuery']))
                np.testing.assert_array_equal(points, before)
                self.assertEqual(vars(value), attributes)

    @unittest.skipIf(np is None, 'NumPy runs in the pinned array-oracle lane')
    def test_count_addition_occurs_after_the_public_tuple_hook(self):
        events = []
        class Count:
            def __radd__(self, current):
                events.append(('count', current))
                return 2
        points = np.arange(12.).reshape(4, 3)
        def query(argument):
            events.append(('tuple', argument is points))
            return points.reshape(1, 4, 3)
        value = SimpleNamespace(get_cubic_bezier_tuples_from_points=query)
        result = compat.VMobject.insert_n_curves_to_point_list(value, Count(), points)
        self.assertEqual(events, [('tuple', True), ('count', 1)])
        self.assertEqual(result.shape, (8, 3))

    @unittest.skipIf(np is None, 'NumPy runs in the pinned array-oracle lane')
    def test_a_public_tuple_hook_failure_is_propagated_without_count_coercion(self):
        failure = RuntimeError('custom tuple hook failed')
        class Count:
            def __radd__(self, current):
                raise AssertionError('count touched before the failing tuple hook')
        def query(argument):
            raise failure
        value = SimpleNamespace(get_cubic_bezier_tuples_from_points=query)
        with self.assertRaises(RuntimeError) as raised:
            compat.VMobject.insert_n_curves_to_point_list(value, Count(), np.zeros((4, 3)))
        self.assertIs(raised.exception, failure)


if __name__ == '__main__':
    unittest.main()
