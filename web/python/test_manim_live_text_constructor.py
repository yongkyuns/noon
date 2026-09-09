import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimLiveTextConstructorTests(unittest.TestCase):
    def test_live_text_routes_through_current_context_and_rejects_unsupported_kinds(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        existing = env.get("PYTHONPATH")
        env["PYTHONPATH"] = (
            str(python_dir)
            if not existing
            else os.pathsep.join((str(python_dir), existing))
        )
        source = textwrap.dedent(
            r"""
            import _manim_compat
            _manim_compat.install()
            import _manim_semantic_handles as handles
            import _typed_geometry_test_support as _geometry_test
            _geometry_test.install_module_bridge(handles, lambda *args: object())

            import _manim_typst as typst
            import noon

            class LiveContext:
                def __init__(self): self.calls = []
                def liveExecutionOwnership(self): return "returned"
                def liveCreateManimText(self, *args):
                    self.calls.append(("text", args))
                    return object()
                def liveCreateManimTypst(self, *args):
                    self.calls.append(("typst", args))
                    return object()
                def liveTargetEditor(self, handle):
                    self.calls.append(("copy", handle))
                    return object()

            context = LiveContext()
            handles._live_constructor_context = lambda kind="primitive": context
            typst._create_authoring_text_handle = lambda *args: (_ for _ in ()).throw(
                AssertionError("live Text mutated the pre-execution authoring store")
            )
            label = typst.Text(
                "Late", font="DejaVu Sans Mono", font_size=36,
                line_spacing=0.25, color=noon.BLUE, opacity=0.4,
            )
            assert label._canonical_live_target_context is context
            assert len(context.calls) == 1
            kind, args = context.calls[0]
            assert kind == "text"
            source, family, size, spacing, red, green, blue, alpha, opacity = args
            assert (source, family, size, spacing) == ("Late", "DejaVu Sans Mono", 36.0, 0.25)
            assert (red, green, blue, alpha) == (
                noon.BLUE.red, noon.BLUE.green, noon.BLUE.blue, noon.BLUE.alpha,
            )
            assert opacity == 0.4
            label_clone = label.copy()
            assert type(label_clone) is type(label)
            assert label_clone._source == label._source
            assert label_clone._font == label._font
            assert label_clone._font_size == label._font_size
            assert label_clone._line_spacing == label._line_spacing
            assert context.calls[-1][0] == "copy"

            typst._create_authoring_text_handle = lambda *args: (_ for _ in ()).throw(
                AssertionError("transferred Text reached direct authoring")
            )
            typst._create_authoring_typst_handle = lambda *args: (_ for _ in ()).throw(
                AssertionError("live Typst reached direct authoring")
            )
            handles._live_constructor_context = lambda kind="primitive": (_ for _ in ()).throw(
                RuntimeError(f"live {kind} construction is unavailable while execution is transferred")
            )
            try:
                typst.Text("Late")
            except RuntimeError as error:
                assert "while execution is transferred" in str(error)
            else:
                raise AssertionError("transferred Text construction must reject")

            context = LiveContext()
            handles._live_constructor_context = lambda kind="primitive": context
            diagram = typst.Typst(
                "#circle(radius: 1em)", font_size=30, color=noon.BLUE, opacity=0.6
            )
            formula = typst.MathTypst("x^2", font_size=24)
            assert diagram._canonical_live_target_context is context
            assert formula._canonical_live_target_context is context
            assert [kind for kind, _ in context.calls] == ["typst", "typst"]
            assert context.calls[0][1][1] is False
            assert context.calls[1][1][1] is True

            before = len(context.calls)
            clone = diagram.copy()
            assert clone._canonical_live_target_context is context
            assert type(clone) is type(diagram)
            assert clone._source == diagram._source
            assert clone._font_size == diagram._font_size
            assert len(context.calls) == before + 1
            assert context.calls[-1][0] == "copy"
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
