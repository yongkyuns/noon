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

    def test_font_face_clone_order_and_color_batch_failure_cleanup(self):
        source = textwrap.dedent('''
            import noon
            import _manim_typst as text

            batches = []
            class Batch:
                def __init__(self):
                    self.freed = False
                    self.consumed = False
                    self.values = []
                    batches.append(self)
                def setBaseColor(self, *args): pass
                def push(self, *args):
                    if args[0] == "push failure": raise ValueError("push failed")
                    self.values.append(args)
                def free(self):
                    assert not self.consumed and not self.freed
                    self.freed = True

            class FaceHandle:
                def __init__(self, should_fail=False):
                    self.should_fail = should_fail
                    self.clones = 0
                    self.wrapper = None
                def cloneFace(self):
                    self.clones += 1
                    if self.should_fail: raise RuntimeError("clone failed")
                    self.wrapper = Transfer()
                    return self.wrapper
            class Transfer:
                def __init__(self): self.freed = False
                def free(self): self.freed = True
            class Handle:
                def setColor(self, *args): pass
                def setOpacity(self, *args): pass
                def setObjectOpacity(self, *args): pass

            text._new_text_color_batch = Batch
            text._live_text_context = lambda: None
            face_handle = FaceHandle()
            face = object.__new__(text.NativeFontFace)
            object.__setattr__(face, "_family", "Test")
            object.__setattr__(face, "_face_index", 0)
            object.__setattr__(face, "_handle", face_handle)
            calls = []
            def construct(*args):
                batch, cloned_face = args[-2:]
                assert batch is batches[-1]
                assert cloned_face is face_handle.wrapper
                batch.consumed = True
                calls.append(args)
                return Handle()
            text._create_authoring_text_handle = construct
            value = text.Text("colored", font=face, t2c={"x": noon.RED})
            assert len(calls) == 1 and face_handle.clones == 1
            assert calls[0][-2] is batches[-1] and calls[0][-1] is face_handle.wrapper
            assert batches[-1].consumed and not batches[-1].freed
            assert not face_handle.wrapper.freed

            live_calls = []
            class Live:
                def liveCreateManimText(self, *args):
                    live_calls.append(args)
                    args[-2].consumed = True
                    return Handle()
            text._live_text_context = lambda: Live()
            live_face = FaceHandle()
            object.__setattr__(face, "_handle", live_face)
            live_value = text.Text("live colored", font=face, t2c={"x": noon.BLUE})
            assert len(live_calls) == 1
            assert live_calls[0][-2] is batches[-1]
            assert live_calls[0][-1] is live_face.wrapper
            assert batches[-1].consumed and not batches[-1].freed
            assert not live_face.wrapper.freed
            text._live_text_context = lambda: None

            push_failure_face = FaceHandle()
            object.__setattr__(face, "_handle", push_failure_face)
            before_calls, before_batches = len(calls), len(batches)
            try: text.Text("push", font=face, t2c={"push failure": noon.RED})
            except ValueError: pass
            else: raise AssertionError("color batch push failure was swallowed")
            assert len(batches) == before_batches + 1
            assert batches[-1].freed and not batches[-1].consumed
            assert push_failure_face.clones == 0 and len(calls) == before_calls

            clone_failure_face = FaceHandle(should_fail=True)
            object.__setattr__(face, "_handle", clone_failure_face)
            before_calls = len(calls)
            try: text.Text("clone", font=face, t2c={"x": noon.RED})
            except RuntimeError as error: assert str(error) == "clone failed"
            else: raise AssertionError("face clone failure was swallowed")
            assert batches[-1].freed and not batches[-1].consumed
            assert clone_failure_face.clones == 1 and len(calls) == before_calls

            constructor_failure_face = FaceHandle()
            object.__setattr__(face, "_handle", constructor_failure_face)
            def reject(*args):
                args[-2].consumed = True
                raise ValueError("constructor rejected")
            text._create_authoring_text_handle = reject
            try: text.Text("constructor", font=face, t2c={"x": noon.RED})
            except ValueError as error: assert str(error) == "constructor rejected"
            else: raise AssertionError("constructor error was swallowed")
            assert batches[-1].consumed and not batches[-1].freed
            assert not constructor_failure_face.wrapper.freed
        ''')
        directory = Path(__file__).resolve().parent
        result = subprocess.run(
            [sys.executable, "-c", source], cwd=directory,
            env={**os.environ, "PYTHONPATH": str(directory)},
            capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
