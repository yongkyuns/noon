import unittest

import noon


class ManimMobjectBasePathMethodTests(unittest.TestCase):
    def test_base_path_queries_raise_the_pinned_override_error(self):
        mobject = object.__new__(noon.Mobject)

        with self.assertRaisesRegex(
            NotImplementedError, "^Please override in a child class\\.$"
        ):
            mobject.point_from_proportion(0.5)
        with self.assertRaisesRegex(
            NotImplementedError, "^Please override in a child class\\.$"
        ):
            mobject.proportion_from_point((0.0, 0.0, 0.0))


if __name__ == "__main__":
    unittest.main()
