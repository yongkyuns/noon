import unittest
from unittest.mock import patch

import _manim_compat as compat
import _manim_svg as svg


class _LeafHandle:
    def __init__(self, slot):
        self.semanticSlot = slot
        self.semanticGeneration = 0


class _FamilyHandle:
    def __init__(self, count=2):
        self._members = [_LeafHandle(index + 1) for index in range(count)]
        self.memberCount = count

    def memberMobject(self, index):
        return self._members[index]

    def memberKeys(self):
        return [f"{member.semanticSlot}:{member.semanticGeneration}" for member in self._members]


class SvgFacadeTests(unittest.TestCase):
    def test_from_string_wraps_authoritative_family_members_without_geometry_copy(self):
        calls = []
        family = _FamilyHandle()

        def create(source, should_center, height, width):
            calls.append((source, should_center, height, width))
            return family

        with patch.object(svg, "_create_svg_handle", create):
            value = svg.SVGMobject.from_string(
                '<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0 L1 0"/></svg>'
            )

        self.assertIs(value._semantic_family_handle, family)
        self.assertEqual(len(value), 2)
        self.assertEqual(len(value.submobjects), 2)
        self.assertTrue(all(isinstance(member, compat.VMobject) for member in value.submobjects))
        self.assertEqual(
            calls,
            [(
                '<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0 L1 0"/></svg>',
                True,
                2.0,
                None,
            )],
        )

    def test_unsupported_parser_configuration_fails_before_rust_call(self):
        calls = []
        with patch.object(svg, "_create_svg_handle", lambda *args: calls.append(args)):
            with self.assertRaises(NotImplementedError):
                svg.SVGMobject.from_string("<svg/>", path_string_config={"should_subdivide_sharp_curves": True})
        self.assertEqual(calls, [])

    def test_empty_source_is_rejected_before_rust_call(self):
        calls = []
        with patch.object(svg, "_create_svg_handle", lambda *args: calls.append(args)):
            with self.assertRaises(ValueError):
                svg.SVGMobject.from_string("   ")
        self.assertEqual(calls, [])


if __name__ == "__main__":
    unittest.main()
