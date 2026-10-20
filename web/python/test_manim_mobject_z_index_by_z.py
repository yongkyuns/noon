import unittest

import noon


class ManimMobjectZIndexByZTests(unittest.TestCase):
    def test_planar_mobject_uses_zero_z_and_returns_itself(self):
        mobject = object.__new__(noon.Mobject)
        calls = []

        def set_z_index(value, family=True):
            calls.append((value, family))
            return mobject

        mobject.set_z_index = set_z_index

        self.assertIs(mobject.set_z_index_by_z_Point3D(), mobject)
        self.assertEqual(calls, [(0.0, True)])


if __name__ == "__main__":
    unittest.main()
