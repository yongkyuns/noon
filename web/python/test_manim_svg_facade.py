import os
import subprocess
import sys
import textwrap
import unittest
from unittest.mock import patch
from pathlib import Path

import noon
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


from _typed_family_test_support import install_split_queries
install_split_queries(_FamilyHandle, lambda self: self._members)


class SvgFacadeTests(unittest.TestCase):
    def test_lazy_public_exports_bind_only_after_success(self):
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = str(python_dir)
        source = textwrap.dedent(
            """
            import importlib
            import sys
            import types
            sys.modules["js"] = types.ModuleType("js")
            import noon

            provider = types.ModuleType("_cached_export_provider")
            provider.CachedExport = object()
            sys.modules[provider.__name__] = provider
            noon._PUBLIC_EXPORTS["CachedExport"] = provider.__name__
            calls = [0]
            original = noon.__getattr__
            def counted(name):
                calls[0] += 1
                return original(name)
            noon.__getattr__ = counted

            first = getattr(noon, "CachedExport")
            assert getattr(noon, "CachedExport") is first
            assert first is provider.CachedExport and calls == [1]

            noon._PUBLIC_EXPORTS["RetryExport"] = "_missing_export_provider"
            try:
                noon.RetryExport
            except ModuleNotFoundError:
                pass
            else:
                raise AssertionError("missing provider unexpectedly imported")
            assert "RetryExport" not in noon.__dict__
            retry_provider = types.ModuleType("_missing_export_provider")
            retry_provider.RetryExport = object()
            sys.modules[retry_provider.__name__] = retry_provider
            assert noon.RetryExport is retry_provider.RetryExport

            noon._PUBLIC_EXPORTS["LateExport"] = "_late_export_provider"
            late_provider = types.ModuleType("_late_export_provider")
            sys.modules[late_provider.__name__] = late_provider
            try:
                noon.LateExport
            except AttributeError:
                pass
            else:
                raise AssertionError("missing provider attribute unexpectedly resolved")
            assert "LateExport" not in noon.__dict__
            late_provider.LateExport = object()
            assert noon.LateExport is late_provider.LateExport

            try:
                noon.NotAPublicExport
            except AttributeError as error:
                assert "NotAPublicExport" in str(error)
            else:
                raise AssertionError("unknown public attribute unexpectedly resolved")

            # This provider imports noon while it is being initialized.
            svg_export = noon.SVGMobject
            svg_provider = importlib.import_module("_manim_svg")
            assert svg_export is svg_provider.SVGMobject
            assert noon.SVGMobject is svg_export
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source],
            check=False,
            cwd=python_dir,
            env=env,
            capture_output=True,
            text=True,
        )
        self.assertEqual(
            completed.returncode,
            0,
            f"lazy export subprocess failed:\nstdout:{completed.stdout}\nstderr:{completed.stderr}",
        )

    def test_public_noon_export_is_lazy_svg_facade(self):
        self.assertIs(noon.SVGMobject, svg.SVGMobject)
        self.assertIn("SVGMobject", noon.__all__)

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

    def test_custom_svg_default_uses_effective_typed_transport(self):
        create_calls = []
        transport_calls = []
        family = _FamilyHandle(count=1)

        class Options:
            @staticmethod
            def svgDefaultTransport(*args):
                transport_calls.append(args)
                return ("svg-default-transport", *args)

        def create(source, should_center, height, width_or_defaults):
            create_calls.append((source, should_center, height, width_or_defaults))
            return family

        defaults = {
            "color": "#010203",
            "opacity": 0.2,
            "fill_color": "#112233",
            "fill_opacity": None,
            "stroke_width": None,
            "stroke_color": None,
            "stroke_opacity": 0.8,
        }
        source = '<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0 L1 0"/></svg>'
        with (
            patch.object(svg, "_svg_transport_options", Options),
            patch.object(svg, "_create_svg_handle", create),
        ):
            value = svg.SVGMobject.from_string(source, width=4.0, height=None, svg_default=defaults)

        self.assertIs(value._semantic_family_handle, family)
        self.assertEqual(
            transport_calls,
            [(4.0, 0x112233, 0.2, 0x010203, 0.8, None)],
        )
        self.assertEqual(create_calls[0][:3], (source, True, None))
        self.assertEqual(create_calls[0][3], ("svg-default-transport", *transport_calls[0]))
        self.assertEqual(value.svg_default, defaults)

    def test_svg_default_float_color_matches_manim_hex_truncation(self):
        self.assertEqual(
            svg._rgb24("svg_default.fill_color", noon.Color(0.5, 0.1, 1.0)),
            0x7F19FF,
        )

    def test_custom_svg_default_requires_all_manim_keys_before_rust_call(self):
        calls = []
        defaults = dict(svg._MANIM_DEFAULT_SVG_STYLE)
        defaults.pop("stroke_opacity")
        with patch.object(svg, "_create_svg_handle", lambda *args: calls.append(args)):
            with self.assertRaisesRegex(KeyError, "stroke_opacity"):
                svg.SVGMobject.from_string("<svg/>", svg_default=defaults)
        self.assertEqual(calls, [])

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
