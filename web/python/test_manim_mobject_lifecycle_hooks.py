import copy
import unittest
from unittest.mock import patch

import noon


class ManimMobjectLifecycleHookTests(unittest.TestCase):
    def test_base_hooks_are_noop_and_return_the_receiver(self):
        mobject = object.__new__(noon.Mobject)
        state_before = copy.deepcopy(mobject.__dict__)

        self.assertIs(mobject.generate_points(), mobject)
        self.assertIs(mobject.init_colors(), mobject)
        self.assertIs(mobject.init_colors(propagate_colors=False), mobject)
        self.assertEqual(mobject.__dict__, state_before)

    def test_constructor_dispatches_subclass_hooks_in_order(self):
        class CustomMobject(noon.Mobject):
            def __init__(self):
                self.hook_calls = []
                super().__init__()

            def generate_points(self):
                self.hook_calls.append("generate_points")
                return self

            def init_colors(self, propagate_colors=True):
                self.hook_calls.append(("init_colors", propagate_colors))
                return self

        with patch.object(noon, "_semantic_operations"):
            mobject = CustomMobject()

        self.assertEqual(
            mobject.hook_calls,
            ["generate_points", ("init_colors", True)],
        )


if __name__ == "__main__":
    unittest.main()
