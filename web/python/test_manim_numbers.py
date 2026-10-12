import os
from pathlib import Path
import subprocess
import sys
import textwrap
import unittest


class ManimNumberWrapperTests(unittest.TestCase):
    def test_decimal_routes_construction_mutation_and_observation_to_rust_handles(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        existing = env.get("PYTHONPATH")
        env["PYTHONPATH"] = str(python_dir) if not existing else os.pathsep.join((str(python_dir), existing))
        source = textwrap.dedent(
            """
            import sys
            import types

            calls = []

            class Semantic:
                def setColor(self, *args): calls.append(("color", args))
                def setObjectOpacity(self, *args): calls.append(("opacity", args))

            class Number:
                def __init__(self, value): self._value = value; self.semantic = Semantic()
                def mobject(self): return self.semantic
                def value(self, context=None):
                    return context.queryMobjectDecimalValue(self.semantic) if context is not None else self._value
                def text(self): return f"{self._value:.2f}"
                def fontSize(self, context):
                    calls.append(("font-size", context))
                    return 96.0 if context is not None else 48.0
                def integerValue(self, context=None):
                    return context.queryMobjectIntegerValue(self.semantic) if context is not None else round(self._value)
                def setValue(self, value): calls.append(("set", value)); self._value = value
                def incrementValue(self, delta): calls.append(("increment", delta)); self._value += delta
                def setValueLive(self, context, value): context.liveSetDecimalValue(self.semantic, value); self._value = value
                def incrementValueLive(self, context, delta): context.liveIncrementDecimalValue(self.semantic, delta); self._value += delta

            def create(*args):
                calls.append(("create", args))
                return Number(args[0])

            fake_js = types.ModuleType("js")
            fake_js.noonCreateAuthoringDecimalNumberHandle = create
            fake_js.noonNumericFromMobject = lambda semantic: Number(0)
            sys.modules["js"] = fake_js

            import _manim_numbers as numbers
            import noon
            numbers._live_text_context = lambda: None
            decimal = noon.DecimalNumber(1.25, num_decimal_places=2, include_sign=True, unit="m")
            assert calls[0][0] == "create"
            assert calls[0][1][:7] == (1.25, 2, True, True, False, "m", 48.0)
            assert calls[0][1][-1] is None
            assert decimal.get_value() == 1.25
            assert decimal.font_size == 48.0
            decimal.set_value(3.5).increment_value(0.5)
            assert decimal.get_value() == 4.0
            assert decimal._source == "4.00"
            assert [entry[0] for entry in calls if entry[0] in {"set", "increment"}] == ["set", "increment"]

            class Context:
                def liveExecutionOwnership(self): return "returned"
                def liveSetColor(self, *args): calls.append(("live-color", args))
                def liveSetObjectOpacity(self, *args): calls.append(("live-opacity", args))
                def liveSetDecimalValue(self, semantic, value):
                    calls.append(("live-set", semantic, value))
                def liveIncrementDecimalValue(self, semantic, value):
                    calls.append(("live-increment", semantic, value))
                def queryMobjectDecimalValue(self, semantic):
                    calls.append(("live-value", semantic)); return 7.5
                def queryMobjectIntegerValue(self, semantic):
                    calls.append(("live-integer-value", semantic)); return 8

            context = Context()
            numbers._live_text_context = lambda: context
            live = noon.Integer(2.5)
            assert live._canonical_live_target_context is context
            assert calls[-3][0] == "create"
            assert calls[-3][1][1] == 0
            assert calls[-3][1][-1] is context
            assert live.get_value() == 2
            assert live.font_size == 48.0
            live._scene = object()
            assert live.get_value() == 8
            assert calls[-1][0] == "live-integer-value"
            decimal_live = noon.DecimalNumber(2.5)
            decimal_live._scene = object()
            assert decimal_live.get_value() == 7.5
            assert calls[-1][0] == "live-value"
            live.set_value(5).increment_value(2)
            assert [entry[0] for entry in calls if entry[0] in {"live-set", "live-increment"}] == [
                "live-set", "live-increment",
            ]

            class ColdContext:
                def liveExecutionOwnership(self): return "unstarted"

            cold_context = ColdContext()
            wrapped = numbers.DecimalNumber._from_numeric_handle(Number(4), cold_context)
            assert wrapped._canonical_live_target_context is cold_context
            assert [entry for entry in calls if entry[0] == "font-size"][-1][1] is None
            wrapped_live = numbers.DecimalNumber._from_numeric_handle(Number(4), context)
            assert [entry for entry in calls if entry[0] == "font-size"][-1][1] is context
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source],
            cwd=python_dir,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(
            completed.returncode,
            0,
            f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
