"""Group coordinate and dimension ergonomics dispatch to shared Rust layout."""
import types
import unittest
from unittest.mock import patch

import noon
import _manim_semantic_handles as semantic


class GroupCoordinateDimensionTests(unittest.TestCase):
    def setUp(self):
        self.calls = []
        self.group = object.__new__(noon.VGroup)
        self.group._semantic_family_handle = object()
        self.anchor = types.SimpleNamespace(
            rescaleToFit=lambda *args: self.calls.append(("fit", args)),
        )

    def test_group_inherits_writable_dimension_properties(self):
        for cls in (noon.Group, noon.VGroup):
            with self.subTest(cls=cls.__name__):
                group = object.__new__(cls)
                with patch.object(semantic, "_layout_anchor", return_value=self.anchor), \
                     patch.object(semantic, "_group_live_layout_context", return_value=None):
                    group.width = 4
                    group.height = 2
        self.assertEqual(self.calls, [("fit", (4.0, 0, False)),
                                     ("fit", (2.0, 1, False))] * 2)

    def test_directional_coordinates_use_family_placement(self):
        layout = types.SimpleNamespace(
            moveToPoint=lambda *args: self.calls.append(("point", args)),
            moveToFamily=lambda *args: self.calls.append(("family", args)),
        )
        target = object.__new__(noon.Group)
        with patch.object(semantic, "_group_live_layout_context", return_value=None), \
             patch.object(semantic, "_shared_family_layout", return_value=layout):
            self.assertIs(self.group.set_x(3, noon.RIGHT), self.group)
            self.assertIs(self.group.set_y(-2, noon.DOWN), self.group)
            self.assertIs(self.group.match_x(target, noon.LEFT), self.group)
            self.assertIs(self.group.match_y(target, noon.UP), self.group)
        self.assertEqual(self.calls, [
            ("point", (3.0, 0.0, 1.0, 0.0, 1.0, 0.0)),
            ("point", (0.0, -2.0, 0.0, -1.0, 0.0, 1.0)),
            ("family", (layout, -1.0, 0.0, 1.0, 0.0)),
            ("family", (layout, 0.0, 1.0, 0.0, 1.0)),
        ])

    def test_live_group_coordinates_and_dimensions_use_published_context(self):
        context = types.SimpleNamespace(
            liveMoveFamilyToPoint=lambda *args: self.calls.append(("move", args)),
            liveRescaleToFit=lambda *args: self.calls.append(("fit", args)),
        )
        with patch.object(semantic, "_group_live_layout_context", return_value=context), \
             patch.object(semantic, "_layout_anchor", return_value=self.anchor):
            self.group.set_coord(5, 0, noon.LEFT)
            self.group.height = 3
        self.assertEqual(self.calls, [
            ("move", (self.group._semantic_family_handle, 5.0, 0.0, -1.0, 0.0, 1.0, 0.0)),
            ("fit", (self.anchor, 3.0, 1, False)),
        ])

    def test_invalid_coordinate_does_not_start_a_family_mutation(self):
        with patch.object(semantic, "_group_move_to") as move:
            with self.assertRaises(NotImplementedError):
                self.group.set_coord(1, 2)
            with self.assertRaises(ValueError):
                self.group.set_x(float("nan"))
            move.assert_not_called()
