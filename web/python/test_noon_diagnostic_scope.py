"""Exception interception at the actual host-language boundary (#2047)."""
import asyncio
import gc
import json
import unittest
import weakref
from unittest.mock import patch

from _noon_diagnostics import source_diagnostic_scope
from _manim_source_execution import compile_authoring_source, execute_authoring_module

DOCUMENT = {"filename": "noon-s1-r7.py", "revision": "s1-r7"}


def scope(source, sink, phase="construct"):
    return source_diagnostic_scope(source=source, phase=phase, on_error=sink, **DOCUMENT)


class SourceDiagnosticScopeTests(unittest.IsolatedAsyncioTestCase):
    def test_success_never_captures_or_emits(self):
        emitted = []
        with patch("_noon_diagnostics.capture_exception", side_effect=AssertionError("capture on success")):
            with scope("answer = 42", emitted.append):
                pass
        self.assertEqual(emitted, [])

    def test_exception_identity_cause_and_traceback_preserved(self):
        emitted = []
        failure = ValueError("actual failure")
        cause = RuntimeError("original cause")
        try:
            with scope("", emitted.append):
                raise failure from cause
        except ValueError as seen:
            self.assertIs(seen, failure)
            self.assertIs(seen.__cause__, cause)
            self.assertEqual(seen.__traceback__.tb_frame.f_code.co_name,
                             "test_exception_identity_cause_and_traceback_preserved")
        else:
            self.fail("exception was swallowed")
        self.assertEqual(len(emitted), 1)
        self.assertIsInstance(emitted[0], str)
        self.assertEqual(json.loads(emitted[0])["exception"]["type"], "ValueError")

    def test_failed_sink_does_not_replace_original_error(self):
        def broken_sink(value):
            raise RuntimeError("sink failed")
        failure = ValueError("original")
        with self.assertRaises(ValueError) as raised:
            with scope("", broken_sink):
                raise failure
        self.assertIs(raised.exception, failure)
        self.assertIsNone(failure.__cause__)
        self.assertIsNone(failure.__context__)

    def test_failed_capture_does_not_replace_original_error(self):
        failure = ValueError("original")
        with patch("_noon_diagnostics.capture_exception", side_effect=RuntimeError("capture failed")):
            with self.assertRaises(ValueError) as raised:
                with scope("", lambda _: self.fail("sink must not run")):
                    raise failure
        self.assertIs(raised.exception, failure)

    def test_base_exceptions_are_never_absorbed(self):
        for kind in (KeyboardInterrupt, SystemExit, asyncio.CancelledError):
            failure = kind("stop")
            emitted = []
            with self.subTest(kind=kind), self.assertRaises(kind) as raised:
                with scope("", emitted.append):
                    raise failure
            self.assertIs(raised.exception, failure)
            self.assertEqual(len(emitted), 1)

    def test_scope_does_not_retain_source_or_sink_after_exit(self):
        class Sink:
            def __call__(self, value):
                pass
        sink = Sink()
        ref = weakref.ref(sink)
        observer = scope("private source", sink)
        with observer:
            pass
        del sink
        gc.collect()
        self.assertIsNone(ref())
        self.assertIsNone(observer.source)
        self.assertIsNone(observer.on_error)

    def test_scope_releases_source_after_failure(self):
        observer = scope("private source", lambda value: None)
        with self.assertRaises(ValueError):
            with observer:
                raise ValueError()
        self.assertIsNone(observer.source)
        self.assertIsNone(observer.on_error)

    def test_exact_compiler_syntax_error(self):
        source = "label = '😀한글'; result = (1 + )\n"
        emitted = []
        with self.assertRaises(SyntaxError):
            with scope(source, emitted.append, "compile"):
                compile_authoring_source(source, filename=DOCUMENT["filename"])
        diagnostic = json.loads(emitted[0])
        self.assertEqual(diagnostic["document"], DOCUMENT)
        self.assertEqual(diagnostic["phase"], "compile")
        self.assertEqual(diagnostic["exception"]["location"]["line"], 1)
        self.assertEqual(diagnostic["exception"]["location"]["column"],
                         len(source[:source.index(")")].encode("utf-16-le")) // 2)

    async def test_compile_then_execute_once(self):
        source = "events.append('executed')\nresult = 42\n"
        emitted = []
        events = []
        with scope(source, emitted.append, "compile"):
            code, pairs = compile_authoring_source(source, filename=DOCUMENT["filename"])
        self.assertEqual(events, [])
        namespace = {"events": events}
        with scope(source, emitted.append):
            await execute_authoring_module(code, namespace)
        self.assertEqual(namespace["result"], 42)
        self.assertEqual(events, ["executed"])
        self.assertEqual(emitted, [])

    async def test_error_after_suspension_retains_original_source(self):
        source = "import asyncio\nawait asyncio.sleep(0)\nlabel = '😀'; missing\n"
        code, _ = compile_authoring_source(source, filename=DOCUMENT["filename"])
        emitted = []
        with self.assertRaises(NameError):
            with scope(source, emitted.append):
                await execute_authoring_module(code, {})
        diagnostic = json.loads(emitted[0])
        self.assertEqual(diagnostic["exception"]["location"]["line"], 3)
        self.assertEqual(diagnostic["exception"]["type"], "NameError")
        self.assertEqual(diagnostic["document"], DOCUMENT)

    async def test_simultaneous_scopes_do_not_share_last_error(self):
        results = [[], []]
        async def run(index):
            with self.assertRaises(ValueError):
                with source_diagnostic_scope(source="", filename="scene.py", revision=str(index),
                                             phase="construct", on_error=results[index].append):
                    await asyncio.sleep(0)
                    raise ValueError(str(index))
        await asyncio.gather(run(0), run(1))
        for index in (0, 1):
            captured = json.loads(results[index][0])
            self.assertEqual(captured["document"]["revision"], str(index))
            self.assertEqual(captured["exception"]["message"], str(index))

    async def test_failure_then_success_has_no_stale_diagnostic(self):
        for source, fails in [("missing\n", True), ("result = 7\n", False)]:
            emitted = []
            code, _ = compile_authoring_source(source, filename=DOCUMENT["filename"])
            try:
                with scope(source, emitted.append):
                    await execute_authoring_module(code, {})
            except NameError:
                self.assertTrue(fails)
            self.assertEqual(len(emitted), int(fails))

    async def test_deferred_function_points_at_definition_source(self):
        source = "def callback():\n    return missing\ncallback()\n"
        code, _ = compile_authoring_source(source, filename=DOCUMENT["filename"])
        emitted = []
        with self.assertRaises(NameError):
            with scope(source, emitted.append):
                await execute_authoring_module(code, {})
        self.assertEqual(json.loads(emitted[0])["exception"]["location"]["line"], 2)

    def test_group_keeps_original_identity(self):
        failure = ExceptionGroup("multiple failures", [ValueError("x"), TypeError("y")])
        emitted = []
        with self.assertRaises(ExceptionGroup) as raised:
            with scope("", emitted.append):
                raise failure
        self.assertIs(raised.exception, failure)
        self.assertEqual(len(json.loads(emitted[0])["exception"]["children"]), 2)


if __name__ == "__main__":
    unittest.main()
