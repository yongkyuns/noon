"""Bounded, JSON-safe exception facts for the authoring host boundary (#2047).

This is exception projection, not error classification or recovery policy. The
caller supplies the exact executed source, virtual filename and revision. User source
is not registered in linecache, no locals are captured, and the snapshot retains no
exception, traceback, engine handle or JS proxy. The host must still propagate
its original failure and retire fatal engines through its existing fatal path.

Locations use one-based lines and zero-based UTF-16 columns (exclusive ends),
matching the browser editor. Unknown or invalid columns remain unavailable;
filename matches are exact, never basename or traceback-message heuristics.
"""
from __future__ import annotations

import traceback

from _noon_errors import NoonError, NoonErrorCause

DIAGNOSTIC_VERSION = 1
MAX_EXCEPTIONS = 8
MAX_FRAMES = 16
MAX_ENGINE_CAUSES = 8
MAX_NOTES = 4
PHASES = frozenset({"startup", "dependencies", "compile", "construct", "callback", "runtime"})


def _utf16_length(text: str) -> int:
    return len(text.encode("utf-16-le", errors="surrogatepass")) // 2


class _Capture:
    def __init__(self, source: str, filename: str):
        self.lines = source.replace("\r\n", "\n").replace("\r", "\n").split("\n")
        self.filename = filename
        self.remaining = MAX_EXCEPTIONS
        self.active: set[int] = set()
        self.truncated = False

    def text(self, value: str, limit: int) -> str:
        # Limits use UTF-16 units on both sides of the worker boundary. Do not
        # split a non-BMP character or scan an arbitrarily long error message.
        if not isinstance(value, str):
            self.truncated = True
            return "<unavailable>"
        units = 0
        end = 0
        for char in value[:limit]:
            width = 2 if ord(char) > 0xFFFF else 1
            if units + width > limit:
                break
            units += width
            end += 1
        self.truncated |= end < len(value)
        return value[:end]

    def message(self, error: BaseException) -> str:
        try:
            value = str(error)
        except BaseException:
            # A broken __str__ must not replace the failure being reported.
            value = "<exception message unavailable>"
            self.truncated = True
        return self.text(value, 2048)

    def column(self, line: int, offset: int | None, *, utf8: bool) -> int | None:
        if type(offset) is not int or offset < 0:
            return None
        text = self.lines[line - 1]
        if utf8:
            encoded = text.encode("utf-8", errors="surrogatepass")
            if offset > len(encoded):
                return None
            try:
                prefix = encoded[:offset].decode("utf-8")
            except UnicodeDecodeError:
                return None
        else:
            if offset > len(text):
                return None
            prefix = text[:offset]
        return _utf16_length(prefix)

    def location(self, line, column, end_line, end_column, *, utf8):
        if type(line) is not int or not 1 <= line <= len(self.lines):
            return None
        start = self.column(line, column, utf8=utf8)
        end = None
        if type(end_line) is int and line <= end_line <= len(self.lines):
            end = self.column(end_line, end_column, utf8=utf8)
        if start is None or end is None or (end_line == line and end < start):
            end_line = end = None
        return {"line": line, "column": start, "endLine": end_line, "endColumn": end}

    def engine(self, error):
        if not isinstance(error, NoonError):
            return None
        if not all(isinstance(value, str) for value in (error.category, error.code)):
            self.truncated = True
            return None
        causes = []
        seen = set()
        cause = error.rust_cause
        while cause is not None:
            if (not isinstance(cause, NoonErrorCause) or id(cause) in seen
                    or len(causes) == MAX_ENGINE_CAUSES
                    or not all(isinstance(value, str)
                               for value in (cause.category, cause.code, cause.message))):
                self.truncated = True
                break
            seen.add(id(cause))
            causes.append({"category": self.text(cause.category, 128),
                           "code": self.text(cause.code, 128),
                           "message": self.text(cause.message, 512)})
            cause = cause.cause
        operation = error.operation
        if operation is not None and not isinstance(operation, str):
            self.truncated = True
            operation = None
        return {"category": self.text(error.category, 128),
                "code": self.text(error.code, 128),
                "operation": None if operation is None else self.text(operation, 256),
                "causes": causes}

    def exception(self, error):
        if self.remaining == 0 or id(error) in self.active:
            self.truncated = True
            return None
        self.remaining -= 1
        self.active.add(id(error))
        try:
            # Capture the original traceback with a benign exception value so
            # traceback does not recursively expand arbitrary exception groups
            # or causes. Those are traversed with our shared node budget below.
            stack = traceback.TracebackException(
                Exception, None, error.__traceback__, limit=-(MAX_FRAMES + 1),
                lookup_lines=False, capture_locals=False,
            ).stack
            self.truncated |= len(stack) > MAX_FRAMES
            frames = []
            location = None
            for frame in stack[-MAX_FRAMES:]:
                user_source = frame.filename == self.filename
                frames.append({"filename": self.text(frame.filename, 512),
                               "function": self.text(frame.name, 128),
                               "line": frame.lineno if type(frame.lineno) is int and frame.lineno > 0 else None,
                               "userSource": user_source})
                if user_source:
                    location = self.location(frame.lineno, frame.colno, frame.end_lineno,
                                             frame.end_colno, utf8=True)
            if isinstance(error, SyntaxError) and error.filename == self.filename:
                column = error.offset - 1 if type(error.offset) is int else None
                end = error.end_offset - 1 if type(error.end_offset) is int else None
                location = self.location(error.lineno, column, error.end_lineno, end,
                                         utf8=False)
            notes = getattr(error, "__notes__", ())
            if not isinstance(notes, (list, tuple)):
                notes = ()
                self.truncated = True
            self.truncated |= len(notes) > MAX_NOTES
            note_text = []
            for note in notes[:MAX_NOTES]:
                if isinstance(note, str):
                    note_text.append(self.text(note, 512))
                else:
                    self.truncated = True
            node = {"type": self.text(type(error).__qualname__, 128),
                    "module": self.text(type(error).__module__, 128),
                    "message": self.message(error), "location": location,
                    "frames": frames, "engine": self.engine(error), "notes": note_text,
                    "cause": None, "causeRelation": None, "children": []}
            cause, relation = error.__cause__, "cause"
            if cause is None and not error.__suppress_context__:
                cause, relation = error.__context__, "context"
            if cause is None and isinstance(error, NoonError):
                # Shared-host callback projection (#2044) may retain its actual
                # Python exception independently of transport-specific chaining.
                cause, relation = getattr(error, "python_cause", None), "callback"
            if isinstance(cause, BaseException):
                node["cause"] = self.exception(cause)
                if node["cause"] is not None:
                    node["causeRelation"] = relation
            if isinstance(error, BaseExceptionGroup):
                for child in error.exceptions:
                    snapshot = self.exception(child)
                    if snapshot is None:
                        if self.remaining == 0:
                            break
                    else:
                        node["children"].append(snapshot)
            return node
        finally:
            self.active.remove(id(error))


def capture_exception(error: BaseException, *, source: str, filename: str,
                      revision: str, phase: str) -> dict:
    """Copy exception facts; the caller, not this helper, owns the run identity.

    Call while handling the original exception. Full raw technical traceback
    may be retained separately by the host; `truncated` explicitly reports any
    omitted snapshot text, frames, causes or group members. There is no global
    last-error mailbox, scene cache, source execution or recovery decision here.
    """
    if not isinstance(error, BaseException):
        raise TypeError("error must be a Python exception")
    if not isinstance(source, str):
        raise TypeError("source must be the executed Python source string")
    for name, value, limit in (("filename", filename, 512), ("revision", revision, 128)):
        if not isinstance(value, str) or not value or _utf16_length(value) > limit:
            raise ValueError(f"{name} must be a non-empty string of at most {limit} UTF-16 units")
    if phase not in PHASES:
        raise ValueError("unknown authoring diagnostic phase")
    capture = _Capture(source, filename)
    exception = capture.exception(error)
    return {"version": DIAGNOSTIC_VERSION, "phase": phase,
            "document": {"filename": filename, "revision": revision},
            "exception": exception, "truncated": capture.truncated}


class source_diagnostic_scope:
    """Observe an escaping source error without changing propagation or ownership.

    The host supplies a per-invocation string sink, not a global last-error
    mailbox. The sink receives JSON only; no exception or PyProxy escapes.
    Successful execution performs no traceback capture. This scope can span
    a host await, but does not rewrite authored code or decide engine recovery.
    """

    def __init__(self, *, source, filename, revision, phase, on_error):
        self.source = source
        self.filename = filename
        self.revision = revision
        self.phase = phase
        self.on_error = on_error

    def __enter__(self):
        return self

    def __exit__(self, exception_type, error, tb):
        try:
            if error is not None:
                try:
                    import json
                    self.on_error(json.dumps(capture_exception(
                        error, source=self.source, filename=self.filename,
                        revision=self.revision, phase=self.phase,
                    ), ensure_ascii=True))
                except BaseException:
                    # Diagnostics must never replace the original failure,
                    # including when exception formatting or the sink fails.
                    pass
        finally:
            # A retained context manager must not keep source or host proxies.
            self.source = self.on_error = None
        return False
