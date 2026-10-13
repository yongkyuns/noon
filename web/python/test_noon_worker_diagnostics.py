"""Execute the production worker's Python body under a minimal host, not Pyodide.

These tests exercise real compilation, traceback capture and failure→retry.
Real worker/renderer/browser qualification remains in the recovery smoke test.
"""
import ast
import inspect
import json
import re
import sys
import types
import unittest
from pathlib import Path
from unittest.mock import patch

from _manim_source_execution import bind_portable_construct

WORKER = Path(__file__).resolve().parents[1] / "python-worker.source.js"
BODY = re.search(r"async function runAuthoringSource\([^`]+`\n(.*?)\n`", WORKER.read_text(), re.S).group(1)
DOCUMENT = {"filename": "scene.py", "revision": "run-1"}


class WorkerDiagnosticsTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.events = []
        self.original = ValueError("original host exception")
        events = self.events
        original = self.original
        class Scene:
            def construct(self):
                pass
        def explode():
            raise original
        noon = types.ModuleType("noon")
        noon.Scene = Scene
        noon.explode = explode
        async def execute_construct(scene, *, portable_constructs):
            method = bind_portable_construct(scene.construct, portable_constructs) or scene.construct
            value = method()
            if inspect.isawaitable(value):
                await value
        async def barrier(method, *args, **kwargs):
            return method(*args, **kwargs)
        class Context:
            def liveHandoffDuration(self):
                return 0
            def authoredDuration(self):
                return 0
        scene = types.ModuleType("_manim_scene")
        scene.execute_construct = execute_construct
        scene.await_source_barrier = barrier
        scene.await_module_source_barrier = barrier
        scene.execution_context = lambda _scene: Context()
        updaters = types.ModuleType("_manim_updaters")
        updaters.canonical_callback_session_id = lambda _scene: None
        js = types.ModuleType("js")
        def register(_context):
            events.append("publish")
            return "semantic-1"
        js.noonRegisterSemanticExecution = register
        js.noonSemanticContinuationGeneration = lambda _context: None
        self.modules = patch.dict(sys.modules, {"noon": noon, "_manim_scene": scene,
                                               "_manim_updaters": updaters, "js": js})
        self.modules.start()
        self.addCleanup(self.modules.stop)

    async def execute(self, source, *, dependency_failure=False, revision="run-1"):
        async def dependencies():
            self.events.append("dependencies")
            if dependency_failure:
                raise ConnectionError("resource download failed")
        namespace = {"__noon_source": source, "__noon_context_json": "{}",
                     "__noon_source_document_json": json.dumps({**DOCUMENT, "revision": revision}),
                     "__noon_prepare_dependencies": dependencies}
        caught = None
        try:
            await eval(compile(BODY, "<noon-host>", "exec", flags=ast.PyCF_ALLOW_TOP_LEVEL_AWAIT), namespace)
        except BaseException as error:
            caught = error
        raw = namespace.get("__noon_diagnostic_json")
        return caught, json.loads(raw) if raw else None, namespace

    async def test_syntax_failure_is_captured_before_dependency_loading(self):
        error, diagnostic, _ = await self.execute("class Broken(:\n    pass\n")
        self.assertIsInstance(error, SyntaxError)
        self.assertEqual(diagnostic["phase"], "compile")
        self.assertEqual(diagnostic["document"], DOCUMENT)
        self.assertEqual(diagnostic["exception"]["location"]["line"], 1)
        self.assertEqual(self.events, [])

    async def test_unicode_user_expression_not_host_wrapper_is_reported(self):
        source = "label = '😀한글'; value = missing\n"
        error, diagnostic, _ = await self.execute(source)
        self.assertIsInstance(error, NameError)
        position = diagnostic["exception"]["location"]
        expected = len(source[:source.index("missing")].encode("utf-16-le")) // 2
        self.assertEqual(position["column"], expected)
        self.assertEqual(diagnostic["phase"], "construct")
        self.assertEqual(self.events, ["dependencies"])

    async def test_construct_exception_has_correct_line(self):
        source = "from noon import Scene\nclass Demo(Scene):\n    def construct(self):\n        missing\n"
        _, diagnostic, _ = await self.execute(source)
        self.assertEqual(diagnostic["exception"]["location"]["line"], 4)

    async def test_original_exception_object_is_not_replaced(self):
        error, diagnostic, _ = await self.execute("from noon import explode\nexplode()\n")
        self.assertIs(error, self.original)
        self.assertEqual(diagnostic["exception"]["message"], str(self.original))
        self.assertEqual(diagnostic["exception"]["location"]["line"], 2)

    async def test_capture_failure_does_not_replace_original_exception(self):
        with patch("_noon_diagnostics.capture_exception", side_effect=RuntimeError("diagnostics failed")):
            error, diagnostic, _ = await self.execute("from noon import explode\nexplode()\n")
        self.assertIs(error, self.original)
        self.assertIsNone(diagnostic)

    async def test_failed_then_corrected_source_has_no_stale_diagnostic(self):
        _, first, _ = await self.execute("missing\n")
        error, second, namespace = await self.execute("from noon import Scene\nresult = Scene()\n", revision="run-2")
        self.assertIsNotNone(first)
        self.assertIsNone(error)
        self.assertIsNone(second)
        self.assertEqual(json.loads(namespace["__noon_result_json"])["kind"], "semantic_scene")
        self.assertEqual(self.events.count("publish"), 1)

    async def test_dependency_failure_has_dependency_phase_not_guessed_user_blame(self):
        error, diagnostic, _ = await self.execute("from noon import Scene\nresult = Scene()\n", dependency_failure=True)
        self.assertIsInstance(error, ConnectionError)
        self.assertEqual(diagnostic["phase"], "dependencies")
        self.assertIsNone(diagnostic["exception"]["location"])
        self.assertNotIn("publish", self.events)

    async def test_source_side_effects_execute_once(self):
        error, diagnostic, namespace = await self.execute("from noon import Scene\ncount = globals().get('count', 0) + 1\nresult = Scene()\n")
        self.assertIsNone(error)
        self.assertIsNone(diagnostic)
        self.assertEqual(namespace["__noon_namespace"]["count"], 1)
        self.assertEqual(self.events, ["dependencies", "publish"])

    async def test_typed_engine_failure_keeps_its_metadata(self):
        source = "from _noon_errors import NoonUnsupportedError, NoonErrorCause\nraise NoonUnsupportedError(NoonErrorCause('unsupported_operation', 'test.code', 'bounded test'), None, 'Test.operation')\n"
        _, diagnostic, _ = await self.execute(source)
        self.assertEqual(diagnostic["exception"]["engine"]["code"], "test.code")
        self.assertEqual(diagnostic["exception"]["engine"]["operation"], "Test.operation")


if __name__ == "__main__":
    unittest.main()
