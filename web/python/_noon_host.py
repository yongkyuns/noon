"""Optional host bindings only; no scene, scheduler, or semantic state.

The public Python implementation is shared unchanged. Browser functions resolve
once at import to the existing JS/WASM bridge; CPython resolves native functions.
"""
from __future__ import annotations
import importlib
import json
import sys
from functools import partial


def _backend():
    # Explicit js fixtures remain usable by existing Python unit tests.
    if sys.platform == "emscripten" or "js" in sys.modules:
        return importlib.import_module("js")
    return importlib.import_module("_noon_native_host")


def __getattr__(name):
    if name.startswith("__"):
        raise AttributeError(name)
    try:
        return getattr(_backend(), name)
    except AttributeError:
        raise AttributeError(f"Noon host does not provide {name}") from None


def encode_callback(value):
    """Serialize only the browser's existing bridge; native values stay typed."""
    if sys.platform == "emscripten" or "js" in sys.modules:
        return json.dumps(value, separators=(",", ":"), allow_nan=False)
    return value


def decode_callback(value):
    return value if isinstance(value, dict) else json.loads(str(value))


def can_wait_sync():
    if sys.platform == "emscripten" or "js" in sys.modules:
        from pyodide.ffi import can_run_sync
        return can_run_sync()
    return True


def wait_sync(value):
    if sys.platform == "emscripten" or "js" in sys.modules:
        from pyodide.ffi import run_sync
        return run_sync(value)
    from _noon_native_host import _NativeCompletion
    if not isinstance(value, _NativeCompletion):
        raise TypeError("native synchronous wait accepts only Noon host completions")
    return value.run_sync()


# A real browser worker has one immutable binding module for its lifetime.
# Resolve each host symbol only on its first use; caching a function retains no
# Scene/context and never bypasses the Rust generation/retirement checks. Missing
# optional symbols are not cached, so lazy resource preparation remains possible.
# Native import-only tools may install a temporary js fixture: keep their lazy
# resolution above rather than retaining fixture functions across tool invocations.
if sys.platform == "emscripten":
    _browser = importlib.import_module("js")
    from pyodide.ffi import can_run_sync as can_wait_sync, run_sync as wait_sync

    def __getattr__(name):
        if name.startswith("__"):
            raise AttributeError(name)
        try:
            value = getattr(_browser, name)
        except AttributeError:
            raise AttributeError(f"Noon host does not provide {name}") from None
        globals()[name] = value
        return value

    # The browser codec and wait primitive are selected at import, not for every
    # local edit or callback. Native values still take the typed path above.
    encode_callback = partial(json.dumps, separators=(",", ":"), allow_nan=False)

    def decode_callback(value):
        return json.loads(str(value))
