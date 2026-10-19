import unittest
from unittest.mock import Mock

import noon


class ManimMobjectSurroundTests(unittest.TestCase):
    def test_surround_replaces_then_scales_with_buffer_and_returns_self(self):
        mobject = object.__new__(noon.Mobject)
        target = Mock()
        target.length_over_dim.return_value = 2.0
        mobject.replace = Mock(return_value=mobject)
        mobject.scale = Mock(return_value=mobject)

        returned = mobject.surround(
            target, dim_to_match=1, stretch=True, buff=0.5
        )

        self.assertIs(returned, mobject)
        mobject.replace.assert_called_once_with(target, 1, True)
        mobject.scale.assert_called_once_with(1.25)
        target.length_over_dim.assert_called_once_with(1)


if __name__ == "__main__":
    unittest.main()
