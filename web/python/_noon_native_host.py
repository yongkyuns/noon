"""CPython host integration over the optional native Rust extension.

The finite sampled driver is renderer-free, like browser external-sample
qualification. It advances all deterministic samples in Rust, not Python.
"""
from __future__ import annotations
import asyncio
from contextvars import ContextVar
import _noon_native as _native

_store = ContextVar("noon_native_store", default=None)
_settings = ContextVar("noon_native_settings", default=(60.0, False))
_resources = ContextVar("noon_native_resources", default=None)


class _NativeRunResources:
    """Only handles acquired by this invocation; cleanup never scans global IDs."""
    def __init__(self):
        self.contexts = []
        self.callback_sessions = []

    def close(self, keep=None):
        import _manim_updaters
        for context in reversed(self.contexts):
            if context is not keep:
                context.retire()
        for session_id, context in self.callback_sessions:
            if context is not keep:
                _manim_updaters.release_session(session_id)
        self.contexts.clear()
        self.callback_sessions.clear()


def callback_session_acquired(session_id, context):
    resources = _resources.get()
    if resources is not None:
        resources.callback_sessions.append((session_id, context))


def _arena():
    arena = _store.get()
    if arena is None:
        arena = _native.Store()
        _store.set(arena)
    return arena

noonAuthoringGeometryOptions = _native.GeometryOptions
noonAuthoringVectorPath = _native.AuthoringVectorPath
noonAuthoringImageOptions = _native.ImageOptions


def noonCreateAuthoringImageHandle(options):
    return _arena().image(options)


def noonCreateAuthoringSvgHandle(source, should_center, height, width_or_defaults):
    return _arena().svg(source, should_center, height, width_or_defaults)


if hasattr(_native, "FontFace"):
    noonNativeFontFace = _native.FontFace
    noonBundledNativeFontFace = _native.bundled_native_font_face
    noonTextColorBatch = _native.TextColorBatch

    def noonCreateAuthoringTextHandle(*args):
        return _arena().text(*args)

    def noonCreateAuthoringMarkupTextHandle(*args):
        return _arena().markup_text(*args)

noonResolveAnimationOptions = _native.resolve_animation_options
noonResolveCreateAnimationOptions = _native.resolve_create_options
noonResolveTransformAnimationOptions = _native.resolve_transform_options


def noonCreateAuthoringGeometryHandle(options):
    return _arena().geometry(options)


def noonCreateAuthoringFamilyHandle(batch, z_index, *color):
    if color:
        return _arena().family_with_color(batch, z_index, *color)
    return _arena().family(batch, z_index)


def noonCreateCanonicalAuthoringSceneContext():
    context = _arena().context()
    context.configure(*_settings.get())
    resources = _resources.get()
    if resources is not None:
        resources.contexts.append(context)
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


async def run_scene(scene_class, *, sample_hz=60.0, retain_history=False):
    from _manim_scene import execute_construct
    resources = _NativeRunResources()
    resource_token = _resources.set(resources)
    store_token = _store.set(_native.Store())
    settings_token = _settings.set((float(sample_hz), bool(retain_history)))
    scene = None
    try:
        scene = scene_class()
        await execute_construct(scene)
        resources.close(keep=getattr(scene, "_canonical_authoring_context", None))
        return scene
    except BaseException:
        resources.close()
        raise
    finally:
        _resources.reset(resource_token)
        _settings.reset(settings_token)
        _store.reset(store_token)


async def run_source(source, context=None, *, sample_hz=60.0, portable=False, filename="<noon>", retain_history=False):
    """Run unchanged Python source with an explicit finite sampling policy.

    portable=False executes the original synchronous call stack; portable=True
    exercises exactly the bounded compiler used by the browser source host.
    """
    from _noon_source import execute_source
    resources = _NativeRunResources()
    resource_token = _resources.set(resources)
    store_token = _store.set(_native.Store())
    settings_token = _settings.set((float(sample_hz), bool(retain_history)))
    try:
        scene = await execute_source(source, context, portable=portable,
                                     filename=filename)
        resources.close(keep=getattr(scene, "_canonical_authoring_context", None))
        return scene
    except BaseException:
        resources.close()
        raise
    finally:
        _resources.reset(resource_token)
        _settings.reset(settings_token)
        _store.reset(store_token)
