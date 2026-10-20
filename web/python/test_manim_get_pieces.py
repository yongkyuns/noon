import json
from pathlib import Path
import unittest
from unittest.mock import Mock, call, patch

import _manim_compat as compat
from _typed_geometry_test_support import identity_only_wrapper


ROOT = Path(__file__).resolve().parents[2]


class ManimGetPiecesTests(unittest.TestCase):
    def test_leaf_composes_equal_partial_intervals_into_plain_group(self):
        source = identity_only_wrapper(compat.VMobject)
        template = Mock()
        pieces = [Mock(name=f"piece_{index}") for index in range(2)]
        source.copy = Mock(return_value=template)
        template.copy.side_effect = pieces
        for piece in pieces:
            piece.pointwise_become_partial.return_value = piece

        class GroupResult:
            def __init__(self, *members):
                self.submobjects = list(members)

        with patch.object(compat, "_is_shared_family", return_value=False), patch.object(
            compat, "Group", GroupResult
        ):
            result = source.get_pieces(2)

        self.assertIsInstance(result, GroupResult)
        self.assertEqual(result.submobjects, pieces)
        self.assertEqual(
            [piece.pointwise_become_partial.call_args for piece in pieces],
            [call(source, 0.0, 0.5), call(source, 0.5, 1.0)],
        )
        source.copy.assert_called_once_with()
        self.assertEqual(template.copy.call_count, 2)

    def test_zero_pieces_returns_an_empty_group_and_invalid_count_fails(self):
        source = identity_only_wrapper(compat.VMobject)
        template = Mock()
        source.copy = Mock(return_value=template)

        class GroupResult:
            def __init__(self, *members):
                self.submobjects = list(members)

        with patch.object(compat, "_is_shared_family", return_value=False), patch.object(
            compat, "Group", GroupResult
        ):
            self.assertEqual(len(source.get_pieces(0).submobjects), 0)
            with self.assertRaises(TypeError):
                source.get_pieces(1.5)

        self.assertEqual(template.copy.call_count, 0)
        self.assertEqual(source.copy.call_count, 2)

    def test_vgroup_root_is_rejected_before_copying(self):
        family = identity_only_wrapper(compat.VGroup)
        family.copy = Mock()
        with patch.object(compat, "_is_shared_family", return_value=True):
            with self.assertRaisesRegex(NotImplementedError, "shared-family roots"):
                family.get_pieces(2)
        family.copy.assert_not_called()

    def test_compatibility_ledger_records_the_bounded_leaf_profile(self):
        policy = json.loads(
            (ROOT / "compat" / "manim-v0.21.0.json").read_text(encoding="utf-8")
        )
        disposition = policy["overrides"]["Mobject"]
        self.assertIn("Mobject.get_pieces()", disposition["reason"])
        self.assertIn(
            "scripts/manim-differential.py:mobject_get_pieces_live",
            disposition["evidence"],
        )


if __name__ == "__main__":
    unittest.main()
