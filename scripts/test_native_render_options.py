"""Actual CPython binding to shared Rust output values, without a renderer."""
from fractions import Fraction
import unittest

import _noon_native as native
import _noon_native_host as host
from noon import NoonUnsupportedError, NoonValueError, resolve_render_options


class NativeRenderOptions(unittest.TestCase):
    def test_exact_ratio_and_value_overrides_need_no_scene_context(self):
        owner = host._store.get()
        value = resolve_render_options(quality="l", resolution=(65, 33),
                                       frame_rate=Fraction(60000, 1001), format="png")
        self.assertEqual((value.width, value.height, value.fps, value.format),
                         (65, 33, (60000, 1001), "png"))
        self.assertIs(host._store.get(), owner)

    def test_shared_errors_keep_their_typed_python_identity_and_code(self):
        for options, kind, code in (
            ({"quality": "L"}, NoonValueError, "render.quality"),
            ({"resolution": "65x33"}, NoonValueError, "render.resolution"),
            ({"frame_rate": "nan"}, NoonValueError, "render.frame_rate"),
            ({"format": "gif"}, NoonUnsupportedError, "render.unsupported_format"),
        ):
            with self.subTest(options=options), self.assertRaises(kind) as caught:
                resolve_render_options(**options)
            self.assertEqual(caught.exception.code, code)

    def test_direct_binding_cannot_bypass_shared_dimension_validation(self):
        with self.assertRaises(RuntimeError) as caught:
            native.resolve_render_options(None, None, "15", None, 0, None)
        self.assertEqual(caught.exception.code, "render.resolution")


if __name__ == "__main__":
    unittest.main()
