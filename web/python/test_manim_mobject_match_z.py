import unittest
from unittest.mock import Mock

import noon


class ManimMobjectMatchZTests(unittest.TestCase):
    def test_match_z_uses_the_z_only_shared_alignment_mask(self):
        mobject = object.__new__(noon.Mobject)
        target = object.__new__(noon.Mobject)
        mobject.move_to = Mock(return_value=mobject)

        self.assertIs(mobject.match_z(target, direction=noon.UP), mobject)
        mobject.move_to.assert_called_once_with(
            target,
            aligned_edge=noon.UP,
            coor_mask=(0.0, 0.0, 0.0),
        )

    def test_set_coord_zero_z_preserves_identity(self):
        mobject = object.__new__(noon.Mobject)
        mobject.set_z = Mock(return_value=mobject)

        self.assertIs(mobject.set_coord(0.0, 2), mobject)
        mobject.set_z.assert_called_once_with(0.0)

    def test_nonzero_z_coordinate_remains_explicitly_unsupported(self):
        mobject = object.__new__(noon.Mobject)

        with self.assertRaisesRegex(NotImplementedError, "only planar z=0"):
            mobject.set_coord(1.0, 2)

    def test_set_coord_still_rejects_noninteger_dimensions(self):
        mobject = object.__new__(noon.Mobject)

        with self.assertRaisesRegex(TypeError, "dim must be an integer"):
            mobject.set_coord(0.0, 2.0)


if __name__ == "__main__":
    unittest.main()
