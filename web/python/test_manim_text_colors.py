import os
from pathlib import Path
import subprocess
import sys
import textwrap
import unittest


class TextColorConstructorTests(unittest.TestCase):
    def test_color_arguments_ownership_and_cold_live_equivalence(self):
        source = textwrap.dedent('''
            import noon
            import _manim_typst as text

            batches = []
            class Batch:
                def __init__(self):
                    self.values = []
                    self.freed = False
                    self.consumed = False
                    batches.append(self)
                def push(self, *args):
                    if args[0] == "push failure":
                        raise ValueError("push failed")
                    self.values.append(args)
                def setBaseColor(self, *args):
                    self.base_color = args
                def free(self):
                    assert not self.consumed
                    self.freed = True
            class Handle:
                def setColor(self, *args): pass
                def setObjectOpacity(self, *args): pass
            calls = []
            def construct(*args):
                batch = args[-1]
                batch.consumed = True
                calls.append(args)
                if args[0] == "constructor failure":
                    raise ValueError("Rust rejected selector")
                return Handle()
            text._new_text_color_batch = Batch
            text._create_authoring_text_handle = construct
            text._live_text_context = lambda: None
            cold = text.Text("é é", color=noon.BLUE, t2c={"ignored": noon.RED}, text2color={"é": "#FF0000", "[-1:]": noon.BLUE})
            assert batches[-1].values == [
                ("é", 1., 0., 0., 1.),
                ("[-1:]", noon.BLUE.red, noon.BLUE.green, noon.BLUE.blue, noon.BLUE.alpha),
            ]
            assert batches[-1].consumed and not batches[-1].freed
            assert batches[-1].base_color == (noon.BLUE.red, noon.BLUE.green, noon.BLUE.blue, noon.BLUE.alpha)
            assert calls[-1][:4] == ("é é", "DejaVu Sans Mono", 48., -1.)
            cold_values = batches[-1].values

            class Live:
                liveCreateManimText = staticmethod(construct)
            text._live_text_context = lambda: Live()
            live = text.Text("é é", color=noon.BLUE, t2c={"é": "#FF0000", "[-1:]": noon.BLUE})
            assert batches[-1].values == cold_values
            assert calls[-1][:4] == calls[-2][:4]
            assert len(calls[-1]) == 10

            before = len(batches)
            for options in ({"t2c": []}, {"t2c": {42: noon.RED}}, {"t2c": {"x": object()}}):
                try:
                    text.Text("x", **options)
                except (TypeError, ValueError): pass
                else: raise AssertionError("invalid mapping accepted")
            assert len(batches) == before
            try:
                text.Text("x", t2c={"push failure": noon.RED})
            except ValueError: pass
            else: raise AssertionError("failed push accepted")
            assert batches[-1].freed and not batches[-1].consumed
            try:
                text.Text("constructor failure", t2c={"x": noon.RED})
            except ValueError: pass
            else: raise AssertionError("failed constructor accepted")
            assert batches[-1].consumed and not batches[-1].freed
        ''')
        directory = Path(__file__).resolve().parent
        result = subprocess.run(
            [sys.executable, "-c", source], cwd=directory,
            env={**os.environ, "PYTHONPATH": str(directory)},
            capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
