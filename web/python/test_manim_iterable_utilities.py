import unittest
import noon


class IterableUtilities(unittest.TestCase):
    def test_batches_preserve_adjacent_order_identity_and_callback_failure(self):
        first, second = object(), object()
        calls = []
        def classify(value):
            calls.append(value)
            return None if value is first else 2
        batches = noon.batch_by_property([first, first, second, first], classify)
        self.assertEqual(batches, [([first, first], None), ([second], 2), ([first], None)])
        self.assertEqual(calls, [first, first, second, first])
        def fail(value): raise ValueError("user callback")
        with self.assertRaisesRegex(ValueError, "user callback"):
            noon.batch_by_property([first], fail)

    def test_unique_chain_is_lazy_and_preserves_first_equal_value(self):
        events = []
        def source():
            for value in [1, 1, 2]:
                events.append(value)
                yield value
        result = noon.uniq_chain(source(), [2, 3])
        self.assertEqual(events, [])
        self.assertEqual(next(result), 1)
        self.assertEqual(events, [1])
        self.assertEqual(list(result), [2, 3])
        self.assertEqual(events, [1, 1, 2])
        first, second = [frozenset({1}), frozenset({1})]
        self.assertIs(next(noon.uniq_chain([first], [second])), first)
        with self.assertRaises(TypeError): list(noon.uniq_chain([[1]]))

    def test_nested_hashing_preserves_container_order_rules_and_user_failures(self):
        value = {"a": [1, {2, 3}], "b": (4, 5)}
        reordered = {"b": [4, 5], "a": (1, {3, 2})}
        self.assertEqual(noon.hash_obj(value), noon.hash_obj(reordered))
        self.assertEqual(noon.hash_obj([1, 2]), hash((hash(1), hash(2))))
        self.assertNotEqual(noon.hash_obj([1, 2]), noon.hash_obj([2, 1]))
        class BrokenHash:
            def __hash__(self): raise ValueError("user hash")
        with self.assertRaisesRegex(ValueError, "user hash"):
            noon.hash_obj([BrokenHash()])
        with self.assertRaises(TypeError): noon.hash_obj(bytearray(b"x"))
        cyclic = []; cyclic.append(cyclic)
        with self.assertRaises(RecursionError): noon.hash_obj(cyclic)

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
