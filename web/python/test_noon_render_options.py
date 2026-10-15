"""Python coercion and resource-free values; shared Rust owns option meanings."""
from dataclasses import FrozenInstanceError
from fractions import Fraction
from types import SimpleNamespace
import sys
import unittest
from unittest.mock import Mock, patch

from _noon_render_options import RenderOptions, resolve_render_options


class RenderOptionCoercion(unittest.TestCase):
    def bridge(self):
        return Mock(return_value=SimpleNamespace(pixelWidth=65, pixelHeight=33,
                    frameRateNumerator=2997, frameRateDenominator=100, format="png"))

    def test_values_project_once_and_keep_the_rust_ratio_without_float_conversion(self):
        bridge = self.bridge()
        with patch.dict(sys.modules, {"_noon_host": SimpleNamespace(noonResolveRenderOptions=bridge)}):
            result = resolve_render_options(quality="l", resolution=(65, 33),
                                            frame_rate=Fraction(2997, 100), format="png")
        bridge.assert_called_once_with("l", "65,33", "2997/100", "png", None, None)
        self.assertEqual(result, RenderOptions(65, 33, (2997, 100), "png"))
        with self.assertRaises(FrozenInstanceError):
            result.width = 100

    def test_numeric_spellings_and_explicit_ratios_reach_the_shared_parser(self):
        for value, expected in ((60, "60"), (29.97, "29.97"),
                                ((60000, 1001), "60000/1001"), (None, None)):
            bridge = self.bridge()
            with patch.dict(sys.modules, {"_noon_host": SimpleNamespace(noonResolveRenderOptions=bridge)}):
                resolve_render_options(frame_rate=value)
            self.assertEqual(bridge.call_args.args[2], expected)

    def test_invalid_python_types_and_dimensions_fail_before_the_host_call(self):
        for options in ({"frame_rate": True}, {"frame_rate": object()},
                        {"frame_rate": [1, True]}, {"frame_rate": [1, 2, 3]},
                        {"quality": 1}, {"format": False},
                        {"resolution": [1, False]}, {"resolution": object()},
                        {"pixel_width": True}, {"pixel_height": 1.5},
                        {"pixel_width": -1}, {"pixel_height": 2**32}):
            bridge = self.bridge()
            with patch.dict(sys.modules, {"_noon_host": SimpleNamespace(noonResolveRenderOptions=bridge)}):
                with self.assertRaises((TypeError, ValueError)):
                    resolve_render_options(**options)
            bridge.assert_not_called()


if __name__ == "__main__":
    unittest.main()
