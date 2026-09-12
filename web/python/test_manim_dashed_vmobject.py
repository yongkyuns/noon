import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimDashedVMobjectTests(unittest.TestCase):
    def test_constructor_routes_detached_and_live_work_to_rust(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        existing = env.get("PYTHONPATH")
        env["PYTHONPATH"] = str(python_dir) if not existing else os.pathsep.join((str(python_dir), existing))
        source = textwrap.dedent(
            r"""
            import _manim_compat as compat
            import _manim_semantic_handles as shared

            calls = []

            class FakeHandle:
                def __init__(self, name):
                    self.name = name

                def dashedVmobject(self, count, ratio, offset, equal):
                    calls.append(("detached", self.name, count, ratio, offset, equal))
                    return FakeHandle("detached-result")

            class FakeContext:
                def liveDashedVMobject(self, source, count, ratio, offset, equal):
                    calls.append(("live", source.name, count, ratio, offset, equal))
                    return FakeHandle("live-result")

            source = object.__new__(compat.VMobject)
            shared._attach_shared_handle(source, FakeHandle("source"))
            source.get_subcurve = lambda *_: (_ for _ in ()).throw(
                AssertionError("Python subcurve segmentation was called")
            )

            import _manim_dashed_vmobject as dashed

            result = dashed.DashedVMobject(
                source,
                num_dashes=7,
                dashed_ratio=0.4,
                dash_offset=-0.25,
                equal_lengths=False,
            )
            assert isinstance(result, compat.VMobject)
            assert result._semantic_handle.name == "detached-result"
            assert calls == [("detached", "source", 7, 0.4, -0.25, False)]

            context = FakeContext()
            original_live = shared._live_mutation_context
            shared._live_mutation_context = lambda value: context
            try:
                live = dashed.DashedVMobject(source, num_dashes=2)
            finally:
                shared._live_mutation_context = original_live
            assert live._semantic_handle.name == "live-result"
            assert live._canonical_live_target_context is context
            assert calls[-1] == ("live", "source", 2, 0.5, 0.0, True)

            before = list(calls)
            for kwargs in (
                {"dashed_ratio": -0.1},
                {"dashed_ratio": 1.1},
                {"dash_offset": float("nan")},
                {"num_dashes": 1 << 40},
            ):
                try:
                    dashed.DashedVMobject(source, **kwargs)
                except ValueError:
                    pass
                else:
                    raise AssertionError("invalid dashed parameters must fail before Rust dispatch")
            assert calls == before
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
