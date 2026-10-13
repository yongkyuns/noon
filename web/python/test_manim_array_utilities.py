import unittest
try:
    import numpy as np
except ImportError:
    np = None
import _manim_iterables as helpers
import noon


class ArrayUtilities(unittest.TestCase):
    @unittest.skipIf(np is None, "NumPy array contracts run in the pinned utility qualification lane")
    def test_shape_dtype_aliasing_and_empty_array_rules(self):
        values = np.array([[1, 2], [3, 4]], dtype=np.int64)
        for function in [helpers.resize_array, helpers.resize_preserving_order, helpers.resize_with_interpolation]:
            self.assertIs(function(values, 2), values)
        np.testing.assert_array_equal(helpers.resize_array(values, 3), [[1, 2], [3, 4], [1, 2]])
        np.testing.assert_array_equal(helpers.resize_preserving_order(values, 3), [[1, 2], [1, 2], [3, 4]])
        interpolated = helpers.resize_with_interpolation(values, 3)
        self.assertEqual(interpolated.dtype, np.float64)
        np.testing.assert_array_equal(interpolated, [[1, 2], [2, 3], [3, 4]])
        empty = helpers.resize_preserving_order(np.empty((0, 2), dtype=np.int64), 3)
        self.assertEqual(empty.shape, (3, 2))
        self.assertEqual(empty.dtype, np.float64)
        with self.assertRaises(IndexError): helpers.resize_with_interpolation(np.array([]), 1)

    @unittest.skipIf(np is None, "NumPy array contracts run in the pinned utility qualification lane")
    def test_public_stretch_preserves_the_pinned_warning_and_copy(self):
        values = np.array([[1, 2], [3, 4]])
        np.testing.assert_array_equal(noon.stretch_array_to_length(values, 3), [[1, 2], [1, 2], [3, 4]])
        self.assertFalse(np.shares_memory(noon.stretch_array_to_length(values, 2), values))
        with self.assertRaises(Warning): noon.stretch_array_to_length(values, 1)
        with self.assertRaises(IndexError): noon.stretch_array_to_length(np.empty((0, 2)), 1)

    def test_public_interpolation_keeps_operator_order_and_extrapolates(self):
        self.assertEqual(noon.interpolate(0, 10, -0.5), -5)
        self.assertEqual(noon.integer_interpolate(0, 10, -0.5), (0, 0))
        self.assertEqual(noon.integer_interpolate(0, 10, 1), (9, 1.0))
        events = []
        class Operand:
            def __init__(self, name): self.name = name
            def __rsub__(self, other): events.append(("rsub", self.name, other)); return Operand("complement")
            def __mul__(self, other): events.append(("mul", self.name, other.name)); return Operand("product")
            def __add__(self, other): events.append(("add", self.name, other.name)); return Operand("sum")
        result = noon.interpolate(Operand("start"), Operand("end"), Operand("alpha"))
        self.assertEqual(result.name, "sum")
        self.assertEqual(events, [("rsub", "alpha", 1), ("mul", "complement", "start"),
                                  ("mul", "alpha", "end"), ("add", "product", "product")])

    @unittest.skipIf(np is None, "NumPy interpolation runs in the pinned utility lane")
    def test_interpolation_broadcasts_without_mutating_or_aliasing_inputs(self):
        start = np.array([0, 1, 2], dtype=np.float32)
        end = np.array([4, 5, 6], dtype=np.float32)
        values = noon.interpolate(start, end, np.array([[0.0], [0.5], [1.0]], dtype=np.float32))
        self.assertEqual(values.dtype, np.float32)
        np.testing.assert_array_equal(values, [[0, 1, 2], [2, 3, 4], [4, 5, 6]])
        self.assertFalse(np.shares_memory(values, start))
        np.testing.assert_array_equal(start, [0, 1, 2])
        self.assertIs(type(noon.inverse_interpolate(0, 10, 2.5)), np.float64)
        self.assertEqual(noon.match_interpolate(0, 100, 0, 10, 2.5), 25)
        with self.assertWarns(RuntimeWarning):
            self.assertTrue(np.isnan(noon.inverse_interpolate(1, 1, 1)))

    def test_callback_order_lazy_consumption_and_nested_hashing(self):
        calls = []
        def property_func(item):
            calls.append(item)
            return item
        self.assertEqual(helpers.batch_by_property(iter([None, None, 1, None]), property_func),
                         [([None, None], None), ([1], 1), ([None], None)])
        self.assertEqual(calls, [None, None, 1, None])
        values = iter([1, 1, 2])
        generator = helpers.uniq_chain(values)
        self.assertEqual(next(generator), 1)
        self.assertEqual(list(values), [1, 2])
        self.assertEqual(helpers.hash_obj({"a": [1, 2], "b": {3, 4}}),
                         helpers.hash_obj({"b": {4, 3}, "a": [1, 2]}))
        with self.assertRaises(TypeError): helpers.hash_obj(bytearray(b"x"))


if __name__ == "__main__": unittest.main()
