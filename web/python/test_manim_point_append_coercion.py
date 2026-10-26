"""Actual CE incoming-argument/public-hook regressions; Rust owns point state."""
import builtins
import json
from pathlib import Path
import sys
import types
import unittest
from unittest.mock import Mock, patch

try:
    import numpy as np
except ImportError:
    np = None

import _manim_compat as compat
import _manim_path_editing as editing
from _typed_geometry_test_support import identity_only_wrapper

FIXTURE = json.loads((Path(__file__).resolve().parents[2] / "fixtures/manim-v0.21/point-append.json").read_text())


def argument(spec):
    return np.array(spec["values"], dtype=spec["dtype"]) if spec["container"] == "array" else spec["values"].copy()


@unittest.skipIf(np is None, "NumPy coercion runs in the pinned qualification lane")
class PointAppendCoercionTests(unittest.TestCase):
    def test_incoming_arguments_match_original_ce_before_shared_publication(self):
        ffi = types.ModuleType("pyodide.ffi")
        ffi.to_js = lambda values: values
        for cls in (compat.VMobject, compat.VGroup):
            for case in FIXTURE["cases"]:
                if case["prefixRows"] != 0:
                    continue  # Prefix preservation is tested by real Rust/browser paths.
                with self.subTest(cls=cls.__name__, method=case["method"], spec=case["spec"]["id"]):
                    value = identity_only_wrapper(cls)
                    handle = Mock()
                    if cls is compat.VGroup:
                        value._semantic_family_handle = handle
                    incoming = argument(case["spec"])
                    before = np.asarray(incoming).copy()
                    with patch.dict(sys.modules, {"pyodide.ffi": ffi}), \
                         patch.object(editing, "_handle_for", return_value=handle), \
                         patch.object(editing, "_live_mutation_context", return_value=None), \
                         patch.object(editing, "_live_constructor_context", return_value=None), \
                         patch("_manim_updaters.canonical_callback_phase_active", return_value=False):
                        if case["output"]["kind"] == "raised":
                            with self.assertRaises(getattr(builtins, case["output"]["type"])) as raised:
                                getattr(value, case["method"])(incoming)
                            self.assertEqual(str(raised.exception), case["output"]["message"])
                            handle.appendPoints3D.assert_not_called()
                        else:
                            self.assertIs(getattr(value, case["method"])(incoming), value)
                            handle.appendPoints3D.assert_called_once_with([coordinate for point in case["pointsXYZ"] for coordinate in point])
                    np.testing.assert_array_equal(np.asarray(incoming), before)

    def test_public_append_hooks_keep_ce_argument_identity_shape_and_return(self):
        for cls in (compat.VMobject, compat.VGroup):
            for case in FIXTURE["hookCases"]:
                with self.subTest(cls=cls.__name__, method=case["method"], spec=case["spec"]["id"]):
                    value = identity_only_wrapper(cls)
                    incoming = argument(case["spec"])
                    events = []
                    def append(received):
                        array = np.asarray(received)
                        events.append({"isOriginal": received is incoming, "type": type(received).__name__, "shape": list(array.shape), "dtype": array.dtype.name, "values": array.tolist()})
                        return object()
                    value.append_points = append
                    if case["output"]["kind"] == "raised":
                        with self.assertRaises(getattr(builtins, case["output"]["type"])) as raised:
                            getattr(value, case["method"])(incoming)
                        self.assertEqual(str(raised.exception), case["output"]["message"])
                    else:
                        self.assertIs(getattr(value, case["method"])(incoming), value)
                    self.assertEqual(events, case["appendHook"])


if __name__ == "__main__":
    unittest.main()
