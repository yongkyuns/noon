"""Project Rust-owned failure metadata into Python exceptions.

This module performs no admission, handle validation, lifecycle transitions or
message classification. Unmarked exceptions retain their original identity.
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import NoReturn


@dataclass(frozen=True)
class NoonErrorCause:
    category: str
    code: str
    message: str
    cause: NoonErrorCause | None = None


class NoonError(Exception):
    """A shared-engine failure, retaining the exact original JS exception."""

    def __init__(self, diagnostic: NoonErrorCause, js_error: object, operation: str | None):
        super().__init__(diagnostic.message)
        self.category = diagnostic.category
        self.code = diagnostic.code
        self.rust_cause = diagnostic.cause
        self.js_error = js_error
        self.operation = operation


class NoonValueError(NoonError, ValueError):
    pass


class NoonIndexError(NoonValueError, IndexError):
    """A structured invalid-input failure that also satisfies Python index semantics."""


class NoonForeignHandleError(NoonError, ValueError):
    pass


class NoonStaleHandleError(NoonError, ReferenceError):
    pass


class NoonMissingResourceError(NoonError, LookupError):
    pass


class NoonUnsupportedError(NoonError, NotImplementedError):
    pass


class NoonPendingError(NoonError, RuntimeError):
    pass


class NoonStalePublicationError(NoonError, RuntimeError):
    pass


class NoonCallbackError(NoonError, RuntimeError):
    pass


class NoonOwnershipError(NoonError, RuntimeError):
    def take_player(self):
        """Consume the original Rust rejection and recover its exact player.

        Rust's existing takePlayer operation remains the only ownership authority;
        Python neither copies the player nor tracks an independent lease state.
        """
        return self.js_error.takePlayer()


_EXCEPTION_TYPES = {
    "invalid_input": NoonValueError,
    "foreign_handle": NoonForeignHandleError,
    "stale_handle": NoonStaleHandleError,
    "missing_resource": NoonMissingResourceError,
    "unsupported_operation": NoonUnsupportedError,
    "pending_work": NoonPendingError,
    "stale_publication": NoonStalePublicationError,
    "callback_failure": NoonCallbackError,
    "ownership": NoonOwnershipError,
}

_CODE_EXCEPTION_TYPES = {
    "authoring.invalid_submobject_index": NoonIndexError,
}


def _diagnostic(value: object) -> NoonErrorCause | None:
    if getattr(value, "noonErrorVersion", None) != 1:
        return None
    category = getattr(value, "category", None)
    code = getattr(value, "code", None)
    message = getattr(value, "message", None)
    if not all(isinstance(field, str) for field in (category, code, message)):
        return None
    cause = getattr(value, "cause", None)
    return NoonErrorCause(category, code, message, _diagnostic(cause))


def map_engine_error(error: Exception, *, operation: str | None = None) -> Exception:
    """Map only the typed Noon boundary. Unknown/ordinary Python errors pass through."""
    if isinstance(error, NoonError):
        return error
    original = getattr(error, "js_error", None)
    diagnostic = _diagnostic(original)
    if diagnostic is None:
        return error
    exception_type = _CODE_EXCEPTION_TYPES.get(
        diagnostic.code,
        _EXCEPTION_TYPES.get(diagnostic.category, NoonError),
    )
    return exception_type(diagnostic, original, operation)


def raise_engine_error(error: Exception, *, operation: str | None = None) -> NoReturn:
    mapped = map_engine_error(error, operation=operation)
    if mapped is error:
        raise error
    raise mapped from error


def engine_call(function, *args, operation: str | None = None):
    """Explicit callsite adapter; successful calls and their return values are unchanged."""
    try:
        return function(*args)
    except Exception as error:
        raise_engine_error(error, operation=operation)


async def engine_await(awaitable, *, operation: str | None = None):
    """Preserve typed failures on the existing portable continuation boundary."""
    try:
        return await awaitable
    except Exception as error:
        raise_engine_error(error, operation=operation)
