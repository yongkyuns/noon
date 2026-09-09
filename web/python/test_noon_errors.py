"""Pure Python adapter tests; real WASM/Pyodide coverage is in the browser smoke."""
import asyncio
from types import SimpleNamespace
import unittest

from _noon_errors import (
    NoonError, NoonForeignHandleError, NoonOwnershipError, NoonPendingError,
    engine_await, engine_call, map_engine_error,
)


def diagnostic(category, code, message="useful diagnostic", cause=None, **extra):
    return SimpleNamespace(noonErrorVersion=1, category=category, code=code,
                           message=message, cause=cause, **extra)


def js_exception(value):
    error = RuntimeError("JS boundary")
    error.js_error = value
    return error


class ErrorProjectionTests(unittest.TestCase):
    def test_python_and_unmarked_errors_preserve_identity(self):
        for error in (ValueError("foreign handle"), RuntimeError("unsupported pending"),
                      js_exception(SimpleNamespace(message="stale handle"))):
            self.assertIs(map_engine_error(error), error)

    def test_only_category_selects_exception_not_diagnostic_wording(self):
        for message in ("unsupported", "pending callback", "unrelated prose"):
            original = diagnostic("foreign_handle", "authoring.foreign_store", message)
            mapped = map_engine_error(js_exception(original))
            self.assertIsInstance(mapped, NoonForeignHandleError)
            self.assertIsInstance(mapped, ValueError)
            self.assertEqual(str(mapped), message)
            self.assertIs(mapped.js_error, original)

    def test_unknown_category_is_not_reclassified(self):
        error = map_engine_error(js_exception(diagnostic("future", "new.code", "foreign")))
        self.assertIs(type(error), NoonError)
        self.assertEqual(error.category, "future")

    def test_causes_and_original_are_preserved_without_consumption(self):
        calls = []
        player = object()
        def take():
            calls.append("take")
            return player
        cause = diagnostic("foreign_handle", "authoring.foreign_store")
        original = diagnostic("ownership", "ownership.foreign_scene", cause=cause, takePlayer=take)
        error = map_engine_error(js_exception(original), operation="returnExecutionPlayer")
        self.assertIsInstance(error, NoonOwnershipError)
        self.assertEqual(error.operation, "returnExecutionPlayer")
        self.assertEqual(error.rust_cause.code, "authoring.foreign_store")
        self.assertEqual(calls, [])
        self.assertIs(error.take_player(), player)
        self.assertEqual(calls, ["take"])
        self.assertIs(map_engine_error(error), error)

    def test_call_preserves_success_and_python_failure(self):
        value = object()
        self.assertIs(engine_call(lambda: value), value)
        original = ValueError("plain Python argument error")
        def fail():
            raise original
        with self.assertRaises(ValueError) as caught:
            engine_call(fail)
        self.assertIs(caught.exception, original)

    def test_call_chains_original_exception(self):
        original = js_exception(diagnostic("pending_work", "publication.segment_pending"))
        def fail():
            raise original
        with self.assertRaises(NoonPendingError) as caught:
            engine_call(fail, operation="Scene.add")
        self.assertIs(caught.exception.__cause__, original)

    def test_await_maps_the_same_contract(self):
        original = js_exception(diagnostic("pending_work", "completion.not_at_boundary"))
        async def fail():
            raise original
        with self.assertRaises(NoonPendingError) as caught:
            asyncio.run(engine_await(fail(), operation="Scene.continuation"))
        self.assertIs(caught.exception.__cause__, original)


if __name__ == "__main__":
    unittest.main()
