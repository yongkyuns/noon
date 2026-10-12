import unittest

from noon import Mobject


class ConstructorDefaults(unittest.TestCase):
    def probe(self):
        class Probe(Mobject):
            def __init__(self, *, color="white", radius=1):
                self.arguments = (color, radius)
        return Probe

    def test_defaults_layer_and_per_call_keywords_take_precedence(self):
        probe = self.probe()
        original = probe.__init__
        probe.set_default(color="red")
        probe.set_default(radius=2)
        self.assertEqual(probe().arguments, ("red", 2))
        self.assertEqual(probe(color="blue").arguments, ("blue", 2))
        self.assertEqual(probe(radius=3).arguments, ("red", 3))
        probe.set_default()
        self.assertIs(probe.__init__, original)
        self.assertEqual(probe().arguments, ("white", 1))

    def test_child_reset_uses_constructor_captured_at_class_definition(self):
        parent = self.probe()
        parent.set_default(color="red")
        class Child(parent): pass
        Child.set_default(radius=4)
        self.assertEqual(Child().arguments, ("red", 4))
        self.assertEqual(parent().arguments, ("red", 1))
        Child.set_default()
        self.assertEqual(Child().arguments, ("red", 1))
        parent.set_default()
        self.assertEqual(Child().arguments, ("red", 1))
        self.assertEqual(parent().arguments, ("white", 1))

    def test_sibling_defaults_are_isolated_and_unknown_arguments_stay_errors(self):
        first, second = self.probe(), self.probe()
        first.set_default(color="red")
        self.assertEqual(second().arguments, ("white", 1))
        first.set_default(unknown=1)
        with self.assertRaises(TypeError): first()
        first.set_default()
        self.assertEqual(first().arguments, ("white", 1))


if __name__ == "__main__":
    unittest.main()
