"""CPython host integration over the optional native Rust extension.

The finite sampled driver is renderer-free, like browser external-sample
qualification. It advances all deterministic samples in Rust, not Python.
"""
from __future__ import annotations
import asyncio
from contextvars import ContextVar
import _noon_native as _native

_store = ContextVar("noon_native_store", default=None)
_settings = ContextVar("noon_native_settings", default=(60.0, False, True))
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

noonAudioSource = _native.AudioSource
noonAudioClipOptions = _native.ClipOptions

noonImageMapping = _native.ImageMapping
noonImagePaintOptions = _native.ImagePaintOptions
noonPlotSamplingPlan = _native.PlotSamplingPlan
noonLinearGradient = _native.LinearGradient
noonRadialGradient = _native.RadialGradient
noonNoisePaint = _native.NoisePaint
noonHatchPaint = _native.HatchPaint
noonShaderParameter = _native.ShaderParameter
noonShaderSchema = _native.ShaderSchema
noonShaderEffect = _native.EffectDefinition.shader
noonEffectUpdate = _native.EffectUpdate
noonGlow = _native.EffectDefinition.glow
noonBlur = _native.EffectDefinition.blur
noonBloom = _native.EffectDefinition.bloom
noonBackdropBlur = _native.EffectDefinition.backdropBlur
noonShadow = _native.EffectDefinition.shadow
noonPixelate = _native.EffectDefinition.pixelate
noonNoiseDissolve = _native.EffectDefinition.noiseDissolve

noonAuthoringMeshOptions = _native.MeshInput
noonAuthoringMeshFamilyOptions = _native.MeshFamilyInput
noonSurfaceSamplingPlan = _native.SurfaceSamplingPlan

def noonCreateAuthoringMeshHandle(candidate):
    return _arena().mesh(candidate)

def noonCreateAuthoringMeshFamilyHandle(candidate):
    return _arena().mesh_family(candidate)

noonAuthoringGraphOptions = _native.GraphOptions
noonAuthoringSampleSpaceOptions = _native.SampleSpaceOptions


def noonCreateAuthoringSampleSpaceHandle(options):
    return _native.create_detached_sample_space(_arena(), options)


noonAuthoringCoordinateOptions = _native.CoordinateOptions

def noonCreateAuthoringCoordinateHandle(options):
    return _arena().coordinates(options)

noonAuthoringArrowOptions = _native.ArrowOptions
noonLinearTransformationPathArc = _native.linear_transformation_path_arc


def noonCreateAuthoringArrowHandle(options):
    return _native.create_arrow(_arena(), options)


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

if hasattr(_native, "CodeOptions"):
    noonCodeOptions = _native.CodeOptions
    noonCodeFromFamily = _native.code_from_family
    noonCodeStylesList = _native.code_styles_list
    noonCodeLanguagesList = _native.code_languages_list

    def noonCreateAuthoringCodeHandle(source, options, context):
        return _native.create_code(_arena(), source, options, context)

if hasattr(_native, "prepare_latex"):
    async def noonPrepareLatex():
        _native.prepare_latex(_arena())

    def noonCreateAuthoringLatexStringsHandle(strings, math, size, red, green, blue, alpha,
                                               opacity, isolates, context):
        return _native.create_latex(_arena(), strings, math, size, (red, green, blue, alpha),
                                    opacity, isolates, context)

    def noonCreateAuthoringLatexHandle(source, *arguments):
        return noonCreateAuthoringLatexStringsHandle([source], *arguments)

    noonAuthoringBarChartOptions = _native.BarChartOptions

    def noonCreateAuthoringBarChart(options, labels, context):
        return _native.create_bar_chart(_arena(), options, labels, context)

    def noonBarChartLabels(chart, size, buff, math, rgba, context):
        return _native.bar_chart_labels(_arena(), chart, size, buff, math, rgba, context)

    noonMatrixFromFamily = _native.matrix_from_family

    def noonCreateAuthoringMatrixHandle(rows, v, h, bracket_h, bracket_v, stretch, context):
        return _native.create_matrix(_arena(), rows, (v, h, bracket_h, bracket_v, stretch), context)

    def noonCreateAuthoringIntegerMatrixHandle(rows, v, h, bracket_h, bracket_v, stretch, context):
        return _native.create_numeric_matrix(
            _arena(), rows, (v, h, bracket_h, bracket_v, stretch), False, context,
        )

    def noonCreateAuthoringDecimalMatrixHandle(rows, v, h, bracket_h, bracket_v, stretch, context):
        return _native.create_numeric_matrix(
            _arena(), rows, (v, h, bracket_h, bracket_v, stretch), True, context,
        )

    def noonCreateAuthoringMobjectMatrixHandle(rows, v, h, bracket_h, bracket_v, stretch, context):
        return _native.create_mobject_matrix(
            _arena(), rows, (v, h, bracket_h, bracket_v, stretch), context,
        )

    noonTableFromFamily = _native.table_from_family
    noonTableCell = _native.table_cell

    def noonIsAuthoringFamilyHandle(handle):
        return isinstance(handle, _native.FamilyHandle)

    def _table_options(values):
        return (values[:4], values[4:8], values[8:12], values[12:])

    def noonCreateAuthoringTableHandle(rows, *values):
        return noonCreateAuthoringTableWithLabelsHandle(rows, None, None, [], *values)

    def noonCreateAuthoringTableWithLabelsHandle(rows, row_labels, column_labels, corner, *values):
        return _native.create_text_table(
            _arena(), rows, row_labels, column_labels, next(iter(corner), None),
            _table_options(values[:-1]), values[-1],
        )

    def noonCreateAuthoringMobjectTableHandle(rows, row_labels, column_labels, corner, *values):
        return _native.create_mobject_table(
            _arena(), rows, row_labels, column_labels, next(iter(corner), None),
            _table_options(values[:-1]), values[-1],
        )

    def noonCreateAuthoringMathTableHandle(rows, *values):
        return _native.create_math_table(_arena(), rows, _table_options(values[:-1]), values[-1])

    def noonCreateAuthoringIntegerTableHandle(rows, *values):
        return _native.create_numeric_table(_arena(), rows, None, _table_options(values[:-1]), values[-1])

    def noonCreateAuthoringDecimalTableHandle(rows, places, sign, commas, ellipsis, unit, *values):
        return _native.create_numeric_table(
            _arena(), rows, (places, sign, commas, ellipsis, unit),
            _table_options(values[:-1]), values[-1],
        )

    def noonGetHighlightedTableCell(context, selection, column, red, green, blue, alpha, opacity):
        return _native.highlighted_cell(context, selection, column, (red, green, blue, alpha), opacity, False)

    def noonHighlightTableCell(context, selection, column, red, green, blue, alpha, opacity):
        return _native.highlighted_cell(context, selection, column, (red, green, blue, alpha), opacity, True)

    def noonCreateAuthoringVariableHandle(label, value, places, sign, commas, ellipsis, unit, size, context):
        return _native.create_variable(
            _arena(), label, value, (places, sign, commas, ellipsis, unit), size, context,
        )

    def noonCreateAuthoringDecimalNumberHandle(*arguments):
        return _native.create_decimal(_arena(), *arguments)

    def noonNumericFromMobject(mobject):
        return _native.decimal_from_mobject(_arena(), mobject)

    def noonNumberLineDecimalNumberLabelFamily(*arguments):
        return _native.line_decimal_labels(_arena(), *arguments)

    def noonNumberPlaneDecimalCoordinateLabelFamilies(*arguments):
        return _native.decimal_coordinates(_arena(), *arguments, True)

    def noonAxesDecimalCoordinateLabelFamilies(*arguments):
        return _native.decimal_coordinates(_arena(), *arguments, False)

    def noonDecimalNumberLabelMembers(family):
        return _native.decimal_label_members(_arena(), family)

if hasattr(_native.Store, "typst"):
    def noonCreateAuthoringTypstHandle(*args):
        return _arena().typst(*args)

noonResolveAnimationOptions = _native.resolve_animation_options
noonResolveGlowPulseAnimationOptions = _native.resolve_glow_pulse_options
noonResolveCreateAnimationOptions = _native.resolve_create_options
noonResolveTransformAnimationOptions = _native.resolve_transform_options


def noonCreateAuthoringValueTrackerHandle(initial):
    return _arena().tracker(initial)


def noonCreateAuthoringGeometryHandle(options):
    return _arena().geometry(options)


def noonCreateAuthoringFamilyHandle(batch, z_index, draw_enabled, *color):
    if color:
        return _arena().family_with_color(batch, z_index, draw_enabled, *color)
    return _arena().family(batch, z_index, draw_enabled)


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


def noonPrepareSemanticEffects(context, staged):
    return _NativeCompletion(context.publish_prepared_effects, staged)


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


def noonStageSemanticContinuationProvisionalGeometry(context, token, options):
    return context.stage_callback_provisional_geometry(token, options)


def noonStageSemanticContinuationProvisionalShift(context, token, mobject, x, y):
    context.stage_callback_provisional_shift(token, mobject, x, y)


def noonStageSemanticContinuationProvisionalFill(context, token, mobject, r, g, b, a, opacity):
    context.stage_callback_provisional_fill(token, mobject, (r, g, b, a), opacity)


def noonSemanticContinuationProvisionalCenter(context, token, mobject):
    return context.callback_provisional_center(token, mobject)


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


async def run_scene(scene_class, *, sample_hz=60.0, retain_history=False, prepare_shaders=True):
    from _manim_scene import execute_construct
    resources = _NativeRunResources()
    resource_token = _resources.set(resources)
    store_token = _store.set(_native.Store())
    settings_token = _settings.set((float(sample_hz), bool(retain_history), bool(prepare_shaders)))
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


async def run_source(source, context=None, *, sample_hz=60.0, portable=False, filename="<noon>", retain_history=False, prepare_shaders=True, scene_name=None):
    """Run unchanged Python source with an explicit finite sampling policy.

    portable=False executes the original synchronous call stack; portable=True
    exercises exactly the bounded compiler used by the browser source host.
    Set prepare_shaders=False to forbid GPU allocation for live shader admission.
    """
    from _noon_source import execute_source
    resources = _NativeRunResources()
    resource_token = _resources.set(resources)
    store_token = _store.set(_native.Store())
    settings_token = _settings.set((float(sample_hz), bool(retain_history), bool(prepare_shaders)))
    try:
        scene = await execute_source(source, context, portable=portable,
                                     filename=filename, scene_name=scene_name)
        resources.close(keep=getattr(scene, "_canonical_authoring_context", None))
        return scene
    except BaseException:
        resources.close()
        raise
    finally:
        _resources.reset(resource_token)
        _settings.reset(settings_token)
        _store.reset(store_token)
