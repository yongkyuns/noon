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
            handles.install()
            import _manim_typst as typst
            import noon

            class LiveContext:
                def __init__(self): self.calls = []
                def liveCreateManimText(self, *args):
                    self.calls.append(args)
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
            source, family, size, spacing, red, green, blue, alpha, opacity = context.calls[0]
            assert (source, family, size, spacing) == ("Late", "DejaVu Sans Mono", 36.0, 0.25)
            assert (red, green, blue, alpha) == (
                noon.BLUE.red, noon.BLUE.green, noon.BLUE.blue, noon.BLUE.alpha,
            )
            assert opacity == 0.4

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

            handles._live_constructor_context = lambda kind="primitive": LiveContext()
            try:
                typst.Typst("#circle(radius: 1em)")
            except NotImplementedError as error:
                assert "Typst construction after live" in str(error)
            else:
                raise AssertionError("live Typst construction must reject before mutation")
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
