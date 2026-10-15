"""Exception facts, identity and source-range contract for #2047."""
import gc
import json
import linecache
import unittest
import weakref

from _noon_diagnostics import MAX_EXCEPTIONS, capture_exception
from _noon_errors import NoonCallbackError, NoonErrorCause, NoonUnsupportedError

FILE = "noon-scene-17.py"
REVISION = "selection-4/run-17"


def snapshot(error, source="", filename=FILE, phase="construct"):
    return capture_exception(error, source=source, filename=filename,
                             revision=REVISION, phase=phase)


def failed_source(source, namespace=None, filename=FILE):
    try:
        exec(compile(source, filename, "exec"), namespace or {})
    except BaseException as error:
        return error
    raise AssertionError("fixture must fail")


def utf16(text):
    return len(text.encode("utf-16-le")) // 2


def nodes(error):
    return 1 + (nodes(error["cause"]) if error["cause"] else 0) + sum(
        nodes(child) for child in error["children"]
    )


class DiagnosticTests(unittest.TestCase):
    def test_name_error_has_exact_source_and_no_engine_classification(self):
        source = "answer = missing_name\n"
        diagnostic = snapshot(failed_source(source), source)
        error = diagnostic["exception"]
        self.assertEqual(error["type"], "NameError")
        self.assertEqual(error["module"], "builtins")
        self.assertIsNone(error["engine"])
        self.assertEqual(error["location"], {
            "line": 1, "column": 9, "endLine": 1, "endColumn": 21,
        })
        self.assertEqual(diagnostic["document"], {"filename": FILE, "revision": REVISION})
        self.assertEqual(json.loads(json.dumps(diagnostic)), diagnostic)

    def test_runtime_unicode_byte_positions_become_utf16(self):
        source = "label = '😀한글'; answer = missing\n"
        location = snapshot(failed_source(source), source)["exception"]["location"]
        prefix = source[:source.index("missing")]
        self.assertEqual(location["column"], utf16(prefix))
        self.assertEqual(location["endColumn"], utf16(prefix + "missing"))

    def test_real_syntax_error_unicode_character_positions_become_utf16(self):
        source = "label = '😀한글'; answer = (1 + )\n"
        error = failed_source(source)
        self.assertIsInstance(error, SyntaxError)
        diagnostic = snapshot(error, source, phase="compile")
        self.assertEqual(diagnostic["phase"], "compile")
        self.assertEqual(diagnostic["exception"]["location"]["column"],
                         utf16(source[:source.index(")")]))

    def test_multiline_source(self):
        source = "value = (\n    '😀',\n    missing\n)\n"
        self.assertEqual(snapshot(failed_source(source), source)["exception"]["location"], {
            "line": 3, "column": 4, "endLine": 3, "endColumn": 11,
        })

    def test_crlf_source(self):
        source = "label = '😀'\r\nvalue = missing\r\n"
        location = snapshot(failed_source(source), source)["exception"]["location"]
        self.assertEqual((location["line"], location["column"]), (2, 8))

    def test_tab_is_one_editor_code_unit_not_visual_width(self):
        source = "def f():\n\treturn missing\nf()\n"
        location = snapshot(failed_source(source), source)["exception"]["location"]
        self.assertEqual((location["line"], location["column"]), (2, 8))

    def test_exact_filename_not_same_basename(self):
        source = "missing\n"
        diagnostic = snapshot(failed_source(source, filename="/other/" + FILE), source)
        self.assertIsNone(diagnostic["exception"]["location"])
        self.assertFalse(any(frame["userSource"] for frame in diagnostic["exception"]["frames"]))

    def test_unrelated_syntax_error_has_no_user_location(self):
        error = SyntaxError("bad", ("/other/" + FILE, 1, 1, "x"))
        self.assertIsNone(snapshot(error, "x")["exception"]["location"])

    def test_invalid_line_does_not_guess_a_location(self):
        error = SyntaxError("bad", (FILE, 400, 1, "x"))
        self.assertIsNone(snapshot(error, "x")["exception"]["location"])

    def test_unavailable_column_is_line_only(self):
        error = SyntaxError("bad", (FILE, 1, None, "x"))
        self.assertEqual(snapshot(error, "x")["exception"]["location"], {
            "line": 1, "column": None, "endLine": None, "endColumn": None,
        })

    def test_invalid_column_is_not_clamped_to_an_unrelated_character(self):
        error = SyntaxError("bad", (FILE, 1, 40, "x", 1, 41))
        self.assertIsNone(snapshot(error, "x")["exception"]["location"]["column"])

    def test_engine_category_code_operation_and_causes_are_preserved(self):
        nested = NoonErrorCause("invalid_input", "paint.opacity", "opacity out of range")
        error = NoonUnsupportedError(
            NoonErrorCause("unsupported_operation", "matching.key_map", "unsupported", nested),
            object(), "TransformMatchingShapes",
        )
        result = snapshot(error)["exception"]
        self.assertEqual(result["engine"], {
            "category": "unsupported_operation", "code": "matching.key_map",
            "operation": "TransformMatchingShapes",
            "causes": [{"category": "invalid_input", "code": "paint.opacity",
                        "message": "opacity out of range"}],
        })
        json.dumps(result)  # The opaque original JS/engine object is not transported.

    def test_ordinary_error_attributes_and_wording_do_not_imply_noon_error(self):
        error = TypeError("unsupported_operation: invalid_input: radius must be positive")
        error.category = "unsupported_operation"
        error.code = "fake"
        self.assertIsNone(snapshot(error)["exception"]["engine"])

    def test_explicit_cause(self):
        source = "try:\n    1 / 0\nexcept Exception as cause:\n    raise ValueError('outer') from cause\n"
        error = snapshot(failed_source(source), source)["exception"]
        self.assertEqual(error["causeRelation"], "cause")
        self.assertEqual(error["cause"]["type"], "ZeroDivisionError")
        self.assertEqual(error["cause"]["location"]["line"], 2)

    def test_implicit_context(self):
        source = "try:\n    1 / 0\nexcept Exception:\n    raise ValueError('outer')\n"
        self.assertEqual(snapshot(failed_source(source), source)["exception"]["causeRelation"], "context")

    def test_suppressed_context_stays_suppressed(self):
        source = "try:\n    1 / 0\nexcept Exception:\n    raise ValueError('outer') from None\n"
        self.assertIsNone(snapshot(failed_source(source), source)["exception"]["cause"])

    def test_shared_host_callback_python_cause(self):
        cause = failed_source("missing\n")
        error = NoonCallbackError(
            NoonErrorCause("callback_failure", "python.callback_failed", "callback failed"),
            object(), "Scene.continuation",
        )
        error.python_cause = cause
        captured = snapshot(error, "missing\n")["exception"]
        self.assertEqual(captured["causeRelation"], "callback")
        self.assertEqual(captured["cause"]["location"]["line"], 1)

    def test_cycle_is_bounded_and_reported(self):
        error = ValueError("cycle")
        error.__cause__ = error
        captured = snapshot(error)
        self.assertTrue(captured["truncated"])
        self.assertIsNone(captured["exception"]["cause"])

    def test_rust_cause_cycle_is_bounded_and_reported(self):
        cause = NoonErrorCause("x", "y", "z")
        object.__setattr__(cause, "cause", cause)
        error = NoonUnsupportedError(NoonErrorCause("x", "y", "z", cause), None, None)
        captured = snapshot(error)
        self.assertTrue(captured["truncated"])
        self.assertEqual(len(captured["exception"]["engine"]["causes"]), 1)

    def test_repeated_group_member_is_not_a_cycle(self):
        error = ValueError("same")
        captured = snapshot(ExceptionGroup("group", [error, error]))
        self.assertFalse(captured["truncated"])
        self.assertEqual(len(captured["exception"]["children"]), 2)

    def test_group_and_chain_share_one_budget(self):
        error = ValueError("root")
        for _ in range(20):
            parent = ExceptionGroup("group", [error, ValueError("sibling")])
            error = parent
        captured = snapshot(error)
        self.assertTrue(captured["truncated"])
        self.assertLessEqual(nodes(captured["exception"]), MAX_EXCEPTIONS)
        json.dumps(captured)

    def test_unicode_text_limits_do_not_split_characters(self):
        error = ValueError("😀" * 5000)
        error.add_note("한" * 1000)
        captured = snapshot(error)
        self.assertEqual(utf16(captured["exception"]["message"]), 2048)
        self.assertEqual(len(captured["exception"]["notes"][0]), 512)
        self.assertTrue(captured["truncated"])
        captured["exception"]["message"].encode("utf-8")

    def test_notes_and_frames_are_bounded(self):
        source = "def f(n):\n    return f(n-1) if n else 1/0\nf(60)\n"
        error = failed_source(source)
        for i in range(20):
            error.add_note(str(i))
        captured = snapshot(error, source)
        self.assertTrue(captured["truncated"])
        self.assertEqual(len(captured["exception"]["frames"]), 16)
        self.assertEqual(len(captured["exception"]["notes"]), 4)

    def test_broken_str_does_not_replace_original_failure(self):
        class Broken(Exception):
            def __str__(self):
                raise RuntimeError("formatter crashed")
        captured = snapshot(Broken())
        self.assertTrue(captured["truncated"])
        self.assertEqual(captured["exception"]["message"], "<exception message unavailable>")

    def test_original_exception_traceback_and_notes_are_unchanged(self):
        source = "1 / 0\n"
        error = failed_source(source)
        error.add_note("original note")
        original_traceback = error.__traceback__
        captured = snapshot(error, source)
        self.assertIs(error.__traceback__, original_traceback)
        self.assertEqual(error.__notes__, ["original note"])
        error.add_note("later")
        self.assertEqual(captured["exception"]["notes"], ["original note"])

    def test_snapshot_does_not_keep_local_objects_alive(self):
        class Local:
            def __repr__(self):
                raise AssertionError("locals must not be inspected")
        def capture():
            local = Local()
            ref = weakref.ref(local)
            try:
                raise ValueError("failure")
            except ValueError as error:
                result = snapshot(error)
            return result, ref
        result, ref = capture()
        gc.collect()
        self.assertIsNone(ref())
        json.dumps(result)

    def test_source_is_not_embedded_or_registered_in_linecache(self):
        source = "secret = 'private-user-source-7f15'\nmissing\n"
        linecache.cache.pop(FILE, None)
        captured = snapshot(failed_source(source), source)
        self.assertNotIn("private-user-source-7f15", json.dumps(captured))
        self.assertNotIn(FILE, linecache.cache)

    def test_unavailable_exception_module_does_not_break_snapshot(self):
        custom = type("Custom", (Exception,), {"__module__": None})
        captured = snapshot(custom("message"))
        self.assertEqual(captured["exception"]["module"], "<unavailable>")
        self.assertTrue(captured["truncated"])
        json.dumps(captured)

    def test_invalid_host_identity_is_rejected_not_truncated(self):
        for filename, revision in [("", REVISION), (FILE, ""), (FILE, "😀" * 65)]:
            with self.subTest(filename=filename, revision=revision):
                with self.assertRaises(ValueError):
                    capture_exception(ValueError(), source="", filename=filename,
                                      revision=revision, phase="construct")
        with self.assertRaises(ValueError):
            snapshot(ValueError(), phase="guessed-user-error")
        with self.assertRaises(TypeError):
            snapshot("not an exception")


if __name__ == "__main__":
    unittest.main()
