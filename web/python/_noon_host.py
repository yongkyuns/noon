"""Optional host bindings only; no scene, scheduler, or semantic state.

The public Python implementation is shared unchanged. Browser functions resolve
once at import to the existing JS/WASM bridge; CPython resolves native functions.
"""
from __future__ import annotations
import importlib
import json
import sys


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
