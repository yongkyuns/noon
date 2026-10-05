"""CPython host integration over the optional native Rust extension.

The finite sampled driver is renderer-free, like browser external-sample
qualification. It advances all deterministic samples in Rust, not Python.
"""
from __future__ import annotations
import asyncio
from contextvars import ContextVar
import _noon_native as _native

_store = ContextVar("noon_native_store", default=None)
_settings = ContextVar("noon_native_settings", default=60.0)


def _arena():
    arena = _store.get()
    if arena is None:
        arena = _native.Store()
        _store.set(arena)
    return arena

noonAuthoringGeometryOptions = _native.GeometryOptions
noonResolveAnimationOptions = _native.resolve_animation_options
noonResolveTransformAnimationOptions = _native.resolve_transform_options


def noonCreateAuthoringGeometryHandle(options):
    return _arena().geometry(options)


def noonCreateCanonicalAuthoringSceneContext():
    context = _arena().context()
    context.configure(_settings.get())
    return context


def noonAuthoringMembershipBatch(kind):
    return _native.MembershipBatch(kind)


def noonRequireSemanticContinuationActive(context):
    context.requireActive()


class _NativeCompletion:
    """One deferred native host call, awaitable or synchronously consumable.

    This holds no scene values or timeline. The Rust operation is admitted before
    the completion is constructed, matching the browser's eager admission.
    """
    __slots__ = ("_call", "_args", "_consumed")

    def __init__(self, call, *args):
        self._call, self._args, self._consumed = call, args, False

    def run_sync(self):
        if self._consumed:
            raise RuntimeError("host completion can be consumed only once")
        self._consumed = True
        return self._call(*self._args)

    async def _wait(self):
        # One cancellation point per barrier/required callback, never per frame.
        await asyncio.sleep(0)
        return self.run_sync()

    def __await__(self):
        return self._wait().__await__()


def noonAwaitSemanticContinuation(context):
    return _NativeCompletion(context.drive)


def noonSetSemanticContinuationCallbackSession(context, session_id):
    context.requireActive()
    # Callable identity belongs to the common Python callback table. Rust has
    # already registered the callbacks and determines which occurrences run.


def noonSemanticContinuationGeneration(context):
    context.requireActive()
    return context.runtime_identity()


def noonReadSemanticContinuationCallback(context, token, request):
    return _NativeCompletion(context.read_callback, token, request)


def noonCompleteSemanticContinuationCallback(context, token, batch):
    return _NativeCompletion(context.submit_callback, token, batch)


def noonAcknowledgeSemanticContinuationCallback(context, token, failure=None):
    if failure is not None:
        context.retire()
        raise RuntimeError(failure)
    context.acknowledge_callback(token)
    return _NativeCompletion(context.drive)


def noonFailSemanticContinuationCallback(context, token, message):
    return _NativeCompletion(context.fail_callback, token, message)


def close_scene(scene):
    """Retire one native session and release its host-only callable references."""
    import _manim_updaters
    context = getattr(scene, "_canonical_authoring_context", None)
    if context is not None:
        context.retire()
    session = _manim_updaters.canonical_callback_session_id(scene)
    if session is not None:
        _manim_updaters.release_session(session)


async def run_scene(scene_class, *, sample_hz=60.0):
    from _manim_scene import execute_construct
    store_token = _store.set(_native.Store())
    settings_token = _settings.set(float(sample_hz))
    scene = None
    try:
        scene = scene_class()
        await execute_construct(scene)
        return scene
    except BaseException:
        if scene is not None:
            close_scene(scene)
        raise
    finally:
        _settings.reset(settings_token)
        _store.reset(store_token)


async def run_source(source, context=None, *, sample_hz=60.0, portable=False, filename="<noon>"):
    """Run unchanged Python source with an explicit finite sampling policy.

    portable=False executes the original synchronous call stack; portable=True
    exercises exactly the bounded compiler used by the browser source host.
    """
    from _noon_source import execute_source
    store_token = _store.set(_native.Store())
    settings_token = _settings.set(float(sample_hz))
    selected = []
    def remember(scene):
        if not any(existing is scene for existing in selected):
            selected.append(scene)
    try:
        return await execute_source(source, context, portable=portable,
                                    filename=filename, on_scene=remember)
    except BaseException:
        for scene in selected:
            close_scene(scene)
        raise
    finally:
        _settings.reset(settings_token)
        _store.reset(store_token)
