"""Address-model and authoring-contract tests, not renderer qualification.

Extract only the three pure address functions. Do not mock Noon or introduce
another implementation of scene/runtime semantics to make the scene 'pass'.
"""
import ast
from collections import Counter
from math import gcd
from pathlib import Path
import unittest

SOURCE = Path(__file__).resolve().parents[1] / 'web/python/examples/cuda_transpose_banks.py'
TEXT = SOURCE.read_text(encoding='utf-8')
TREE = ast.parse(TEXT, filename=str(SOURCE))
NAMES = {'column_addresses', 'read_service_batches', 'bank_loads'}
FUNCTIONS = [n for n in TREE.body if isinstance(n, ast.FunctionDef) and n.name in NAMES]
NAMESPACE = {}
exec(compile(ast.Module(body=FUNCTIONS, type_ignores=[]), str(SOURCE), 'exec'), NAMESPACE)
column_addresses = NAMESPACE['column_addresses']
read_service_batches = NAMESPACE['read_service_batches']
bank_loads = NAMESPACE['bank_loads']


class AddressModelTests(unittest.TestCase):
    def test_toy_padding(self):
        self.assertEqual(column_addresses(8, 8), tuple(range(0, 64, 8)))
        self.assertEqual(column_addresses(9, 8), tuple(range(0, 72, 9)))
        self.assertEqual(read_service_batches(column_addresses(8, 8), 8),
                         tuple((lane,) for lane in range(8)))
        self.assertEqual(read_service_batches(column_addresses(9, 8), 8),
                         (tuple(range(8)),))

    def test_cuda_parameter_sweep(self):
        for stride, used, rounds in ((32, 1, 32), (33, 32, 1), (34, 16, 2),
                                     (36, 8, 4), (48, 2, 16)):
            with self.subTest(stride=stride):
                loads = bank_loads(stride, 32)
                self.assertEqual(sum(n > 0 for n in loads), used)
                self.assertEqual(sum(loads), 32)
                self.assertEqual(max(loads), rounds)
                self.assertEqual(len(read_service_batches(column_addresses(stride, 32), 32)),
                                 rounds)

    def test_all_toy_and_cuda_column_offsets(self):
        # For B consecutive lanes and distinct words, rounds = gcd(stride, B).
        for banks in (8, 32):
            for stride in range(1, 130):
                for col in range(min(stride, banks)):
                    with self.subTest(banks=banks, stride=stride, column=col):
                        addresses = column_addresses(stride, banks, col)
                        batches = read_service_batches(addresses, banks)
                        self.assertEqual(len(batches), gcd(stride, banks))
                        self.assertEqual(sorted(lane for b in batches for lane in b),
                                         list(range(banks)))
                        for batch in batches:
                            bank_ids = [addresses[lane] % banks for lane in batch]
                            self.assertEqual(len(bank_ids), len(set(bank_ids)))

    def test_broadcast_is_not_a_bank_conflict(self):
        self.assertEqual(read_service_batches((17,) * 32, 32), (tuple(range(32)),))
        # Two addresses in bank 0; two broadcast groups, not four rounds.
        self.assertEqual(read_service_batches((0, 0, 32, 32), 32), ((0, 1), (2, 3)))
        self.assertEqual(read_service_batches((0, 0, 1, 1), 32), ((0, 1, 2, 3),))

    def test_empty_request(self):
        self.assertEqual(read_service_batches((), 32), ())

    def test_invalid_inputs(self):
        for args in ((0, 8), (-1, 8), (8, 0), (8, -1), (8, 8, -1), (8, 8, 8)):
            with self.assertRaises(ValueError):
                column_addresses(*args)
        for args in ((True, 8), (8, False), (8.0, 8), (8, 8, 0.0)):
            with self.assertRaises(TypeError):
                column_addresses(*args)
        for banks in (0, -1, True, 3.5):
            with self.assertRaises(ValueError):
                read_service_batches((0,), banks)
        for address in (-1, 1.5, True):
            with self.assertRaises(ValueError):
                read_service_batches((address,), 8)

    def test_padding_preserves_logical_data_and_transpose(self):
        for n in (8, 32):
            data = [[row * n + col for col in range(n)] for row in range(n)]
            expected = [[data[row][col] for row in range(n)] for col in range(n)]
            for stride in (n, n + 1):
                memory = [None] * (n * stride)
                for row in range(n):
                    for col in range(n):
                        memory[row * stride + col] = data[row][col]
                actual = [[memory[row * stride + col] for row in range(n)]
                          for col in range(n)]
                self.assertEqual(actual, expected)
                self.assertEqual(Counter(v for v in memory if v is not None),
                                 Counter(range(n * n)))
                self.assertEqual(sum(v is None for v in memory), n * (stride - n))


class AuthoringContractTests(unittest.TestCase):
    def test_only_manim_compatible_import(self):
        imports = [n for n in ast.walk(TREE) if isinstance(n, (ast.Import, ast.ImportFrom))]
        self.assertEqual(len(imports), 1)
        self.assertIsInstance(imports[0], ast.ImportFrom)
        self.assertEqual(imports[0].module, 'noon')
        self.assertEqual([n.name for n in imports[0].names], ['*'])
        ast.parse(TEXT.replace('from noon import *', 'from manim import *', 1))

    def test_one_normal_scene_no_host_callbacks(self):
        scenes = [n for n in TREE.body if isinstance(n, ast.ClassDef)]
        self.assertEqual([n.name for n in scenes], ['CudaTransposeBanks'])
        self.assertEqual([ast.unparse(base) for base in scenes[0].bases], ['Scene'])
        forbidden = {'always_redraw', 'add_updater', 'UpdateFromFunc', 'setInterval',
                     'requestAnimationFrame', 'eval', 'exec', 'open'}
        calls = {n.func.id if isinstance(n.func, ast.Name) else n.func.attr
                 for n in ast.walk(TREE) if isinstance(n, ast.Call)
                 and isinstance(n.func, (ast.Name, ast.Attribute))}
        self.assertFalse(calls & forbidden)
        self.assertFalse(any(isinstance(n, (ast.While, ast.AsyncFunctionDef, ast.Await))
                             for n in ast.walk(TREE)))

    def test_explicit_model_limits_and_attribution(self):
        for phrase in ('Toy model: 8 lanes', 'CUDA uses 32 banks',
                       'not measured GPU time', 'do not imply a 32x kernel speedup',
                       '128 bytes per tile (+3.125%)', 'ourarash/cuda-algorithm-lab'):
            self.assertIn(phrase, TEXT)
        self.assertEqual(len(FUNCTIONS), 3)
        # Both packet demonstrations call the SAME function and round duration.
        self.assertIn('self.transfer_column(8,', TEXT)
        self.assertIn('self.transfer_column(9,', TEXT)
        self.assertEqual(TEXT.count('run_time=ROUND_TIME'), 1)


if __name__ == '__main__':
    unittest.main(verbosity=2)
