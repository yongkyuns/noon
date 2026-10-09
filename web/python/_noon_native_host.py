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
_resources = ContextVar("noon_native_resources", default=None)
_video = ContextVar("noon_native_video", default=None)


class _NativeRunResources:
    """Failure cleanup for acquired native handles, not another scene registry.

    The existing callable-table watermark bounds cleanup to registrations made
    during this invocation. Context identity excludes nested/concurrent owners.
    No object traversal, callback-table copy or per-frame hook is introduced.
    """
    def __init__(self):
        import _manim_updaters
        self.callbacks = _manim_updaters
        self.first_session = _manim_updaters._NEXT_SESSION_ID
        self.contexts = []

    def close(self):
        owned = {id(context) for context in self.contexts}
        for context in reversed(self.contexts):
            context.retire()
        for session_id in range(self.first_session, self.callbacks._NEXT_SESSION_ID):
            session = self.callbacks._CANONICAL_SESSIONS.get(session_id)
            if session is not None and id(session.context) in owned:
                self.callbacks.release_session(session_id)
        self.contexts.clear()


def _arena():
    arena = _store.get()
    if arena is None:
        arena = _native.Store()
        _store.set(arena)
    return arena

noonAuthoringGeometryOptions = _native.GeometryOptions
noonResolveRenderOptions = _native.resolve_render_options
noonResolveAnimationOptions = _native.resolve_animation_options
noonResolveTransformAnimationOptions = _native.resolve_transform_options


def noonCreateAuthoringGeometryHandle(options):
    return _arena().geometry(options)


def noonCreateCanonicalAuthoringSceneContext():
    context = _arena().context()
    context.configure(_settings.get())
    resources = _resources.get()
    if resources is not None:
        resources.contexts.append(context)
    video = _video.get()
    if video is not None:
        video.bind(context)
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


def _drive(context):
    video = _video.get()
    return context.drive() if video is None else video.drive(context)


def noonAwaitSemanticContinuation(context):
    return _NativeCompletion(_drive, context)


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
    return _NativeCompletion(_drive, context)


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
    resources = _NativeRunResources()
    resource_token = _resources.set(resources)
    store_token = _store.set(_native.Store())
    settings_token = _settings.set(float(sample_hz))
    scene = None
    try:
        scene = scene_class()
        await execute_construct(scene)
        return scene
    except BaseException:
        resources.close()
        raise
    finally:
        _resources.reset(resource_token)
        _settings.reset(settings_token)
        _store.reset(store_token)


async def run_source(source, context=None, *, sample_hz=60.0, portable=False, filename="<noon>", scene_name=None):
    """Run unchanged Python source with an explicit finite sampling policy.

    portable=False executes the original synchronous call stack; portable=True
    exercises exactly the bounded compiler used by the browser source host.
    """
    from _noon_source import execute_source
    resources = _NativeRunResources()
    resource_token = _resources.set(resources)
    store_token = _store.set(_native.Store())
    settings_token = _settings.set(float(sample_hz))
    try:
        return await execute_source(source, context, portable=portable,
                                    filename=filename, scene_name=scene_name)
    except BaseException:
        resources.close()
        raise
    finally:
        _resources.reset(resource_token)
        _settings.reset(settings_token)
        _store.reset(store_token)


async def _export(execute, output, options):
    """Own one source invocation and native output; no frame loop lives in Python."""
    import os
    if _video.get() is not None:
        raise RuntimeError("nested native video export is not supported")
    if not hasattr(_native, "VideoExport"):
        raise RuntimeError("native video export requires build-native-python.py --export-video")
    video = _native.VideoExport(os.fspath(output), **options)
    token = _video.set(video)
    scene = None
    try:
        scene = await execute()
        return video.finish()
    finally:
        try:
            video.abort()  # Idempotent; successful committed files are retained.
        finally:
            try:
                if scene is not None:
                    close_scene(scene)
            finally:
                _video.reset(token)


def _export_options(*, width, height, fps, frame_rate, quality, resolution, format,
                    max_frames, start_frame, final_hold, png, overwrite, fallback, ffmpeg):
    from _noon_render_options import resolve_render_options
    if fps is not None and frame_rate is not None:
        raise ValueError("supply either fps or frame_rate, not both")
    if png and format not in (None, "png"):
        raise ValueError("png=True conflicts with the selected format")
    resolved = resolve_render_options(
        quality=quality, resolution=resolution, frame_rate=frame_rate if fps is None else fps,
        format="png" if png else format, pixel_width=width, pixel_height=height,
    )
    return dict(width=resolved.width, height=resolved.height,
                p=resolved.fps[0], q=resolved.fps[1], max_frames=max_frames,
                start_frame=start_frame, final_hold=final_hold,
                png=resolved.format == "png", overwrite=overwrite,
                fallback=fallback, ffmpeg=ffmpeg)


async def export_scene(scene_class, output, *, width=None, height=None, fps=None,
                       frame_rate=None, quality=None, resolution=None, format=None,
                       max_frames=108000, start_frame=0, final_hold=0.0,
                       png=False, overwrite=False, fallback=False, ffmpeg=None):
    """Export a finite scene with shared Rust output settings.

    Defaults and quality presets follow the pinned Manim render profile. Explicit
    dimensions/rate override quality. Numeric FPS and explicit ratios are accepted.
    max_frames is a failure cap, not a truncation or an animation-number range.
    Native authoring capability limits and source retirement remain unchanged.
    """
    options = _export_options(width=width, height=height, fps=fps, frame_rate=frame_rate,
                              quality=quality, resolution=resolution, format=format,
                              max_frames=max_frames, start_frame=start_frame,
                              final_hold=final_hold, png=png, overwrite=overwrite,
                              fallback=fallback, ffmpeg=ffmpeg)
    return await _export(lambda: run_scene(scene_class), output, options)


async def export_source(source, output, context=None, *, portable=False,
                        filename="<noon>", scene_name=None,
                        width=None, height=None, fps=None, frame_rate=None,
                        quality=None, resolution=None, format=None,
                        max_frames=108000, start_frame=0, final_hold=0.0,
                        png=False, overwrite=False, fallback=False, ffmpeg=None):
    """Select one scene through the common loader and export via shared Rust.

    scene_name selects an actual class, not an appended source expression. The
    single-result source mode remains available when no class name is supplied.
    Multiple output scenes and Manim configuration-file loading are not implied.
    """
    options = _export_options(width=width, height=height, fps=fps, frame_rate=frame_rate,
                              quality=quality, resolution=resolution, format=format,
                              max_frames=max_frames, start_frame=start_frame,
                              final_hold=final_hold, png=png, overwrite=overwrite,
                              fallback=fallback, ffmpeg=ffmpeg)
    return await _export(lambda: run_source(source, context, portable=portable,
                                           filename=filename, scene_name=scene_name),
                         output, options)
