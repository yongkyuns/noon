import unittest

import noon


class ManimAnimationOverrideTests(unittest.TestCase):
    def test_override_animation_is_available_from_star_import(self):
        namespace = {}
        exec("from noon import *", namespace)
        self.assertIs(namespace["override_animation"], noon.override_animation)

    def test_registration_lookup_duplicate_error_and_subclass_isolation(self):
        class Animation:
            pass

        class Base(noon.Mobject):
            pass

        def first_override(*args, **kwargs):
            return args, kwargs

        def second_override(*args, **kwargs):
            return args, kwargs

        self.assertIsNone(Base.animation_override_for(Animation))
        Base.add_animation_override(Animation, first_override)
        self.assertIs(Base.animation_override_for(Animation), first_override)

        class Child(Base):
            pass

        self.assertIsNone(Child.animation_override_for(Animation))
        with self.assertRaises(noon.MultiAnimationOverrideException) as raised:
            Base.add_animation_override(Animation, second_override)
        self.assertEqual(
            str(raised.exception),
            "The animation Animation for Base is overridden by more than one "
            "method: "
            "ManimAnimationOverrideTests.test_registration_lookup_duplicate_error_and_subclass_isolation.<locals>.first_override "
            "and "
            "ManimAnimationOverrideTests.test_registration_lookup_duplicate_error_and_subclass_isolation.<locals>.second_override.",
        )

    def test_subclass_discovers_intrinsic_override_markers(self):
        class Animation:
            pass

        def intrinsic_override(self):
            return self

        undecorated_override = intrinsic_override
        intrinsic_override = noon.override_animation(Animation)(intrinsic_override)
        self.assertIs(intrinsic_override, undecorated_override)
        self.assertIs(intrinsic_override._override_animation, Animation)

        Intrinsic = type(
            "Intrinsic",
            (noon.Mobject,),
            {"intrinsic_override": intrinsic_override},
        )
        IntrinsicChild = type("IntrinsicChild", (Intrinsic,), {})
        self.assertIs(
            Intrinsic.animation_override_for(Animation),
            Intrinsic.intrinsic_override,
        )
        self.assertIs(
            IntrinsicChild.animation_override_for(Animation),
            IntrinsicChild.intrinsic_override,
        )


if __name__ == "__main__":
    unittest.main()
