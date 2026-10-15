import math
import unittest
import noon
from noon import unit


class UnitScalarTests(unittest.TestCase):
    def test_values_are_module_only_and_keep_numeric_types(self):
        self.assertIs(type(unit.Degrees), float)
        self.assertIs(type(unit.Munits), int)
        self.assertAlmostEqual(180 * unit.Degrees, math.pi)
        for name in ("Degrees", "Munits"):
            self.assertNotIn(name, noon.__all__)
            self.assertFalse(hasattr(noon, name))
        self.assertIn("unit", noon.__all__)
        self.assertIs(noon.unit, unit)
        self.assertFalse(hasattr(unit, "Pixels"))
        self.assertFalse(hasattr(unit, "Percent"))

    def test_user_data_arithmetic_preserves_user_defined_dispatch(self):
        class Recorded:
            def __mul__(self, other): return ("left", other)
            def __rmul__(self, other): return ("right", other)
        for scale in (unit.Degrees, unit.Munits):
            value = Recorded()
            self.assertEqual(value * scale, ("left", scale))
            self.assertEqual(scale * value, ("right", scale))
            self.assertEqual([3] * unit.Munits, [3])
        self.assertTrue(math.isnan(float("nan") * unit.Degrees))
        self.assertEqual(float("inf") * unit.Munits, float("inf"))


if __name__ == "__main__": unittest.main()
