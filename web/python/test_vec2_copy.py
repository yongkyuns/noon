import copy
import pickle
import unittest

from noon import Vec2


class Vec2CopyTests(unittest.TestCase):
    def test_copy_and_deepcopy_keep_immutable_value_atomic(self) -> None:
        value = Vec2(1.0, 2.0)

        self.assertIs(copy.copy(value), value)
        self.assertIs(copy.deepcopy(value), value)
        self.assertEqual(copy.deepcopy(value), Vec2(1.0, 2.0))

    def test_pickle_reconstructs_vec2_with_scalar_arguments(self) -> None:
        value = Vec2(-1.25, 3.5)
        restored = pickle.loads(pickle.dumps(value))

        self.assertIsInstance(restored, Vec2)
        self.assertEqual(restored, value)
        self.assertEqual((restored.x, restored.y), (-1.25, 3.5))


if __name__ == "__main__":
    unittest.main()
