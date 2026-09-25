import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimLiveTextConstructorTests(unittest.TestCase):

    def test_latex_routes_raw_source_and_presentation_in_one_constructor(self):
        source = r"""
from types import SimpleNamespace
import _manim_latex as latex
import noon
assert noon.Tex is latex.Tex and noon.MathTex is latex.MathTex
calls = []
def latex_parts(args, source):
    return SimpleNamespace(source=source, family=lambda: object(), members=lambda: ())
latex._create = lambda *args: calls.append(args) or latex_parts(args, args[0])
latex._create_strings = lambda *args: calls.append(args) or latex_parts(args, r"x{{+}}y  z ")
latex._live_text_context = lambda: None
cold = noon.MathTex(r"\frac{1}{2}", font_size=32, color=noon.BLUE, opacity=0.4)
assert cold.source == r"\frac{1}{2}"
assert calls[0][:3] == (r"\frac{1}{2}", True, 32.0)
assert calls[0][-3:] == (0.4, [], None)
parts = noon.MathTex(r"x{{+}}y", r"{{ z }}")
assert parts.source == r"x{{+}}y  z "
assert calls[-1][:3] == ([r"x{{+}}y", r"{{ z }}"], True, 48.0)
context = object()
latex._live_text_context = lambda: context
live = noon.Tex("raw % source")
assert live._canonical_live_target_context is context
assert calls[-1][0:2] == ("raw % source", False)
assert calls[-1][-1] is context
before = len(calls)
for kwargs in (
    {"font_size": 0}, {"opacity": 2}, {"unknown_option": 1},
):
    try: noon.Tex("invalid", **kwargs)
    except (ValueError, NotImplementedError): pass
    else: raise AssertionError("invalid options accepted")
assert len(calls) == before
isolated = noon.MathTex("x+x", substrings_to_isolate=(value for value in ("x", "+")))
assert isolated.source == "x+x"
assert calls[-1][-2] == ["x", "+"]
failure = RuntimeError("compiler rejected input")
def rejected(*args): raise failure
latex._create = rejected
try: noon.MathTex("bad")
except RuntimeError as error: assert error is failure
else: raise AssertionError("compiler error was swallowed")
"""
        result = subprocess.run(
            [sys.executable, "-c", source], cwd=Path(__file__).resolve().parent,
            capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_markup_text_selects_raw_source_cold_and_live_routes(self) -> None:
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
            import _manim_semantic_handles as handles
            import _typed_geometry_test_support as _geometry_test
            _geometry_test.install_module_bridge(handles, lambda *args: object())
            import _manim_typst as typst
            import noon

            assert noon.MarkupText is typst.MarkupText
            cold_calls = []
            class Handle:
                def setColor(self, *args): pass
                def setOpacity(self, *args): pass
                def setObjectOpacity(self, *args): pass
            typst._create_authoring_markup_text_handle = lambda *args: cold_calls.append(args) or Handle()
            typst._create_authoring_text_handle = lambda *args: (_ for _ in ()).throw(
                AssertionError("MarkupText used the plain Text constructor")
            )
            handles._live_constructor_context = lambda kind="primitive": None
            cold = typst.MarkupText("<b>é</b>", font="DejaVu Sans Mono", font_size=31, line_spacing=0.2)
            assert cold.source == "<b>é</b>"
            assert cold_calls == [("<b>é</b>", "DejaVu Sans Mono", 31.0, 0.2)]
            for kwargs in ({"font_size": 0}, {"line_spacing": -2}):
                try:
                    typst.MarkupText("<i>x</i>", **kwargs)
                except ValueError:
                    pass
                else:
                    raise AssertionError("invalid MarkupText options were accepted")
            assert len(cold_calls) == 1

            class LiveContext:
                def __init__(self): self.calls = []
                def liveExecutionOwnership(self): return "returned"
                def liveCreateManimMarkupText(self, *args):
                    self.calls.append(args)
                    return object()
                def liveCreateManimText(self, *args):
                    raise AssertionError("MarkupText used the plain live Text constructor")

            context = LiveContext()
            handles._live_constructor_context = lambda kind="primitive": context
            live = typst.MarkupText("<span>raw & source</span>", color=noon.BLUE, opacity=0.4)
            assert live.source == "<span>raw & source</span>"
            assert len(context.calls) == 1
            assert context.calls[0][:4] == ("<span>raw & source</span>", "DejaVu Sans Mono", 48.0, -1.0)
            assert context.calls[0][-1] == 0.4
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

class ManimLatexPartTransportTests(unittest.TestCase):
    def test_source_selection_and_colors_route_through_shared_family_handle(self):
        source = r'''
from types import SimpleNamespace
import _manim_latex as latex
import noon
calls = []
class Raw:
    sourceStart=0; sourceEnd=1; firstCluster=0; clusterCount=1
    firstVector=0; vectorCount=0; semanticKey="part:x"
    def free(self): pass
class RawList:
    length=1
    def item(self, index): return Raw()
    def free(self): pass
class Member:
    semanticSlot=7; semanticGeneration=3
    def textSource(self): return "x"
    def textParts(self): return RawList()
class Parts:
    source="x"
    fontSize=72.0
    def family(self): return object()
    def members(self): return (Member(),)
    def sourceMemberIndicesFor(self, needle):
        calls.append(("select", needle)); return (0,) if needle == "x" else ()
    def setMemberColors(self, values): calls.append(("colors", tuple(values)))
latex._create = lambda *args: Parts()
latex._live_text_context = lambda: None
value = noon.MathTex("x")
assert value.font_size == 72.0
part = value.get_part_by_tex("x")
assert part.get_tex_string() == "x"
value.set_color_by_tex("x", noon.BLUE)
assert calls[0] == ("select", "x")
assert calls[1] == ("select", "x")
kind, values = calls[2]
assert kind == "colors" and values == (noon.BLUE.red, noon.BLUE.green, noon.BLUE.blue, noon.BLUE.alpha)
'''
        result = subprocess.run(
            [sys.executable, "-c", source], cwd=Path(__file__).resolve().parent,
            capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
