import unittest

import noon


class ManimMobjectDepthTests(unittest.TestCase):
    def test_planar_depth_fitting_is_a_receiver_preserving_noop(self):
        class RecordingMobject(noon.Mobject):
            def rescale_to_fit(self, *args, **kwargs):
                self.rescale_calls.append((args, kwargs))
                return self

        square = object.__new__(RecordingMobject)
        square.rescale_calls = []
        square.layout_metadata = {"width": 2.0, "height": 2.0, "center": (0, 0)}

        self.assertEqual(square.depth, 0.0)
        self.assertIs(
            square.scale_to_fit_depth(3.0, about_point=object()), square
        )
        self.assertIs(
            square.stretch_to_fit_depth(4.0, about_edge=object()), square
        )
        square.depth = 5.0

        self.assertEqual(square.depth, 0.0)
        self.assertEqual(square.rescale_calls, [])
        self.assertEqual(
            square.layout_metadata,
            {"width": 2.0, "height": 2.0, "center": (0, 0)},
        )

    def test_group_depth_reflects_the_planar_member_profile(self):
        group = object.__new__(noon.Group)

        self.assertEqual(group.depth, 0.0)
        self.assertIs(group.scale_to_fit_depth(2.0), group)
        self.assertEqual(group.depth, 0.0)


if __name__ == "__main__":
    unittest.main()
