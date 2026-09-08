"""Exercise the published example's endpoint oracle without mocking an engine.

The snapshot contains getter results only. Actual animation/continuation behavior
remains covered by shared-authoring-smoke.mjs against the built Rust/WASM package.
"""

import ast
from pathlib import Path
import struct
from types import SimpleNamespace
import unittest


def f32(value):
    return struct.unpack("<f", struct.pack("<f", value))[0]


def adjacent_f32(value, offset):
    bits = struct.unpack("<I", struct.pack("<f", value))[0]
    return struct.unpack("<f", struct.pack("<I", bits + offset))[0]


class PaintExampleEndpointOracleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        source_path = Path(__file__).with_name("examples") / "ordinary_paint_play.py"
        tree = ast.parse(source_path.read_text(encoding="utf-8"), str(source_path))
        scene = next(node for node in tree.body if isinstance(node, ast.ClassDef))
        construct = next(node for node in scene.body if isinstance(node, ast.FunctionDef)
                         and node.name == "construct")

        def is_call(statement, owner, name):
            return (isinstance(statement, ast.Expr)
                    and isinstance(statement.value, ast.Call)
                    and isinstance(statement.value.func, ast.Attribute)
                    and isinstance(statement.value.func.value, ast.Name)
                    and statement.value.func.value.id == owner
                    and statement.value.func.attr == name)

        start = next(i for i, statement in enumerate(construct.body)
                     if is_call(statement, "self", "play")) + 1
        end = next(i for i in range(start, len(construct.body))
                   if is_call(construct.body[i], "circle", "set_fill"))
        oracle = ast.Module(body=construct.body[start:end], type_ignores=[])
        # An empty or accidentally removed endpoint oracle must not pass these tests.
        assertions = [node for node in ast.walk(oracle) if isinstance(node, ast.Assert)]
        if len(assertions) != 3:
            raise AssertionError("expected fill, stroke, and scene-time endpoint assertions")
        cls.oracle = compile(oracle, str(source_path), "exec")

    def evaluate(self, fill, stroke, time=0.4):
        circle = SimpleNamespace(get_fill_opacity=lambda: fill,
                                 get_stroke_opacity=lambda: stroke)
        exec(self.oracle, {"circle": circle, "self": SimpleNamespace(time=time),
                           "struct": struct})

    def test_exact_rust_f32_endpoints_pass(self):
        self.evaluate(f32(0.8), f32(0.3))

    def test_original_decimal_oracle_rejects_correct_runtime_values(self):
        self.assertGreater(abs(f32(0.8) - 0.8), 1e-9)
        self.assertGreater(abs(f32(0.3) - 0.3), 1e-9)

    def test_unrounded_authored_values_are_not_effective_runtime_results(self):
        with self.assertRaises(AssertionError):
            self.evaluate(0.8, 0.3)

    def test_one_ulp_fill_errors_are_rejected(self):
        for offset in (-1, 1):
            with self.subTest(offset=offset), self.assertRaises(AssertionError):
                self.evaluate(adjacent_f32(0.8, offset), f32(0.3))

    def test_one_ulp_stroke_errors_are_rejected(self):
        for offset in (-1, 1):
            with self.subTest(offset=offset), self.assertRaises(AssertionError):
                self.evaluate(f32(0.8), adjacent_f32(0.3, offset))

    def test_channels_remain_independent(self):
        for fill, stroke in ((f32(0.3), f32(0.8)), (f32(0.8), f32(0.8)),
                             (f32(0.3), f32(0.3)), (f32(0.2), f32(0.2))):
            with self.subTest(fill=fill, stroke=stroke), self.assertRaises(AssertionError):
                self.evaluate(fill, stroke)

    def test_nonfinite_opacity_is_rejected(self):
        for invalid in (float("nan"), float("inf"), -float("inf")):
            with self.subTest(invalid=invalid, channel="fill"), self.assertRaises(AssertionError):
                self.evaluate(invalid, f32(0.3))
            with self.subTest(invalid=invalid, channel="stroke"), self.assertRaises(AssertionError):
                self.evaluate(f32(0.8), invalid)

    def test_endpoint_time_assertion_is_preserved(self):
        with self.assertRaises(AssertionError):
            self.evaluate(f32(0.8), f32(0.3), time=0.3)


if __name__ == "__main__":
    unittest.main()
