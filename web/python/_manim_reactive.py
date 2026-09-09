"""Python tracker and native-input wrappers over shared Rust semantic handles.

Python retains invocation scope, constructor options and handle references.
Scalar values, signal identity, graph declarations and playback belong to Rust.
"""

from __future__ import annotations

from _noon_errors import engine_call, raise_engine_error

import math
from contextvars import ContextVar
from typing import Any

import noon as _base

try:
    from js import noonCreateAuthoringValueTrackerHandle as _create_tracker_handle
except ImportError:  # Native CPython may inspect wrappers without a browser store.
    _create_tracker_handle = None

# Python invocation ownership only; scalar values and identity remain in Rust.
_AUTHORING_SCENE: ContextVar[object | None] = ContextVar(
    "noon_authoring_scene", default=None
)


def _current_authoring_scene() -> object | None:
    return _AUTHORING_SCENE.get()


def _enter_authoring_scene(scene: object | None):
    return _AUTHORING_SCENE.set(scene)


def _leave_authoring_scene(token) -> None:
    _AUTHORING_SCENE.reset(token)


def _finite_scalar(name: str, value: object) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise TypeError(f"{name} must be a number")
    result = float(value)
    if not math.isfinite(result):
        raise ValueError(f"{name} must be finite")
    return result


def _vec2_ir(value: object) -> dict[str, float]:
    vector = _base._as_vec2(value)
    return {"x": float(vector.x), "y": float(vector.y)}


def _nonempty_string(name: str, value: object) -> str:
    if not isinstance(value, str):
        raise TypeError(f"{name} must be a string")
    if not value.strip():
        raise ValueError(f"{name} must not be empty")
    return value


def _button(value: object) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise TypeError("button must be an integer")
    if value < 0 or value > 255:
        raise ValueError("button must be in the range 0..255")
    return value


class _ValueAnimationBuilder:
    def __init__(self, tracker: ValueTracker) -> None:
        self.tracker = tracker
        self.anim_args: dict[str, Any] = {}
        self.cannot_pass_args = False
        self.target_value: float | None = None

    def __call__(self, **kwargs: Any) -> _ValueAnimationBuilder:
        if self.cannot_pass_args:
            raise ValueError(
                "Animation arguments must be passed before accessing methods and can only be passed once"
            )
        self.anim_args = dict(kwargs)
        self.cannot_pass_args = True
        return self

    def set_value(self, value: float) -> _ValueAnimationBuilder:
        self.cannot_pass_args = True
        self.target_value = _finite_scalar("value", value)
        return self

    def increment_value(self, delta: float) -> _ValueAnimationBuilder:
        self.cannot_pass_args = True
        self.target_value = self.tracker.get_value() + _finite_scalar("delta", delta)
        return self


class ValueTracker:
    """Declarative scalar input compatible with Manim's common tracker vocabulary."""

    def __init__(self, value: float = 0.0) -> None:
        value = _finite_scalar("value", value)
        scene = _current_authoring_scene()
        if scene is not None:
            canonical = scene.value_tracker(value)
            self._scene = scene
            self._canonical_context = canonical._canonical_context
            self._canonical_handle = canonical._canonical_handle
            return
        if _create_tracker_handle is not None:
            self._scene = None
            self._canonical_context = None
            self._canonical_handle = engine_call(_create_tracker_handle, value)
            return
        raise RuntimeError("ValueTracker requires the shared Rust authoring context")

    @classmethod
    def _from_canonical(
        cls, scene: _base.Scene, context: object, handle: object
    ) -> ValueTracker:
        """Create the typed canonical wrapper without a Python scalar value.

        The context owns the signal's semantic identity and authored/runtime
        values. This object keeps only Python ownership ergonomics plus the
        opaque typed WASM handle.
        """
        tracker = object.__new__(cls)
        tracker._scene = scene
        tracker._canonical_context = context
        tracker._canonical_handle = handle
        return tracker

    def _canonical_context_handle(self) -> tuple[object, object] | None:
        context = getattr(self, "_canonical_context", None)
        handle = getattr(self, "_canonical_handle", None)
        if context is None or handle is None:
            return None
        return context, handle

    def _detached_canonical_handle(self) -> object | None:
        if getattr(self, "_canonical_context", None) is not None:
            return None
        return getattr(self, "_canonical_handle", None)

    def _associate_canonical(self, scene: _base.Scene, context: object) -> None:
        """Adopt a detached shared tracker after Rust commits its Scene scope."""

        if self._scene is scene:
            if getattr(self, "_canonical_context", None) is not context:
                raise ValueError("ValueTracker belongs to another canonical Scene context")
            return
        if self._scene is not None:
            raise ValueError("ValueTracker already belongs to another Scene")
        handle = self._detached_canonical_handle()
        if handle is None:
            raise ValueError("ValueTracker has no detached shared semantic handle")
        try:
            engine_call(context.associateValueTracker, handle)
        except Exception as error:
            raise_engine_error(error)
        self._commit_canonical_association(scene, context)

    def _commit_canonical_association(self, scene: _base.Scene, context: object) -> None:
        """Publish wrapper metadata after Rust has atomically enrolled this handle."""
        self._scene = scene
        self._canonical_context = context

    def get_value(self) -> float:
        import _manim_updaters

        canonical = self._canonical_context_handle()
        if canonical is not None and _manim_updaters.canonical_callback_phase_active():
            # The callback phase is deliberately ahead of the published context
            # frame. Read the token-pinned phase view rather than stale runtime
            # state or this wrapper's authored value.
            return _manim_updaters.canonical_callback_scalar_value(self._scene, canonical[1])
        if canonical is not None:
            context, handle = canonical
            return float(engine_call(context.valueTrackerValue, handle))
        detached = self._detached_canonical_handle()
        if detached is not None:
            try:
                return float(engine_call(detached.detachedValue))
            except Exception as error:
                raise_engine_error(error)
        raise RuntimeError("ValueTracker has no shared semantic handle")

    def set_value(self, value: float) -> ValueTracker:
        import _manim_updaters

        value = _finite_scalar("value", value)
        canonical = self._canonical_context_handle()
        if canonical is not None:
            if _manim_updaters.canonical_callback_phase_active():
                raise NotImplementedError(
                    "canonical ValueTracker callback writes are not supported"
                )
            context, handle = canonical
            try:
                engine_call(context.setValueTracker, handle, value)
            except Exception as error:
                raise_engine_error(error)
            return self
        detached = self._detached_canonical_handle()
        if detached is not None:
            try:
                engine_call(detached.setDetachedValue, value)
            except Exception as error:
                raise_engine_error(error)
            return self
        raise RuntimeError("ValueTracker has no shared semantic handle")

    def increment_value(self, delta: float) -> ValueTracker:
        return self.set_value(self.get_value() + _finite_scalar("delta", delta))

    @property
    def animate(self) -> _ValueAnimationBuilder:
        return _ValueAnimationBuilder(self)


class _NativeSignal:
    def __init__(self) -> None:
        raise TypeError("native signals must be created by the shared Scene API")

    @classmethod
    def _from_canonical(
        cls, scene: _base.Scene, context: object, handle: object
    ) -> _NativeSignal:
        """Create a wrapper over one store-owned native signal handle.

        Native signal identity and values remain in Rust. Python retains only
        the opaque handle required to pass the source back to the same context.
        """
        signal = object.__new__(cls)
        signal._scene = scene
        signal._canonical_context = context
        signal._canonical_handle = handle
        return signal

    def _canonical_context_handle(self) -> tuple[object, object] | None:
        context = getattr(self, "_canonical_context", None)
        handle = getattr(self, "_canonical_handle", None)
        if context is None or handle is None:
            return None
        return context, handle

class NativeVectorSignal(_NativeSignal):
    """Thin handle for a native Vec2-valued input signal."""


class NativeBoolSignal(_NativeSignal):
    """Thin handle for a native bool-valued input signal."""


public = {
    "ValueTracker": ValueTracker,
    "NativeVectorSignal": NativeVectorSignal,
    "NativeBoolSignal": NativeBoolSignal,
}
