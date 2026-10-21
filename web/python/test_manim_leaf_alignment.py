import unittest
from types import SimpleNamespace

from noon import Mobject


class LeafAlignmentTests(unittest.TestCase):
    def test_align_submobjects_between_leaves_is_a_noop_returning_self(self):
        own = SimpleNamespace(submobjects=[])
        other = SimpleNamespace(submobjects=[])

        self.assertIs(Mobject.align_submobjects(own, other), own)


if __name__ == "__main__":
    unittest.main()
