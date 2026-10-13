import unittest
import noon


class IterableUtilities(unittest.TestCase):
    def test_cyclic_adjacency_and_lazy_zip_type(self):
        pairs = noon.adjacent_pairs([1, 2, 3])
        self.assertIsInstance(pairs, zip)
        self.assertEqual(list(pairs), [(1, 2), (2, 3), (3, 1)])
        self.assertEqual(list(noon.adjacent_n_tuples([1, 2], 4)), [(1, 2, 1, 1), (2, 1, 2, 2)])
        self.assertEqual(list(noon.adjacent_n_tuples([], 2)), [])

    def test_order_duplicates_identity_and_iterator_consumption(self):
        self.assertEqual(noon.list_update([1, 2, 3], [2, 4, 4]), [1, 3, 2, 4, 4])
        self.assertEqual(noon.list_update([1, 2, 3], iter([2, 4, 4])), [1, 2, 3])
        self.assertEqual(noon.remove_list_redundancies([1, 2, 1, 3, 2]), [1, 3, 2])
        marker = object()
        expanded, _ = noon.make_even([marker], [1, 2, 3])
        self.assertTrue(all(value is marker for value in expanded))
        self.assertEqual(noon.make_even([1, 2], [3, 4, 5, 6, 7])[0], [1, 1, 1, 2, 2])
        self.assertEqual(noon.make_even_by_cycling([1, 2], [3, 4, 5, 6, 7])[0], [1, 2, 1, 2, 1])
        self.assertEqual(noon.make_even([], []), ([], []))
        with self.assertRaises(IndexError): noon.make_even([], [1])
        with self.assertRaises(StopIteration): noon.make_even_by_cycling([], [1])

    def test_falsey_filtering_and_scalar_string_iterable_admission(self):
        self.assertEqual(noon.remove_nones([None, 0, False, [], "", 1, "a"]), [1, "a"])
        self.assertEqual(noon.listify("ab"), ["ab"])
        self.assertEqual(noon.tuplify("ab"), ("ab",))
        self.assertEqual(noon.listify(iter([1, 2])), [1, 2])
        self.assertEqual(noon.tuplify(None), (None,))
        self.assertTrue(noon.all_elements_are_instances([], int))
        self.assertFalse(noon.all_elements_are_instances([1, "x"], int))
        self.assertEqual(noon.concatenate_lists(iter([1, 2]), [3]), [1, 2, 3])


if __name__ == "__main__": unittest.main()
