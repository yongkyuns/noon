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
