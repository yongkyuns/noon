import copy
from enum import Enum, StrEnum
import pickle
import unittest

import noon
from noon import CapStyleType, DefaultSectionType, LineJointType, RendererType


class EnumExports(unittest.TestCase):
    def test_pinned_members_and_standard_enum_behavior(self):
        expected = {
            RendererType: {"CAIRO": "cairo", "OPENGL": "opengl"},
            LineJointType: {"AUTO": 0, "ROUND": 1, "BEVEL": 2, "MITER": 3},
            CapStyleType: {"AUTO": 0, "ROUND": 1, "BUTT": 2, "SQUARE": 3},
            DefaultSectionType: {"NORMAL": "default.normal"},
        }
        for enum, members in expected.items():
            self.assertEqual({name: member.value for name, member in enum.__members__.items()}, members)
            self.assertEqual([member.name for member in enum], list(members))
            for name, value in members.items():
                member = enum[name]
                self.assertIs(enum(value), member)
                self.assertIs(copy.deepcopy(member), member)
                self.assertIs(pickle.loads(pickle.dumps(member)), member)
            with self.assertRaises(ValueError):
                enum("invalid-manim-token")
        self.assertEqual(RendererType.__bases__, (Enum,))
        self.assertEqual(DefaultSectionType.__bases__, (StrEnum,))
        self.assertIsInstance(DefaultSectionType.NORMAL, str)
        self.assertEqual(str(DefaultSectionType.NORMAL), "default.normal")
        self.assertNotEqual(RendererType.CAIRO, "cairo")

    def test_public_imports_keep_identity_and_unique_exports(self):
        import _manim_namespace
        import _manim_enums
        namespace = {}
        exec("from noon import *", namespace)
        for enum in (RendererType, LineJointType, CapStyleType, DefaultSectionType):
            self.assertIs(namespace[enum.__name__], enum)
            self.assertIs(getattr(_manim_enums, enum.__name__), enum)
            self.assertIn(enum.__name__, dir(noon))
        _manim_namespace.install()
        _manim_namespace.install()
        self.assertEqual(len(noon.__all__), len(set(noon.__all__)))


if __name__ == "__main__":
    unittest.main()
