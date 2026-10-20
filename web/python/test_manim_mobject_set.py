import unittest

import noon


class ManimMobjectSetTests(unittest.TestCase):
    def test_set_uses_attribute_assignment_and_returns_receiver(self):
        mobject = object.__new__(noon.Mobject)

        self.assertIs(mobject.set(), mobject)
        self.assertIs(
            mobject.set(name="set-probe", custom_value={"value": 7}),
            mobject,
        )
        self.assertEqual(mobject.name, "set-probe")
        self.assertEqual(mobject.custom_value, {"value": 7})


if __name__ == "__main__":
    unittest.main()
