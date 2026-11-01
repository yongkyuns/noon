import initNoonWeb, {
  WasmAuthoringStore,
  WasmAudioSource,
  WasmAudioClipOptions,
  WasmAuthoringFamilyHandle,
  WasmEffectDefinition,
  WasmEffectUpdate,
  WasmShaderParameter,
  WasmShaderSchema,
  WasmLinearGradient,
  WasmRadialGradient,
  WasmNoisePaint,
  WasmHatchPaint,
  WasmImageMapping,
  WasmImagePaintOptions,
  WasmAuthoringVectorPath,
  WasmCoordinateOptions,
  WasmBarChartOptions,
  WasmGraphOptions,
  WasmSampleSpaceOptions,
  WasmPlotSamplingPlan,
  WasmManimArrowOptions,
  WasmManimGeometryOptions,
  WasmImageMobjectOptions,
  WasmMeshOptions,
  WasmMeshFamilyOptions,
  WasmSurfaceSamplingPlan,
  WasmSceneMembershipBatch,
  WasmTextColorBatch,
  WasmNativeFontFace,
  WasmLatexCompiler,
  WasmLatexOptions,
  WasmMatrixOptions,
  WasmCompositeRows,
  WasmTableOptions,
  WasmCodeOptions,
  noonCodeStylesList,
  noonCodeLanguagesList,
  noonLinearTransformationPathArc,
  resolveAnimationOptions,
  resolveCreateAnimationOptions,
  resolveGlowPulseAnimationOptions,
  resolveTransformAnimationOptions,
} from "./pkg/noon_web.js";
import { resolveAnimationOptionsPlain } from "./animation-options.js";
import { attachSemanticEngine } from "./semantic-engine-endpoint.js";
import { createPreparedInterpreterDriver } from "./prepared-interpreter-driver.js";
import { PYTHON_COMPAT_MODULES } from "./python-compat-modules.js";
import { withAuthoringDiagnostics, diagnosedHostCode, authoringDiagnosticPayload } from "./authoring-diagnostic-runtime.js";
import { loadPyodide } from "https://cdn.jsdelivr.net/pyodide/v314.0.5/full/pyodide.mjs";

const AUTHORING_CHANNEL = "noon.authoring";
const AUTHORING_PROTOCOL_VERSION = 7;
const AUTHORING_STARTUP_METRICS_VERSION = 2;
const moduleGraphReadyAt = performance.now();
const authoringMilestones = {
  performanceTimeOriginMs: performance.timeOrigin,
  firstSceneContextCreatedAtMs: null,
  authoringWorkerReadyAtMs: null,
};

const pyodidePromise = initializePyodide();
let requestQueue = Promise.resolve();
const semanticContexts = new Map();
let nextSemanticContext = 0;
let nextContinuationGeneration = 1;
let activeAuthoringRun = null;
// Opaque source may modify imported modules/builtins. Never infer purity against
// that environment on a later Run or retry/reconstruct the source to recover it.
let authoringLibrariesPristine = true;
let fatalAuthoringFailure = false;

pyodidePromise
  .then(() => {
    authoringMilestones.authoringWorkerReadyAtMs = performance.now();
    post("ready");
  })
  .catch(failAuthoringWorker);

// Interpreter-fatal rejections may never settle runPythonAsync. Forward them
// through the existing fatal channel instead of leaving Run pending forever.
self.addEventListener("unhandledrejection", (event) => {
  event.preventDefault();
  failAuthoringWorker(event.reason);
});

function failAuthoringWorker(error) {
  if (fatalAuthoringFailure) return;
  fatalAuthoringFailure = true;
  postError(null, error);
  self.close();
}

// A WASM trap cannot unwind Rust borrows or restore a coherent engine instance.
// Retire this host through the existing fatal channel, even if Python catches it.
self.noonReportEngineTrap = (error) => {
  if (error instanceof WebAssembly.RuntimeError) failAuthoringWorker(error);
};

self.addEventListener("message", (event) => {
  if (fatalAuthoringFailure) return;
  if (isContinuationControl(event.data)) {
    void handleContinuationControl(event.data);
    return;
  }
  requestQueue = requestQueue.then(() => {
    if (!fatalAuthoringFailure) return handleRequest(event.data);
  });
});

async function initializePyodide() {
  const initializeStartedAt = performance.now();
  const resourceDurations = {};
  const noonWebReady = measureStartupTask(resourceDurations, "noonWebInitMs", () => initNoonWeb());
  const pyodideReady = measureStartupTask(resourceDurations, "pyodideInitMs", () => loadPyodide());
  const compatibilityBundleReady = measureStartupTask(
    resourceDurations,
    "compatibilityBundleMs",
    () => loadCompatibilityBundle(),
  );
  const startupResourcesReady = Promise.all([
    noonWebReady,
    pyodideReady,
    compatibilityBundleReady,
  ]);
  const [, pyodide, compatibilityModules] = await startupResourcesReady;
  const resourcesReadyAt = performance.now();
  const authoringStore = new WasmAuthoringStore();
  const authoringStoreCreatedAt = performance.now();
  self.noonCreateCanonicalAuthoringSceneContext = () => {
    const context = authoringStore.createSceneContext();
    authoringMilestones.firstSceneContextCreatedAtMs ??= performance.now();
    return context;
  };
  self.noonCreateAuthoringValueTrackerHandle = (initial) =>
    authoringStore.createValueTracker(initial);
  self.noonRegisterSemanticExecution = registerSemanticExecution;
  self.noonAwaitSemanticContinuation = (context) => awaitSemanticContinuation(context);
  self.noonPrepareSemanticEffects = async (context, staged) => {
    try {
      const continuation = activeAuthoringRun?.continuation;
      if (!continuation || continuation.context !== context || continuation.terminal ||
          continuation.preparedPlayback || continuation.endpoint === null) {
        throw new Error("shader resource preparation requires the active render endpoint");
      }
      if (staged.startsSegment === true) {
        return await awaitSemanticContinuation(context, () =>
          continuation.endpoint.prepareSourceEffects(continuation.generation, staged));
      }
      await continuation.endpoint.prepareSourceEffects(continuation.generation, staged);
    } catch (error) {
      if (error instanceof WebAssembly.RuntimeError) failAuthoringWorker(error);
      throw error;
    } finally { if (!fatalAuthoringFailure) staged.free(); }
  };
  self.noonSetSemanticContinuationCallbackSession = (context, sessionId) => {
    if (!Number.isSafeInteger(sessionId) || sessionId < 0) {
      throw new TypeError("semantic continuation callback session must be a non-negative safe integer");
    }
    if (activeAuthoringRun === null) {
      throw new Error("semantic continuation callback session requires an active Python authoring run");
    }
    const existing = activeAuthoringRun.continuationCallbackSession;
    if (existing !== null && (existing.context !== context || existing.sessionId !== sessionId)) {
      throw new Error("semantic continuation callback session changed during authoring");
    }
    if (activeAuthoringRun.continuation !== null && activeAuthoringRun.continuation.context !== context) {
      throw new Error("semantic continuation callback session belongs to another context");
    }
    activeAuthoringRun.continuationCallbackSession = { context, sessionId };
    const continuation = activeAuthoringRun.continuation;
    if (continuation !== null) {
      const entry = semanticContexts.get(continuation.contextId);
      if (!entry || entry.released) throw new Error("semantic continuation context is retired");
      entry.callbackSessionId = sessionId;
    }
  };
  self.noonCompleteSemanticContinuationCallback = (context, tokenJson, patchBatchJson) =>
    completeContinuationCallback(context, tokenJson, patchBatchJson);
  self.noonAcknowledgeSemanticContinuationCallback = (context, tokenJson, failure) =>
    acknowledgeContinuationCallback(context, tokenJson, failure);
  self.noonFailSemanticContinuationCallback = (context, tokenJson, message) =>
    failContinuationCallback(context, tokenJson, message);
  self.noonReadSemanticContinuationCallback = (context, tokenJson, requestJson) =>
    readContinuationCallback(context, tokenJson, requestJson);
  self.noonStageSemanticContinuationMembership = (context, tokenJson, batch) =>
    stageContinuationCallbackMembership(context, tokenJson, batch);
  self.noonContinuationMembershipRootKeys = (context, tokenJson) =>
    continuationMembershipRootKeys(context, tokenJson);
  self.noonStageSemanticContinuationProvisionalGeometry = (context, tokenJson, options) =>
    stageContinuationCallbackProvisionalGeometry(context, tokenJson, options);
  self.noonStageSemanticContinuationProvisionalShift = (context, tokenJson, object, x, y) =>
    stageContinuationCallbackProvisionalShift(context, tokenJson, object, x, y);
  self.noonStageSemanticContinuationProvisionalFill = (
    context, tokenJson, object, red, green, blue, alpha, opacity,
  ) => stageContinuationCallbackProvisionalFill(
    context, tokenJson, object, red, green, blue, alpha, opacity,
  );
  self.noonSemanticContinuationProvisionalCenter = (context, tokenJson, object) =>
    continuationCallbackProvisionalCenter(context, tokenJson, object);
  self.noonResolveSemanticContinuationProvisionalMobject = (context, tokenJson, object) =>
    resolveContinuationCallbackProvisionalMobject(context, tokenJson, object);
  self.noonAssociateSemanticContinuationCallbackMobjects = (context, tokenJson, batch) =>
    committedContinuationCallbackPlayer(context, tokenJson)
      .associatePublishedCallbackMobjects(context, batch);
  self.noonSemanticContinuationGeneration = (context) => {
    const continuation = activeAuthoringRun?.continuation;
    return continuation?.context === context ? continuation.generation : undefined;
  };
  self.noonRequireSemanticContinuationActive = (context) => {
    if (activeAuthoringRun === null) {
      throw new Error("semantic continuation is not active for this Python source run");
    }
    const continuation = activeAuthoringRun.continuation;
    if (continuation !== null &&
        (continuation.context !== context || continuation.terminal)) {
      throw new Error("semantic continuation is not active for this Python source run");
    }
  };
  self.noonEffectUpdate = (...args) => new WasmEffectUpdate(...args);
  self.noonGlow = (update) => WasmEffectDefinition.glow(update);
  self.noonBlur = (update) => WasmEffectDefinition.blur(update);
  self.noonBloom = (update) => WasmEffectDefinition.bloom(update);
  self.noonBackdropBlur = (update) => WasmEffectDefinition.backdropBlur(update);
  self.noonShadow = (update) => WasmEffectDefinition.shadow(update);
  self.noonNoiseDissolve = (update) => WasmEffectDefinition.noiseDissolve(update);
  self.noonPixelate = (update) => WasmEffectDefinition.pixelate(update);
  self.noonShaderParameter = WasmShaderParameter;
  self.noonShaderSchema = (...args) => new WasmShaderSchema(...args);
  self.noonShaderEffect = (program, update) => WasmEffectDefinition.shader(program, update);
  self.noonLinearGradient = (...args) => new WasmLinearGradient(...args);
  self.noonRadialGradient = (...args) => new WasmRadialGradient(...args);
  self.noonNoisePaint = (...args) => new WasmNoisePaint(...args);
  self.noonHatchPaint = (...args) => new WasmHatchPaint(...args);
  self.noonImageMapping = (...args) => new WasmImageMapping(...args);
  self.noonImagePaintOptions = WasmImagePaintOptions;
  self.noonAuthoringImageOptions = WasmImageMobjectOptions;
  self.noonCreateAuthoringImageHandle = (options) => authoringStore.createImage(options);
  self.noonAuthoringMeshOptions = WasmMeshOptions;
  self.noonAuthoringMeshFamilyOptions = WasmMeshFamilyOptions;
  self.noonSurfaceSamplingPlan = (...args) => new WasmSurfaceSamplingPlan(...args);
  self.noonCreateAuthoringMeshHandle = (options) => authoringStore.createMesh(options);
  self.noonCreateAuthoringMeshFamilyHandle = (options) => authoringStore.createMeshFamily(options);
  self.noonLoadImageUrl = async (url) => {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`image request failed: HTTP ${response.status}`);
    const limit = 32 * 1024 * 1024;
    if (Number(response.headers.get("Content-Length")) > limit) {
      await response.body?.cancel();
      throw new Error("encoded image exceeds 32 MiB");
    }
    if (!response.body) throw new Error("image response has no body");
    const reader = response.body.getReader();
    const chunks = [];
    let size = 0;
    try {
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        size += value.byteLength;
        if (size > limit) {
          await reader.cancel();
          throw new Error("encoded image exceeds 32 MiB");
        }
        chunks.push(value);
      }
    } finally { reader.releaseLock(); }
    const bytes = new Uint8Array(size);
    let offset = 0;
    for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
    return bytes;
  };
  self.noonAuthoringGeometryOptions = WasmManimGeometryOptions;
  self.noonAuthoringArrowOptions = WasmManimArrowOptions;
  self.noonAuthoringGraphOptions = () => new WasmGraphOptions();
  self.noonAuthoringCoordinateOptions = WasmCoordinateOptions;
  self.noonAuthoringSampleSpaceOptions = WasmSampleSpaceOptions;
  self.noonCreateAuthoringSampleSpaceHandle = (options) => authoringStore.createSampleSpace(options);
  self.noonAuthoringBarChartOptions = (values, range, width, height) =>
    new WasmBarChartOptions(values, range, width, height);
  self.noonPlotSamplingPlan = WasmPlotSamplingPlan;
  self.noonCreateAuthoringCoordinateHandle = (options) =>
    authoringStore.createCoordinates(options);
  self.noonCreateAuthoringBarChart = (options, labels, context) => {
    if (!latexCompiler) {
      options.free(); labels.free();
      throw new Error("Call await prepare_latex() before constructing BarChart");
    }
    return context == null ? authoringStore.createLabeledBarChart(options, labels, latexCompiler)
      : context.liveCreateLabeledBarChart(options, labels, latexCompiler);
  };
  self.noonBarChartLabels = (chart, size, buff, math, rgba, context) => {
    if (!latexCompiler) throw new Error("Call await prepare_latex() before constructing chart labels");
    return context == null ? chart.labelFamily(size, buff, math, rgba, latexCompiler)
      : context.liveBarLabelFamily(chart, size, buff, math, rgba, latexCompiler);
  };
  self.noonNumberPlaneDecimalCoordinateLabelFamilies = (
    plane, xValues, automaticX, yValues, automaticY, xOptions, yOptions,
  ) => {
    if (!latexCompiler) {
      xOptions.free(); yOptions.free();
      throw new Error("Call await prepare_latex() before constructing NumberPlane coordinates");
    }
    return plane.numberPlaneDecimalCoordinateLabelFamilies(
      xValues, automaticX, yValues, automaticY, xOptions, yOptions, latexCompiler,
    );
  };
  self.noonNumberLineDecimalNumberLabelFamily = (
    line, values, automatic, options, attach,
  ) => {
    if (!latexCompiler) {
      options.free();
      throw new Error("Call await prepare_latex() before constructing NumberLine numbers");
    }
    return line.numberLineDecimalNumberLabelFamily(
      values, automatic, options, attach, latexCompiler,
    );
  };
  self.noonAxesDecimalCoordinateLabelFamilies = (
    axes, xValues, automaticX, yValues, automaticY, xOptions, yOptions,
  ) => {
    if (!latexCompiler) {
      xOptions.free(); yOptions.free();
      throw new Error("Call await prepare_latex() before constructing Axes coordinates");
    }
    return axes.axesDecimalCoordinateLabelFamilies(
      xValues, automaticX, yValues, automaticY, xOptions, yOptions, latexCompiler,
    );
  };
  self.noonAuthoringVectorPath = () => new WasmAuthoringVectorPath();
  self.noonCreateAuthoringGeometryHandle = (options) =>
    authoringStore.createManimGeometry(options);
  self.noonCreateAuthoringArrowHandle = (options) =>
    authoringStore.createManimArrow(options);
  self.noonAudioSource = WasmAudioSource;
  self.noonAudioClipOptions = (...args) => new WasmAudioClipOptions(...args);
  self.noonCreateAuthoringSvgHandle = (source, shouldCenter, height, width) =>
    authoringStore.createSvgFromString(source, shouldCenter, height, width);
  self.noonTextColorBatch = () => new WasmTextColorBatch();
  self.noonNativeFontFace = (family, data, faceIndex) =>
    new WasmNativeFontFace(family, data, faceIndex);
  self.noonBundledNativeFontFace = (family) => WasmNativeFontFace.bundled(family);
  self.noonCreateAuthoringTextHandle = (...args) => authoringStore.createManimText(...args);
  self.noonCreateAuthoringMarkupTextHandle = (...args) => authoringStore.createManimMarkupText(...args);
  self.noonCreateAuthoringTypstHandle = (source, math, fontSize) =>
    authoringStore.createManimTypst(source, math, fontSize);
  let latexCompiler = null;
  let latexPreparation = null;
  self.noonPrepareLatex = async () => {
    if (!latexPreparation) {
      latexPreparation = import("./latex/backend.js")
        .then(({ prepareLatexBackend }) => prepareLatexBackend())
        .then(backend => { latexCompiler = new WasmLatexCompiler(backend); })
        .catch(error => { latexPreparation = null; throw error; });
    }
    await latexPreparation;
  };
  self.noonCreateAuthoringLatexHandle = (source, math, fontSize, red, green, blue, alpha, opacity, isolates, context) => {
    if (!latexCompiler) throw new Error("Call await prepare_latex() before constructing Tex or MathTex");
    const options = new WasmLatexOptions(source, math, fontSize, new Float64Array([red, green, blue, alpha]), opacity);
    options.isolateSubstrings(isolates);
    return context == null ? authoringStore.createLatex(options, latexCompiler)
      : context.liveCreateLatex(options, latexCompiler);
  };
  self.noonCreateAuthoringLatexStringsHandle = (strings, math, fontSize, red, green, blue, alpha, opacity, isolates, context) => {
    if (!latexCompiler) throw new Error("Call await prepare_latex() before constructing Tex or MathTex");
    const options = WasmLatexOptions.fromStrings(strings, math, fontSize, new Float64Array([red, green, blue, alpha]), opacity);
    options.isolateSubstrings(isolates);
    return context == null ? authoringStore.createLatex(options, latexCompiler)
      : context.liveCreateLatex(options, latexCompiler);
  };
  const numericHandle = (handle) => {
    if (!latexCompiler) throw new Error("Call await prepare_latex() before constructing DecimalNumber");
    return {
      // A detached number exposes its retained authored value. An attached
      // number is queried through the current canonical execution session.
      value: (context) => context == null ? handle.value : context.queryMobjectDecimalValue(handle.mobject()),
      text: () => handle.text,
      fontSize: (context) => context == null ? handle.fontSize() : context.queryMobjectDecimalFontSize(handle.mobject()),
      mobject: () => handle.mobject(),
      integerValue: (context) => context == null ? handle.integerValue() : context.queryMobjectIntegerValue(handle.mobject()),
      setValue: (value) => handle.setValue(latexCompiler, value),
      incrementValue: (delta) => handle.incrementValue(latexCompiler, delta),
      setValueLive: (context, value) => context.liveSetDecimalValue(handle.mobject(), latexCompiler, value),
      incrementValueLive: (context, delta) => context.liveIncrementDecimalValue(handle.mobject(), latexCompiler, delta),
    };
  };
  self.noonDecimalNumberLabelMembers = (family) =>
    Array.from(family.decimalNumberLabelMembers(), numericHandle);
  self.noonCreateAuthoringDecimalNumberHandle = (value, places, sign, commas, ellipsis, unit, fontSize, context) => {
    if (!latexCompiler) throw new Error("Call await prepare_latex() before constructing DecimalNumber");
    const args = [value, places, sign, commas, ellipsis, unit, fontSize, latexCompiler];
    return numericHandle(context == null ? authoringStore.createDecimalNumber(...args)
      : context.liveCreateDecimalNumber(...args));
  };
  const isFamilyHandle = (handle) => handle instanceof WasmAuthoringFamilyHandle;
  self.noonIsAuthoringFamilyHandle = isFamilyHandle;
  const compositeRows = (rows) => {
    const entries = new WasmCompositeRows();
    for (const row of rows) {
      entries.beginRow();
      for (const entry of row) {
        if (isFamilyHandle(entry)) entries.appendFamilyEntry(entry);
        else entries.appendEntry(entry);
      }
    }
    return entries;
  };
  self.noonTableOptions = (...args) => new WasmTableOptions(...args);
  self.noonCodeOptions = (...args) => new WasmCodeOptions(...args);
  self.noonCodeFromFamily = (family) => family.asCode();
  self.noonCodeStylesList = () => noonCodeStylesList();
  self.noonCodeLanguagesList = () => noonCodeLanguagesList();
  self.noonCreateAuthoringCodeHandle = (source, options, context) =>
    context == null ? authoringStore.createCode(source, options)
      : context.liveCreateCode(source, options);
  self.noonTableFromFamily = (family) => family.asTable();
  self.noonHighlightTableCell = (context, selection, column, red, green, blue, alpha, opacity) =>
    context == null ? selection.highlightCell(column, red, green, blue, alpha, opacity)
      : context.liveHighlightTableCell(selection, column, red, green, blue, alpha, opacity);
  self.noonGetHighlightedTableCell = (context, selection, column, red, green, blue, alpha, opacity) =>
    context == null ? selection.getHighlightedCell(column, red, green, blue, alpha, opacity)
      : context.liveGetHighlightedTableCell(selection, column, red, green, blue, alpha, opacity);
  self.noonTableCell = (context, selection, column) => context == null
    ? selection.getCell(column) : context.liveTableCell(selection, column);
  const tableOptions = (v, h, outer, inner, red, green, blue, alpha, width, opacity, join, cap, alignX, alignY) => {
    const options = new WasmTableOptions(v, h, outer);
    options.setIncludeInnerLines(inner);
    options.setLineStrokeColor(red, green, blue, alpha);
    options.setLineStrokeWidth(width);
    options.setLineStrokeOpacity(opacity);
    options.setLineStrokeJoin(join);
    options.setLineStrokeCap(cap);
    options.setCellAlignment(alignX, alignY);
    return options;
  };
  self.noonCreateAuthoringTableHandle = (rows, v, h, outer, inner, red, green, blue, alpha, width, opacity, join, cap, alignX, alignY, context) => {
    const args = [Array.from(rows, row => Array.from(row)), tableOptions(v, h, outer, inner, red, green, blue, alpha, width, opacity, join, cap, alignX, alignY)];
    return context == null ? authoringStore.createTable(...args) : context.liveCreateTable(...args);
  };
  self.noonCreateAuthoringTableWithLabelsHandle = (rows, rowLabels, columnLabels, topLeftEntry, v, h, outer, inner, red, green, blue, alpha, width, opacity, join, cap, alignX, alignY, context) => {
    const args = [
      Array.from(rows, row => Array.from(row)),
      compositeRows(rowLabels == null ? [] : [rowLabels]),
      compositeRows(columnLabels == null ? [] : [columnLabels]),
      compositeRows(topLeftEntry.length ? [topLeftEntry] : []),
      tableOptions(v, h, outer, inner, red, green, blue, alpha, width, opacity, join, cap, alignX, alignY)];
    return context == null ? authoringStore.createTableWithLabels(...args)
      : context.liveCreateTableWithLabels(...args);
  };
  self.noonCreateAuthoringMathTableHandle = (rows, v, h, outer, inner, red, green, blue, alpha, width, opacity, join, cap, alignX, alignY, context) => {
    if (!latexCompiler) throw new Error("Call await prepare_latex() before constructing MathTable");
    return context.liveCreateMathTable(Array.from(rows, row => Array.from(row)), tableOptions(v, h, outer, inner, red, green, blue, alpha, width, opacity, join, cap, alignX, alignY), latexCompiler);
  };
  self.noonCreateAuthoringIntegerTableHandle = (rows, v, h, outer, inner, red, green, blue, alpha, width, opacity, join, cap, alignX, alignY, context) => {
    if (!latexCompiler) throw new Error("Call await prepare_latex() before constructing IntegerTable");
    return context.liveCreateIntegerTable(Array.from(rows, row => Array.from(row)), tableOptions(v, h, outer, inner, red, green, blue, alpha, width, opacity, join, cap, alignX, alignY), latexCompiler);
  };
  self.noonCreateAuthoringDecimalTableHandle = (rows, decimalPlaces, includeSign, groupWithCommas, showEllipsis, unit,
    v, h, outer, inner, red, green, blue, alpha, width, opacity, join, cap, alignX, alignY, context) => {
    if (!latexCompiler) throw new Error("Call await prepare_latex() before constructing DecimalTable");
    return context.liveCreateDecimalTable(
      Array.from(rows, row => Array.from(row)), decimalPlaces, includeSign, groupWithCommas,
      showEllipsis, unit, tableOptions(v, h, outer, inner, red, green, blue, alpha, width, opacity, join, cap, alignX, alignY), latexCompiler);
  };
  self.noonCreateAuthoringMobjectTableHandle = (rows, rowLabels, columnLabels, topLeftEntry, v, h, outer, inner, red, green, blue, alpha, width, opacity, join, cap, alignX, alignY, context) => {
    const entries = compositeRows(rows);
    const rowLabelEntries = compositeRows(rowLabels == null ? [] : [rowLabels]);
    const columnLabelEntries = compositeRows(columnLabels == null ? [] : [columnLabels]);
    const topLeftEntryRows = compositeRows(topLeftEntry.length ? [topLeftEntry] : []);
    const args = [
      entries, rowLabelEntries, columnLabelEntries, topLeftEntryRows,
      tableOptions(v, h, outer, inner, red, green, blue, alpha, width, opacity, join, cap, alignX, alignY)];
    return context == null ? authoringStore.createMobjectTableWithLabels(...args)
      : context.liveCreateMobjectTableWithLabels(...args);
  };
  self.noonCreateAuthoringVariableHandle = (label, value, places, sign, commas, ellipsis, unit, fontSize, context) => {
    if (!latexCompiler) throw new Error("Call await prepare_latex() before constructing Variable");
    if (context == null) throw new Error("Variable requires a canonical Scene authoring context");
    const handle = context.liveCreateVariable(label, value, places, sign, commas, ellipsis, unit, fontSize, latexCompiler);
    return {
      family: () => handle.family(),
      label: () => handle.label(),
      equals: () => handle.equals(),
      tracker: () => handle.tracker(),
      value: () => numericHandle(handle.value()),
    };
  };
  self.noonMatrixOptions = WasmMatrixOptions;
  self.noonLinearTransformationPathArc = noonLinearTransformationPathArc;
  self.noonMatrixFromFamily = (family) => family.asMatrix();
  self.noonCreateAuthoringMatrixHandle = (rows, v, h, bh, bv, stretch, context) => {
    if (!latexCompiler) throw new Error("Call await prepare_latex() before constructing Matrix");
    return context.liveCreateMatrix(Array.from(rows, row => Array.from(row)), new WasmMatrixOptions(v, h, bh, bv, stretch), latexCompiler);
  };
  self.noonCreateAuthoringIntegerMatrixHandle = (rows, v, h, bh, bv, stretch, context) => {
    if (!latexCompiler) throw new Error("Call await prepare_latex() before constructing IntegerMatrix");
    return context.liveCreateIntegerMatrix(Array.from(rows, row => Array.from(row)), new WasmMatrixOptions(v, h, bh, bv, stretch), latexCompiler);
  };
  self.noonCreateAuthoringDecimalMatrixHandle = (rows, v, h, bh, bv, stretch, context) => {
    if (!latexCompiler) throw new Error("Call await prepare_latex() before constructing DecimalMatrix");
    return context.liveCreateDecimalMatrix(Array.from(rows, row => Array.from(row)), new WasmMatrixOptions(v, h, bh, bv, stretch), latexCompiler);
  };
  self.noonCreateAuthoringMobjectMatrixHandle = (rows, v, h, bh, bv, stretch, context) => {
    if (!latexCompiler) throw new Error("Call await prepare_latex() before constructing MobjectMatrix");
    const entries = compositeRows(rows);
    return context.liveCreateMobjectMatrix(entries, new WasmMatrixOptions(v, h, bh, bv, stretch), latexCompiler);
  };
  self.noonNumericFromMobject = (mobject) => numericHandle(authoringStore.numericFromMobject(mobject));
  self.noonAuthoringMembershipBatch = (kind) => new WasmSceneMembershipBatch(kind);
  self.noonCreateAuthoringFamilyHandle = (batch, zIndex, drawEnabled, ...color) => color.length
    ? authoringStore.createFamilyWithColor(batch, zIndex, ...color, drawEnabled)
    : authoringStore.createFamily(batch, zIndex, drawEnabled);
  self.noonResolveAnimationOptions = (...args) => resolveAnimationOptionsPlain(resolveAnimationOptions, ...args);
  self.noonResolveCreateAnimationOptions = (...args) => resolveAnimationOptionsPlain(resolveCreateAnimationOptions, ...args);
  self.noonResolveGlowPulseAnimationOptions = (...args) => resolveAnimationOptionsPlain(resolveGlowPulseAnimationOptions, ...args);
  self.noonResolveTransformAnimationOptions = (...args) =>
    resolveAnimationOptionsPlain(resolveTransformAnimationOptions, ...args);
  const bindingsReadyAt = performance.now();

  for (const [index, descriptor] of PYTHON_COMPAT_MODULES.entries()) {
    pyodide.FS.writeFile(descriptor.runtimePath, compatibilityModules[index].source, {
      encoding: "utf8",
    });
  }
  const compatibilityFilesReadyAt = performance.now();

  pyodide.runPython(`
import sys
sys.path.insert(0, "/tmp")
import noon
import _manim_namespace
_manim_namespace.install()
`);
  const importsReadyAt = performance.now();
  const startupMetrics = {
    version: AUTHORING_STARTUP_METRICS_VERSION,
    ...authoringMilestones,
    totalMs: importsReadyAt,
    moduleGraphLoadMs: moduleGraphReadyAt,
    initializeMs: importsReadyAt - initializeStartedAt,
    startupResourcesMs: resourcesReadyAt - initializeStartedAt,
    noonWebInitMs: resourceDurations.noonWebInitMs,
    noonWebInitReadyAtMs: resourceDurations.noonWebInitReadyAtMs,
    pyodideInitMs: resourceDurations.pyodideInitMs,
    pyodideInitReadyAtMs: resourceDurations.pyodideInitReadyAtMs,
    compatibilityBundleMs: resourceDurations.compatibilityBundleMs,
    compatibilityBundleReadyAtMs: resourceDurations.compatibilityBundleReadyAtMs,
    authoringBindingsMs: bindingsReadyAt - resourcesReadyAt,
    compatibilityFsInstallMs: compatibilityFilesReadyAt - bindingsReadyAt,
    compatibilityImportInstallMs: importsReadyAt - compatibilityFilesReadyAt,
    compatibilityModuleCount: compatibilityModules.length,
    compatibilitySourceChars: compatibilityModules.reduce(
      (total, module) => total + module.source.length,
      0,
    ),
    resourcesReadyAtMs: resourcesReadyAt,
    authoringStoreCreatedAtMs: authoringStoreCreatedAt,
    bindingsReadyAtMs: bindingsReadyAt,
    compatibilityFilesReadyAtMs: compatibilityFilesReadyAt,
    importsReadyAtMs: importsReadyAt,
  };
  Object.defineProperties(startupMetrics, {
    firstSceneContextCreatedAtMs: {
      enumerable: true,
      get: () => authoringMilestones.firstSceneContextCreatedAtMs,
    },
    authoringWorkerReadyAtMs: {
      enumerable: true,
      get: () => authoringMilestones.authoringWorkerReadyAtMs,
    },
  });
  self.__noonAuthoringStartupMetrics = Object.freeze(startupMetrics);
  return pyodide;
}

function measureStartupTask(metrics, key, task) {
  const startedAt = performance.now();
  let result;
  try {
    result = task();
  } catch (error) {
    throw error;
  }
  return Promise.resolve(result).then((value) => {
    metrics[key] = performance.now() - startedAt;
    metrics[`${key.replace(/Ms$/, "")}ReadyAtMs`] = performance.now();
    return value;
  });
}

async function loadCompatibilityBundle() {
  const response = await fetch(new URL("./python/compat-bundle.json", import.meta.url));
  if (!response.ok) {
    throw new Error(`Unable to load Noon Python compatibility bundle: HTTP ${response.status}`);
  }
  const bundle = await response.json();
  if (!isRecord(bundle) || bundle.version !== 1 || !Array.isArray(bundle.modules)) {
    throw new Error("Noon Python compatibility bundle has an invalid envelope");
  }
  if (bundle.modules.length !== PYTHON_COMPAT_MODULES.length) {
    throw new Error(
      `Noon Python compatibility bundle module count ${bundle.modules.length} does not match manifest ${PYTHON_COMPAT_MODULES.length}`,
    );
  }
  for (const [index, descriptor] of PYTHON_COMPAT_MODULES.entries()) {
    const module = bundle.modules[index];
    if (
      !isRecord(module) ||
      module.runtimePath !== descriptor.runtimePath ||
      module.label !== descriptor.label ||
      typeof module.source !== "string"
    ) {
      throw new Error(`Noon Python compatibility bundle is stale at ${descriptor.sourcePath}`);
    }
  }
  return bundle.modules;
}


function registerSemanticExecution(context) {
  const run = activeAuthoringRun;
  const continuation = run?.continuation;
  if (run?.preparedPlayback && continuation &&
      (continuation.context !== context || continuation.terminal)) {
    throw new Error("prepared source must return its original context after a readable play or wait barrier");
  }
  if (continuation?.context === context) return continuation.contextId;
  if (run?.preparedPlayback &&
      (run.completedPreparedContextId !== null ||
       typeof context.createFinishedPreparedExecutionPlayer !== "function")) {
    throw new Error("prepared source requires one original context and a prepared-prefix enabled engine");
  }
  // This remains the authoring-run boundary, including returned-player recovery.
  if (run?.preparedPlayback) invokePreparedEngine(() => context.prepareExecutionRun());
  else context.prepareExecutionRun();
  const token = `semantic-${nextSemanticContext++}`;
  semanticContexts.set(token, { context, endpoints: new Set(), released: false,
    ...(run?.preparedPlayback ? { preparedPlayback: true, preparedSourceFinished: false } : {}),
  });
  if (run?.preparedPlayback) run.completedPreparedContextId = token;
  return token;
}

function registerContinuationContext(context) {
  if (activeAuthoringRun === null) {
    throw new Error("semantic continuation requires an active Python authoring run");
  }
  if (activeAuthoringRun.preparedPlayback &&
      ["failed", "cancelled", "finished"].includes(activeAuthoringRun.preparedDriver.state())) {
    throw new Error("prepared source invocation is retired");
  }
  if (activeAuthoringRun.preparedPlayback &&
      (typeof context.prepareInitialSourceSegment !== "function" ||
       typeof context.createPreparedExecutionPlayer !== "function")) {
    throw new Error("prepared playback requires a prepared-prefix enabled engine build");
  }
  if (activeAuthoringRun.continuation !== null) {
    if (activeAuthoringRun.continuation.context !== context) {
      throw new Error("one Python authoring run cannot suspend multiple semantic contexts");
    }
    return activeAuthoringRun.continuation;
  }
  if (activeAuthoringRun.preparedPlayback) invokePreparedEngine(() => context.prepareExecutionRun());
  else context.prepareExecutionRun();
  const callbackSession = activeAuthoringRun.continuationCallbackSession;
  if (callbackSession !== null && callbackSession.context !== context) {
    throw new Error("semantic continuation callback session belongs to another context");
  }
  const contextId = `semantic-${nextSemanticContext++}`;
  const generation = nextContinuationGeneration++;
  const preparedPlayback = activeAuthoringRun.preparedPlayback === true;
  const entry = {
    context,
    endpoints: new Set(),
    released: false,
    preparedPlayback,
    ...(callbackSession === null ? {} : { callbackSessionId: callbackSession.sessionId }),
  };
  const continuation = {
    context,
    contextId,
    generation,
    runRequestId: activeAuthoringRun.requestId,
    endpoint: null,
    callbackRead: null,
    pending: null,
    callbackRequest: null,
    callbackCommit: null,
    committedCallbackPlayer: null,
    terminal: false,
    preparedPlayback,
    preparedDriver: preparedPlayback ? activeAuthoringRun.preparedDriver : null,
    preparedPlayer: null,
    preparedAttachmentStarted: false,
    preparedBootstrapObserved: false,
    observedExecution: false,
    initialSourceReady: preparedPlayback ? preparedEndpointReady() : null,
    preparedSourceFinished: false,
    preparedReady: preparedPlayback ? preparedEndpointReady() : null,
  };
  semanticContexts.set(contextId, entry);
  activeAuthoringRun.continuation = continuation;
  post("semantic_continuation_registered", {
    requestId: activeAuthoringRun.requestId,
    generation,
    semanticExecution: {
      context_id: contextId,
      continuation_generation: generation,
      ...(preparedPlayback ? { prepared_playback: true } : {}),
      ...(callbackSession === null ? {} : { callback_session_id: callbackSession.sessionId }),
    },
    duration: Number(preparedPlayback
      ? invokePreparedEngine(() => context.liveHandoffDuration()) : context.liveHandoffDuration()),
  });
  return continuation;
}

function awaitSemanticContinuation(context, start = null) {
  let continuation;
  try {
    continuation = registerContinuationContext(context);
    if (continuation.terminal) {
      throw new Error("semantic continuation is terminal");
    }
    if (continuation.preparedPlayback) return continuation.preparedDriver.awaitSegment(context);
  } catch (error) {
    return Promise.reject(error);
  }
  return awaitObservedContinuation(continuation, start);
}

function awaitObservedContinuation(continuation, start = null) {
  if (continuation.pending !== null) {
    return Promise.reject(new Error("semantic continuation already has a pending await"));
  }
  const result = new Promise((resolve, reject) => {
    continuation.pending = { resolve, reject };
  });
  if (start !== null) {
    const pending = continuation.pending;
    Promise.resolve().then(start).catch(error => {
      if (continuation.pending === pending) {
        continuation.pending = null;
        pending.reject(error);
      } else if (!continuation.terminal) {
        failContinuation(continuation, error);
      }
    });
  } else if (continuation.endpoint !== null) {
    try {
      continuation.endpoint.startContinuation(continuation.generation);
    } catch (error) {
      failContinuation(continuation, error);
    }
  }
  return result;
}

function preparedEndpointReady() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  // Cancellation can arrive before the driver's first queued preparation task.
  // Its rejection is still awaited there, without a worker-fatal orphan promise.
  void promise.catch(() => {});
  return { promise, resolve, reject };
}

function invokePreparedEngine(callback) {
  if (fatalAuthoringFailure) throw new Error("prepared engine is retired after a fatal failure");
  try { return callback(); }
  catch (error) {
    // These typed calls bypass the Python facade's trap reporter. A trapped
    // WASM instance cannot be unwound, freed or reused by host recovery.
    if (error instanceof WebAssembly.RuntimeError) failAuthoringWorker(error);
    // Rust bindings may reject with a JsValue string. Pyodide's Promise bridge
    // requires an Error; keep the original value as its cause, without retrying.
    throw error instanceof Error ? error : new Error(String(error), { cause: error });
  }
}

function requirePreparedContinuation(run, context, signal = null) {
  signal?.throwIfAborted();
  const continuation = run.continuation;
  const entry = continuation && semanticContexts.get(continuation.contextId);
  if (activeAuthoringRun !== run || !continuation?.preparedPlayback ||
      continuation.context !== context || continuation.terminal || !entry || entry.released) {
    throw new Error("prepared source context or invocation is retired");
  }
  return continuation;
}

function deliverPreparedSourcePrefix(continuation) {
  const player = continuation.preparedPlayer;
  if (!invokePreparedEngine(() => player.reserveSourcePrefixCapacity())) return false;
  if (!invokePreparedEngine(() => player.deliverSourcePrefix())) throw new Error("prepared source lost its retained suffix");
  // Future history is not a presentation. No transport/presentation await is
  // needed to release this source barrier, even while the reader is paused.
  continuation.endpoint.sourcePrefixDelivered();
  return true;
}

function createPreparedSourceDriver(run) {
  return createPreparedInterpreterDriver({
    async prepareSegment(context, signal) {
      const continuation = requirePreparedContinuation(run, context, signal);
      if (continuation.observedExecution) return awaitObservedContinuation(continuation);
      if (!continuation.preparedBootstrapObserved) {
        if (invokePreparedEngine(() => context.beginObservedPreparedSource(run.sourceSceneOnly === true))) {
          continuation.observedExecution = true;
          // Install the original stack's await before attachment can request its
          // first callback. There is no reader or replacement source invocation.
          const event = awaitObservedContinuation(continuation);
          continuation.initialSourceReady.resolve();
          return event;
        }
        // The interpreter has suspended before this queued worker turn. A zero
        // barrier releases once; source resumes on another turn, never in a loop.
        let readable;
        try { readable = invokePreparedEngine(() => context.prepareInitialSourceSegment()); }
        catch (error) { continuation.initialSourceReady.reject(error); throw error; }
        if (!readable) return;
        continuation.initialSourceReady.resolve();
        await continuation.preparedReady.promise;
        requirePreparedContinuation(run, context, signal);
        continuation.preparedBootstrapObserved = true;
        return;
      }
      const prepared = invokePreparedEngine(() => continuation.preparedPlayer.prepareSourceSegment(context));
      if (prepared === false) {
        // This is still the original suspended play/wait. Only a reader request
        // at the required frontier may activate its normal callback protocol.
        const event = awaitContinuationEvent(continuation);
        continuation.endpoint.sourceExecutionRequired();
        return event;
      }
      deliverPreparedSourcePrefix(continuation);
    },
    needsPreparation() {
      const continuation = requirePreparedContinuation(run, run.continuation?.context);
      if (!continuation.preparedPlayer) return true; // Continue initial zero barriers cooperatively.
      return invokePreparedEngine(() => continuation.preparedPlayer.needsPreparation(run.preparationLookaheadSeconds));
    },
    async finishSource(resultJson, signal) {
      if (run.continuation === null) {
        signal.throwIfAborted();
        const result = JSON.parse(resultJson), descriptor = result?.semantic_execution;
        const entry = semanticContexts.get(run.completedPreparedContextId);
        if (activeAuthoringRun !== run || !entry || entry.released || result.duration !== 0 ||
            descriptor?.context_id !== run.completedPreparedContextId ||
            descriptor.continuation_generation != null || descriptor.callback_session_id != null) {
          throw new Error("prepared source produced no readable play or wait interval or completed static scene");
        }
        // Completion includes construct/finally/tear_down and per-run cleanup.
        // Rust validates the exact zero-duration domain when the reader attaches.
        entry.preparedSourceFinished = true;
        return;
      }
      const continuation = requirePreparedContinuation(run, run.continuation?.context, signal);
      const descriptor = JSON.parse(resultJson)?.semantic_execution;
      if (descriptor?.context_id !== continuation.contextId ||
          descriptor.continuation_generation !== continuation.generation) {
        throw new Error("prepared source must return its original context and continuation");
      }
      if (continuation.observedExecution) {
        await continuation.endpoint.publishContinuationResult(continuation.generation);
        requirePreparedContinuation(run, continuation.context, signal);
        continuation.terminal = true;
        return;
      }
      if (descriptor.callback_session_id != null) {
        throw new Error("historical playback cannot discard Python callbacks");
      }
      if (!continuation.preparedBootstrapObserved) {
        // All actual barriers were zero-duration. Only successful source cleanup
        // authorizes Rust to seal the final same-time edits at the exact endpoint.
        if (JSON.parse(resultJson).duration !== 0) throw new Error("prepared source produced no readable interval");
        continuation.preparedSourceFinished = true;
        continuation.initialSourceReady.resolve();
        await continuation.preparedReady.promise;
        requirePreparedContinuation(run, continuation.context, signal);
        continuation.terminal = true;
        return;
      }
      // runAuthoringSource includes the existing construct/finally/tear_down and
      // per-run globals cleanup. Seal only after that entire operation succeeds.
      if (invokePreparedEngine(() => continuation.preparedPlayer.finishSourcePreparation(continuation.context)) === false) {
        // Source and cleanup have returned. Retain their completed canonical
        // player until actual reader demand reaches its final native state.
        continuation.preparedSourceFinished = true;
        const handedOff = awaitContinuationEvent(continuation);
        continuation.endpoint.sourceExecutionRequired();
        await handedOff;
        requirePreparedContinuation(run, continuation.context, signal);
        continuation.terminal = true;
        return;
      }
      if (!deliverPreparedSourcePrefix(continuation)) {
        throw new Error("prepared source did not deliver its explicit final endpoint");
      }
      continuation.terminal = true;
    },
  });
}

function continuationEvent(kind, value = {}) {
  return JSON.stringify({ kind, ...value });
}

function requestContinuationCallback(continuation, phase, player) {
  if (continuation.terminal || continuation.pending === null) {
    return Promise.reject(new Error("required callback reached a continuation without a suspended source"));
  }
  if (continuation.callbackRequest !== null) {
    return Promise.reject(new Error("semantic continuation already has a required callback request"));
  }
  if (continuation.callbackCommit !== null) {
    return Promise.reject(new Error("semantic continuation callback commit is not acknowledged"));
  }
  let phaseTokenJson;
  try {
    phaseTokenJson = JSON.stringify(phase?.token);
  } catch (error) {
    return Promise.reject(new Error(`canonical callback phase token is not serializable: ${error}`));
  }
  if (phaseTokenJson === undefined) {
    return Promise.reject(new Error("canonical callback phase is missing its token"));
  }
  return new Promise((resolve, reject) => {
    continuation.committedCallbackPlayer = null;
    continuation.callbackRequest = { phaseTokenJson, resolve, reject, read: null, player };
    const pending = continuation.pending;
    continuation.pending = null;
    pending.resolve(continuationEvent("callback", { phase }));
  });
}

function stageContinuationCallbackMembership(context, tokenJson, batch) {
  const callback = continuationCallbackRequest(context, tokenJson).callbackRequest;
  if (callback.player === null || callback.player === undefined ||
      typeof callback.player.stageCallbackMembership !== "function") {
    throw new Error("semantic continuation callback has no pinned membership collector");
  }
  callback.player.stageCallbackMembership(tokenJson, batch);
}

function continuationMembershipRootKeys(context, tokenJson) {
  const callback = continuationCallbackRequest(context, tokenJson).callbackRequest;
  if (callback.player === null || callback.player === undefined ||
      typeof callback.player.callbackMembershipRootKeys !== "function") {
    throw new Error("semantic continuation has no pinned membership collector");
  }
  return callback.player.callbackMembershipRootKeys(tokenJson);
}

function stageContinuationCallbackProvisionalGeometry(context, tokenJson, options) {
  const callback = continuationCallbackRequest(context, tokenJson).callbackRequest;
  if (callback.player === null || callback.player === undefined ||
      typeof callback.player.stageCallbackProvisionalGeometry !== "function") {
    throw new Error("semantic continuation callback has no pinned provisional geometry collector");
  }
  return callback.player.stageCallbackProvisionalGeometry(tokenJson, options);
}

function stageContinuationCallbackProvisionalShift(context, tokenJson, object, x, y) {
  const callback = continuationCallbackRequest(context, tokenJson).callbackRequest;
  if (callback.player === null || callback.player === undefined ||
      typeof callback.player.stageCallbackProvisionalShift !== "function") {
    throw new Error("semantic continuation callback has no pinned provisional geometry collector");
  }
  callback.player.stageCallbackProvisionalShift(tokenJson, object, x, y);
}

function stageContinuationCallbackProvisionalFill(
  context, tokenJson, object, red, green, blue, alpha, opacity,
) {
  const callback = continuationCallbackRequest(context, tokenJson).callbackRequest;
  if (callback.player === null || callback.player === undefined ||
      typeof callback.player.stageCallbackProvisionalFill !== "function") {
    throw new Error("semantic continuation callback has no pinned provisional geometry collector");
  }
  callback.player.stageCallbackProvisionalFill(
    tokenJson, object, red, green, blue, alpha, opacity,
  );
}

function continuationCallbackProvisionalCenter(context, tokenJson, object) {
  const callback = continuationCallbackRequest(context, tokenJson).callbackRequest;
  if (callback.player === null || callback.player === undefined ||
      typeof callback.player.callbackProvisionalCenter !== "function") {
    throw new Error("semantic continuation callback has no pinned provisional geometry collector");
  }
  return callback.player.callbackProvisionalCenter(tokenJson, object);
}

function committedContinuationCallbackPlayer(context, tokenJson) {
  const continuation = activeAuthoringRun?.continuation;
  if (!continuation || continuation.context !== context || continuation.terminal ||
      continuation.callbackCommit?.tokenJson !== tokenJson) {
    throw new Error("semantic continuation has no completed callback player");
  }
  const completed = continuation.committedCallbackPlayer;
  const player = completed?.tokenJson === tokenJson ? completed.player : null;
  if (player === null || player === undefined) {
    throw new Error("semantic continuation callback has no committed provisional geometry collector");
  }
  return player;
}

function resolveContinuationCallbackProvisionalMobject(context, tokenJson, object) {
  const player = committedContinuationCallbackPlayer(context, tokenJson);
  return player.resolveCallbackProvisionalMobject(tokenJson, object);
}

function continuationCallbackRequest(context, tokenJson) {
  const continuation = activeAuthoringRun?.continuation;
  if (!continuation || continuation.context !== context || continuation.terminal ||
      continuation.callbackRequest === null) {
    throw new Error("semantic continuation has no pending required callback");
  }
  if (typeof tokenJson !== "string" || tokenJson !== continuation.callbackRequest.phaseTokenJson) {
    throw new Error("semantic continuation callback token is stale");
  }
  return continuation;
}

function parseContinuationCallbackReadRequest(requestJson) {
  if (typeof requestJson !== "string" || requestJson.trim() === "") {
    throw new TypeError("canonical callback read request must be non-empty JSON");
  }
  let request;
  try {
    request = JSON.parse(requestJson);
  } catch (error) {
    throw new TypeError(`canonical callback read request is not valid JSON: ${error}`);
  }
  if (!isRecord(request) ||
      !Number.isSafeInteger(request.request_id) || request.request_id < 0 ||
      !["scalar_signal", "object", "family"].includes(request.kind) || !isRecord(request.node) ||
      !Number.isSafeInteger(request.node.slot) || request.node.slot < 0 || request.node.slot > 0xffffffff ||
      !Number.isSafeInteger(request.node.generation) || request.node.generation < 0 || request.node.generation > 0xffffffff) {
    throw new TypeError("canonical callback read request must contain a request ID, typed kind, and semantic node");
  }
  return request;
}

function readContinuationCallback(context, tokenJson, requestJson) {
  let continuation;
  let request;
  try {
    continuation = continuationCallbackRequest(context, tokenJson);
    request = parseContinuationCallbackReadRequest(requestJson);
  } catch (error) {
    return Promise.reject(error);
  }
  const callback = continuation.callbackRequest;
  if (callback.read !== null) {
    return Promise.reject(new Error("semantic continuation already has a callback read in flight"));
  }
  if (continuation.callbackRead === null) {
    return Promise.reject(new Error("semantic continuation callback sparse reads are unavailable"));
  }
  return new Promise((resolve, reject) => {
    const read = { requestId: request.request_id, resolve, reject };
    callback.read = read;
    Promise.resolve()
      .then(() => continuation.callbackRead(tokenJson, request))
      .then((result) => {
        if (continuation.terminal || continuation.callbackRequest !== callback || callback.read !== read) {
          return;
        }
        callback.read = null;
        resolve(result);
      })
      .catch((error) => {
        if (continuation.callbackRequest === callback && callback.read === read) {
          callback.read = null;
          reject(error instanceof Error ? error : new Error(String(error)));
        }
      });
  });
}

function awaitContinuationEvent(continuation) {
  if (continuation.terminal) {
    return Promise.reject(new Error("semantic continuation is terminal"));
  }
  if (continuation.pending !== null) {
    return Promise.reject(new Error("semantic continuation already has a pending await"));
  }
  return new Promise((resolve, reject) => {
    continuation.pending = { resolve, reject };
  });
}

function completeContinuationCallback(context, tokenJson, patchBatchJson) {
  if (typeof patchBatchJson !== "string" || patchBatchJson.trim() === "") {
    throw new TypeError("semantic continuation callback result must be non-empty JSON");
  }
  const continuation = continuationCallbackRequest(context, tokenJson);
  const callback = continuation.callbackRequest;
  if (callback.read !== null) {
    throw new Error("semantic continuation callback cannot complete while a callback read is pending");
  }
  const next = awaitContinuationEvent(continuation);
  continuation.committedCallbackPlayer = { tokenJson, player: callback.player };
  continuation.callbackRequest = null;
  callback.resolve(patchBatchJson);
  return next;
}

// Rust has published, but the suspended source must attach its Python wrappers
// before the endpoint starts another phase or returns/consumes the player lease.
function requestContinuationCallbackCommit(continuation, phase) {
  const tokenJson = JSON.stringify(phase.token);
  if (continuation.terminal || continuation.pending === null ||
      continuation.callbackRequest !== null || continuation.callbackCommit !== null ||
      continuation.committedCallbackPlayer?.tokenJson !== tokenJson) {
    return Promise.reject(new Error("stale semantic continuation callback commit"));
  }
  return new Promise((resolve, reject) => {
    continuation.callbackCommit = { tokenJson, resolve, reject };
    const pending = continuation.pending;
    continuation.pending = null;
    pending.resolve(continuationEvent("callback_committed", {
      phase: { token: phase.token, region: phase.region ?? 0 },
    }));
  });
}

function acknowledgeContinuationCallback(context, tokenJson, failure = null) {
  const continuation = activeAuthoringRun?.continuation;
  if (!continuation || continuation.context !== context || continuation.terminal ||
      continuation.callbackCommit?.tokenJson !== tokenJson) {
    throw new Error("stale semantic continuation callback acknowledgement");
  }
  const commit = continuation.callbackCommit;
  const next = awaitContinuationEvent(continuation);
  continuation.callbackCommit = null;
  continuation.committedCallbackPlayer = null;
  if (failure === null || failure === undefined) commit.resolve();
  else commit.reject(new Error(String(failure)));
  return next;
}

function failContinuationCallback(context, tokenJson, message) {
  if (typeof message !== "string" || message.trim() === "") {
    throw new TypeError("semantic continuation callback failure requires a message");
  }
  const continuation = continuationCallbackRequest(context, tokenJson);
  const callback = continuation.callbackRequest;
  if (callback.read !== null) {
    callback.read.reject(new Error(message));
    callback.read = null;
  }
  const next = awaitContinuationEvent(continuation);
  continuation.callbackRequest = null;
  callback.reject(new Error(message));
  return next;
}

function completeContinuation(continuation, generation) {
  if (continuation.terminal || generation !== continuation.generation ||
      continuation.pending === null || continuation.callbackRequest !== null ||
      continuation.callbackCommit !== null) {
    throw new Error("stale semantic continuation completion");
  }
  const { resolve } = continuation.pending;
  continuation.pending = null;
  resolve(continuationEvent("complete"));
}

function failContinuation(continuation, error) {
  if (continuation.terminal) return;
  continuation.terminal = true;
  if (continuation.preparedPlayback) {
    continuation.preparedDriver.cancel(error);
    continuation.initialSourceReady.reject(error);
    continuation.preparedReady.reject(error);
    if (!fatalAuthoringFailure) invokePreparedEngine(() => continuation.preparedPlayer?.stopSourcePreparation());
  }
  continuation.committedCallbackPlayer = null;
  if (continuation.callbackCommit !== null) {
    continuation.callbackCommit.reject(error instanceof Error ? error : new Error(String(error)));
    continuation.callbackCommit = null;
  }
  if (continuation.callbackRequest !== null) {
    const callback = continuation.callbackRequest;
    continuation.callbackRequest = null;
    const failure = error instanceof Error ? error : new Error(String(error));
    if (callback.read !== null) callback.read.reject(failure);
    callback.reject(failure);
  }
  if (continuation.pending !== null) {
    const { reject } = continuation.pending;
    continuation.pending = null;
    reject(error instanceof Error ? error : new Error(String(error)));
  }
}

function isContinuationControl(request) {
  return isRecord(request) && request.channel === AUTHORING_CHANNEL &&
    (request.type === "release_semantic_execution" ||
      request.type === "cancel_semantic_continuation" ||
      (request.type === "attach_semantic_execution" &&
       (request.continuationGeneration !== undefined || request.replaceExistingEndpoint === true)));
}

async function handleContinuationControl(request) {
  let requestId = null;
  try {
    validateRequest(request);
    requestId = request.requestId;
    const pyodide = await pyodidePromise;
    if (request.type === "attach_semantic_execution") {
      await attachSemanticExecutionRequest(request, request.continuationGeneration !== undefined, pyodide);
      post("semantic_execution_attached", { requestId });
      return;
    }
    if (request.type === "release_semantic_execution") {
      const entry = semanticContexts.get(request.contextId);
      const continuation = activeAuthoringRun?.continuation;
      if (continuation !== null && continuation !== undefined &&
          continuation.contextId === request.contextId && !continuation.terminal) {
        throw new Error("cannot release an active semantic continuation context");
      }
      if (entry) {
        // This acknowledgement is the pre-authoring source-release barrier.
        // Stop on the owning worker, so a cross-port stop cannot race the next
        // Run's writes into the same semantic arena. Recovery still uses the
        // ordinary endpoint stop/return path without retiring its source.
        for (const endpoint of [...entry.endpoints]) endpoint.stop({ retireSource: true });
        entry.released = true;
        try {
          retireSemanticContext(request.contextId, entry);
        } catch (error) {
          entry.released = false;
          throw error;
        }
      }
      post("semantic_execution_released", { requestId });
      return;
    }
    const entry = semanticContexts.get(request.contextId);
    const continuation = activeAuthoringRun?.continuation;
    if (!entry || continuation === null || continuation === undefined ||
        continuation.contextId !== request.contextId ||
        continuation.generation !== request.continuationGeneration ||
        continuation.runRequestId !== request.continuationRunRequestId ||
        activeAuthoringRun.requestId !== request.continuationRunRequestId) {
      throw new Error("stale semantic continuation cancellation");
    }
    failContinuation(continuation, new Error(request.reason));
    entry.released = true;
    for (const endpoint of [...entry.endpoints]) endpoint.stop();
    if (continuation.preparedPlayback && continuation.endpoint === null) {
      retireSemanticContext(request.contextId, entry);
    }
    post("semantic_continuation_cancelled", { requestId });
  } catch (error) {
    if (request?.type === "attach_semantic_execution") {
      request.controlPort?.close?.();
      request.renderPort?.close?.();
    }
    postError(requestId, error);
  }
}

async function handleRequest(request) {
  let requestId = null;
  try {
    validateRequest(request);
    requestId = request.requestId;
    const pyodide = await pyodidePromise;
    if (request.type === "run") {
      const run = {
        requestId, continuation: null, continuationCallbackSession: null,
        preparedPlayback: request.preparedPlayback === true,
        sourceSceneOnly: false,
        preparationLookaheadSeconds: request.preparationLookaheadSeconds ?? 1,
        preparedDriver: null, completedPreparedContextId: null,
      };
      activeAuthoringRun = run;
      if (run.preparedPlayback) run.preparedDriver = createPreparedSourceDriver(run);
      let completed = false;
      try {
        const workerRunStartedAtMs = performance.now();
        const execute = () => runAuthoringSource(
          pyodide,
          request.source,
          request.context,
          requestId,
        );
        let resultJson = await (run.preparedDriver ? run.preparedDriver.run(execute) : execute());
        const workerRunCompletedAtMs = performance.now();
        if (run.preparedPlayback) {
          const result = JSON.parse(resultJson);
          result.semantic_execution.prepared_playback = true;
          resultJson = JSON.stringify(result);
        } else if (run.continuation !== null) {
          await run.continuation.endpoint.publishContinuationResult(run.continuation.generation);
        }
        post("result", {
          requestId,
          resultJson,
          workerRunTiming: {
            performanceTimeOriginMs: performance.timeOrigin,
            startedAtMs: workerRunStartedAtMs,
            completedAtMs: workerRunCompletedAtMs,
          },
        });
        completed = true;
      } finally {
        if (!completed && run.continuation !== null) {
          const entry = semanticContexts.get(run.continuation.contextId);
          failContinuation(run.continuation, new Error("Python authoring continuation failed"));
          if (fatalAuthoringFailure) {
            // The fatal channel owns worker retirement. Never call back into
            // a trapped WASM engine while the Python source unwinds.
          } else if (!run.preparedPlayback || run.continuation.observedExecution) {
            if (entry) entry.released = true;
            run.continuation.endpoint?.stop();
          } else if (entry && run.continuation.endpoint === null &&
                     !run.continuation.preparedAttachmentStarted) {
            entry.released = true;
            retireSemanticContext(run.continuation.contextId, entry);
          }
        }
        if (!completed && !fatalAuthoringFailure && run.completedPreparedContextId !== null) {
          const entry = semanticContexts.get(run.completedPreparedContextId);
          if (entry) {
            entry.released = true;
            retireSemanticContext(run.completedPreparedContextId, entry);
          }
        }
        if (activeAuthoringRun === run) activeAuthoringRun = null;
      }
      return;
    }
    if (request.type === "attach_semantic_execution") {
      await attachSemanticExecutionRequest(request, false, pyodide);
      post("semantic_execution_attached", { requestId });
      return;
    }
    throw new Error(`Unsupported Python authoring request: ${request.type}`);
  } catch (error) {
    if (request?.type === "attach_semantic_execution") {
      request.controlPort?.close?.();
      request.renderPort?.close?.();
    }
    postError(requestId, error);
  }
}

async function attachSemanticExecutionRequest(request, continuationOnly, pyodide) {
  if (request.sourceHandoff !== undefined && typeof request.sourceHandoff !== "boolean") {
    throw new TypeError("sourceHandoff must be boolean");
  }
  const entry = semanticContexts.get(request.contextId);
  if (!entry || entry.released) throw new Error("unknown or retired semantic execution context");
  const continuation = activeAuthoringRun?.continuation;
  if (entry.preparedPlayback && request.replaceExistingEndpoint === true) {
    if (continuationOnly || request.sourceHandoff === true) throw new Error("receiver replacement cannot restart source continuation attachment");
    return replacePreparedSemanticReceiver(request, entry);
  }
  if (continuationOnly) {
    if (!continuation || continuation.contextId !== request.contextId ||
        continuation.generation !== request.continuationGeneration ||
        continuation.runRequestId !== request.continuationRunRequestId ||
        activeAuthoringRun?.requestId !== request.continuationRunRequestId ||
        continuation.terminal) {
      throw new Error("stale semantic continuation attachment");
    }
    if (continuation.endpoint !== null && request.sourceHandoff !== true) {
      throw new Error("semantic continuation endpoint is already attached");
    }
  }
  if (request.sourceHandoff === true) {
    if (!continuationOnly || !entry.preparedPlayback || !continuation?.preparedPlayer ||
        continuation.observedExecution || request.replaceExistingEndpoint || request.pacing !== "realtime") {
      throw new Error("source handoff requires its original prepared continuation");
    }
    const run = activeAuthoringRun, previousEndpoint = continuation.endpoint;
    // Keep registry retirement behind the asynchronous lease attachment. A
    // cancellation stops the old reader immediately; the candidate must return
    // or discard its source lease before this reservation can be relinquished.
    const pendingAttachment = { stop() {} };
    entry.endpoints.add(pendingAttachment);
    let endpoint;
    try {
      const playing = invokePreparedEngine(() => continuation.preparedPlayer.beginSourceExecution(entry.context));
      continuation.observedExecution = true;
      endpoint = await attachObservedSemanticExecution({ ...request, initiallyPaused: !playing,
        loopDurationSeconds: Math.max(1, entry.context.liveHandoffDuration()) }, entry, continuation, true, pyodide);
      requirePreparedContinuation(run, entry.context);
      // This finishes only the host's join: the original source stack and its
      // last logical segment already completed. No renderer receipt is forged.
      if (continuation.preparedSourceFinished) completeContinuation(continuation, continuation.generation);
      return endpoint;
    } catch (error) {
      entry.released = true;
      failContinuation(continuation, error);
      if (!fatalAuthoringFailure) { endpoint?.stop(); previousEndpoint.stop(); }
      throw error;
    } finally {
      entry.endpoints.delete(pendingAttachment);
      if (!fatalAuthoringFailure) retireSemanticContext(request.contextId, entry);
    }
  }
  if (entry.preparedPlayback) {
    if (entry.preparedSourceFinished === true && !continuationOnly && !request.replaceExistingEndpoint) {
      if (request.callbackSessionId != null || request.pacing !== "realtime") {
        throw new Error("prepared playback requires realtime pacing without Python callbacks");
      }
      return attachFinishedPreparedSemanticExecution(request, entry, pyodide);
    }
    if (!continuationOnly || !continuation?.preparedPlayback || request.replaceExistingEndpoint) {
      throw new Error("prepared playback cannot reattach or replace its endpoint");
    }
    if (request.pacing !== "realtime") {
      throw new Error("prepared playback requires realtime pacing");
    }
    return attachPreparedSemanticExecution(request, entry, continuation, pyodide);
  }
  return attachObservedSemanticExecution(request, entry, continuation, continuationOnly, pyodide);
}

async function attachObservedSemanticExecution(request, entry, continuation, continuationOnly, pyodide) {
  if (request.callbackSessionId !== null && request.callbackSessionId !== undefined) {
    if (entry.callbackSessionId !== undefined && entry.callbackSessionId !== request.callbackSessionId) {
      throw new Error("semantic callback session does not belong to this execution context");
    }
    entry.callbackSessionId = request.callbackSessionId;
    entry.releaseCallbackSession = () =>
      releaseCanonicalCallbackSession(pyodide, request.callbackSessionId);
  }
  // A source continuation may add its first callback after an earlier play.
  // Its suspended Python stack is the handler for the whole source lifetime.
  const runRequiredCallbackPhase = continuationOnly
    ? (frame, player) => requestContinuationCallback(continuation, frame, player)
    : entry.callbackSessionId === undefined ? null
      : (frame, player) => runCanonicalCallbackPhase(pyodide, entry.callbackSessionId, frame, player);
  const completeRequiredCallbackPhase = continuationOnly
    ? (frame) => requestContinuationCallbackCommit(continuation, frame)
    : entry.callbackSessionId === undefined ? null
      : (frame) => finishCanonicalCallbackPhase(pyodide, entry.callbackSessionId, frame, true);
  if (continuationOnly) {
    entry.releaseCallbackSession = () => entry.callbackSessionId === undefined
      ? undefined : releaseCanonicalCallbackSession(pyodide, entry.callbackSessionId);
  }
  const discardRequiredCallbackPhase = entry.callbackSessionId === undefined || continuationOnly
    ? null
    : (frame) => finishCanonicalCallbackPhase(pyodide, entry.callbackSessionId, frame, false);
  if (request.replaceExistingEndpoint) {
    if (continuationOnly || (continuation?.contextId === request.contextId && !continuation.terminal)) {
      throw new Error("cannot replace an endpoint while source continuation is active");
    }
    // Reconnect the control/render ports around the same runtime lease. Stop on
    // this owner before leasing again; cross-port stop messages are not ordered.
    for (const existing of [...entry.endpoints]) existing.stop();
  }
  let endpoint;
  endpoint = await attachSemanticEngine(
    entry.context,
    request,
    () => {
      entry.endpoints.delete(endpoint);
      if (request.completedStaticSource === true) entry.released = true;
      if (continuationOnly && continuation !== undefined) {
        failContinuation(continuation, new Error("semantic continuation endpoint stopped"));
      }
      retireSemanticContext(request.contextId, entry);
    },
    runRequiredCallbackPhase,
    continuationOnly ? {
      generation: continuation.generation,
      completed: continuation.preparedSourceFinished === true && continuation.observedExecution === true,
      onComplete: (generation) => completeContinuation(continuation, generation),
      onError: (_generation, error) => {
        entry.released = true;
        failContinuation(continuation, error);
      },
      onCallbackReadAvailable: (read) => { continuation.callbackRead = read; },
    } : null,
    completeRequiredCallbackPhase,
    discardRequiredCallbackPhase,
  );
  entry.endpoints.add(endpoint);
  if (continuationOnly) continuation.endpoint = endpoint;
  return endpoint;
}

function validatePreparedTransport(request) {
  if (!Number.isInteger(request.session) || request.session < 0 || request.session > 0xffffffff ||
      !["shared", "transferable"].includes(request.transportMode) ||
      !Number.isSafeInteger(request.sharedSlotCapacity) || request.sharedSlotCapacity <= 0) {
    throw new Error("prepared source attachment requires a valid transport and u32 session");
  }
}

async function attachPreparedSemanticExecution(request, entry, continuation, pyodide) {
  if (continuation.preparedAttachmentStarted) throw new Error("prepared source attachment is single-use");
  validatePreparedTransport(request);
  continuation.preparedAttachmentStarted = true;
  const run = activeAuthoringRun;
  const pendingAttachment = { stop() {} };
  entry.endpoints.add(pendingAttachment);
  let endpoint;
  try {
    requirePreparedContinuation(run, entry.context);
    await continuation.initialSourceReady.promise;
    requirePreparedContinuation(run, entry.context);
    if (continuation.observedExecution) {
      endpoint = await attachObservedSemanticExecution(request, entry, continuation, true, pyodide);
      requirePreparedContinuation(run, entry.context);
      continuation.preparedReady.resolve();
      return;
    }
    if (request.callbackSessionId != null || entry.callbackSessionId !== undefined) {
      throw new Error("historical playback cannot discard Python callbacks");
    }
    continuation.preparedPlayer = invokePreparedEngine(() => continuation.preparedSourceFinished
      ? entry.context.createFinishedPreparedExecutionPlayer(request.session)
      : entry.context.createPreparedExecutionPlayer(request.session));
    if (continuation.preparedSourceFinished && continuation.preparedPlayer === undefined) {
      continuation.preparedPlayer = null;
      continuation.observedExecution = true;
      endpoint = await attachObservedSemanticExecution({ ...request, completedStaticSource: true }, entry, continuation, false, pyodide);
      requirePreparedContinuation(run, entry.context);
      continuation.endpoint = endpoint;
      continuation.preparedReady.resolve();
      return;
    }
    const receiver = createPreparedReceiver(request, entry, continuation, continuation.preparedPlayer);
    endpoint = await attachPreparedReceiver(request, entry, receiver, false);
    requirePreparedContinuation(run, entry.context);
    continuation.endpoint = receiver;
    continuation.preparedReady.resolve();
  } catch (error) {
    entry.released = true;
    failContinuation(continuation, error);
    if (!fatalAuthoringFailure && endpoint) endpoint.stop();
    else if (!fatalAuthoringFailure && continuation.preparedPlayer !== null) {
      // Preflight may reject before the endpoint takes ownership. Its onStop
      // clears this handle whenever it has already freed the prepared player.
      invokePreparedEngine(() => continuation.preparedPlayer.free());
      continuation.preparedPlayer = null;
    }
    if (!fatalAuthoringFailure) retireSemanticContext(request.contextId, entry);
    throw error;
  } finally {
    entry.endpoints.delete(pendingAttachment);
    if (!fatalAuthoringFailure) retireSemanticContext(request.contextId, entry);
  }
}

async function attachFinishedPreparedSemanticExecution(request, entry, pyodide) {
  if (entry.preparedAttachmentStarted) throw new Error("prepared source attachment is single-use");
  validatePreparedTransport(request);
  entry.preparedAttachmentStarted = true;
  const pendingAttachment = { stop() {} };
  entry.endpoints.add(pendingAttachment);
  let player, endpoint;
  try {
    player = invokePreparedEngine(() => entry.context.createFinishedPreparedExecutionPlayer(request.session));
    if (player === undefined) {
      player = null;
      endpoint = await attachObservedSemanticExecution({ ...request, completedStaticSource: true }, entry, null, false, pyodide);
      if (entry.released) throw new Error("completed static source was released during attachment");
      return;
    }
    const receiver = createPreparedReceiver(request, entry, null, player);
    player = null; // The receiver now owns cleanup, including failed setup.
    endpoint = await attachPreparedReceiver(request, entry, receiver, false);
    if (entry.released) throw new Error("completed prepared source was released during attachment");
    entry.endpoints.add(endpoint);
  } catch (error) {
    entry.released = true;
    if (!fatalAuthoringFailure) {
      if (endpoint) endpoint.stop();
      else invokePreparedEngine(() => player?.free());
      retireSemanticContext(request.contextId, entry);
    }
    throw error;
  } finally {
    entry.endpoints.delete(pendingAttachment);
    if (!fatalAuthoringFailure) retireSemanticContext(request.contextId, entry);
  }
}

// One lifecycle handle around the same Rust reader. No source, history, clock
// or resource inventory is reconstructed in JavaScript. While detached it also
// keeps registry retirement behind any pending source delivery/renderer setup.
function createPreparedReceiver(request, entry, continuation, player) {
  const receiver = {
    endpoint: null, continuation, replacing: false, attaching: false, stopRequested: false, sourceRequired: false,
    prepared: { player, generation: continuation?.generation,
      wakeSource: () => continuation?.preparedDriver.wake() },
    sourcePrefixDelivered() { receiver.endpoint?.sourcePrefixDelivered(); },
    sourceExecutionRequired() {
      receiver.sourceRequired = true;
      receiver.endpoint?.sourceExecutionRequired();
    },
    stop() {
      if (receiver.attaching) { receiver.stopRequested = true; return; }
      if (receiver.endpoint) { receiver.endpoint.stop(); return; }
      const owned = receiver.prepared.player;
      if (!owned) return;
      receiver.finish(null);
      if (!fatalAuthoringFailure) {
        invokePreparedEngine(() => owned.stopSourcePreparation());
        invokePreparedEngine(() => owned.free());
      }
    },
    finish(retained) {
      entry.endpoints.delete(receiver.endpoint);
      receiver.endpoint = null;
      if (retained) {
        receiver.prepared.player = retained;
        entry.endpoints.add(receiver);
        return;
      }
      receiver.prepared.player = null;
      entry.endpoints.delete(receiver);
      if (entry.preparedReceiver === receiver) entry.preparedReceiver = null;
      if (continuation) continuation.preparedPlayer = null;
      if (!continuation?.observedExecution) {
        entry.released = true;
        if (continuation) failContinuation(continuation, new Error("prepared playback endpoint stopped"));
      }
      retireSemanticContext(request.contextId, entry);
    },
  };
  entry.preparedReceiver = receiver;
  entry.endpoints.add(receiver);
  return receiver;
}

async function attachPreparedReceiver(request, entry, receiver, recovering) {
  receiver.attaching = true;
  let endpoint;
  try {
    endpoint = await attachSemanticEngine(entry.context, request,
      retained => receiver.finish(retained), null, null, null, null, null,
      { ...receiver.prepared, retainOnSetupFailure: recovering });
  } finally { receiver.attaching = false; }
  receiver.endpoint = endpoint;
  entry.endpoints.delete(receiver);
  entry.endpoints.add(endpoint);
  if (entry.released || receiver.stopRequested || entry.preparedReceiver !== receiver) {
    endpoint.stop();
    throw new Error("prepared receiver was retired during attachment");
  }
  if (receiver.continuation) receiver.continuation.endpoint = receiver;
  if (receiver.sourceRequired) endpoint.sourceExecutionRequired();
  return endpoint;
}

async function replacePreparedSemanticReceiver(request, entry) {
  validatePreparedTransport(request);
  const receiver = entry.preparedReceiver;
  if (!receiver?.prepared.player || receiver.continuation?.observedExecution ||
      receiver.replacing || request.callbackSessionId != null || request.pacing !== "realtime") {
    throw new Error("receiver replacement requires its original retained prepared reader");
  }
  receiver.replacing = true;
  try {
    // Rust rejects reused/older namespaces before the old endpoint is detached.
    invokePreparedEngine(() => receiver.prepared.player.resetReceiver(request.session));
    receiver.endpoint?.detachPreparedReceiver();
    const initiallyPaused = !invokePreparedEngine(() => receiver.prepared.player.preparedPlaying());
    await attachPreparedReceiver({ ...request, initiallyPaused }, entry, receiver, true);
  } catch (error) {
    if ((entry.released || receiver.stopRequested) && !fatalAuthoringFailure) receiver.stop();
    throw error;
  } finally {
    receiver.replacing = false;
    if (!fatalAuthoringFailure) retireSemanticContext(request.contextId, entry);
  }
}

function retireSemanticContext(token, entry) {
  if (entry.released && entry.endpoints.size === 0) {
    // A Python Scene or Mobject may retain this exact semantic arena. Release
    // input-owned attachments through its canonical owner before relinquishing
    // our reference; failed retirement must leave the owner available.
    entry.context.retireExecutionSource();
    semanticContexts.delete(token);
    // Cancellation may retire the endpoint while its Python stack is still
    // unwinding. Release through the existing interpreter queue after that run.
    requestQueue = requestQueue
      .then(() => entry.releaseCallbackSession?.())
      .catch((error) => postError(null, error));
    // Python may still retain this same wrapper on a reusable Scene. Dropping
    // our registry reference lets wasm-bindgen finalize it after all owners leave.
  }
}

async function prepareAuthoringDependencies(pyodide, source) {
  // Explicit imports use Pyodide's ordinary package resolver. Implicit Manim
  // namespace aliases are detected by Python's AST parser and loaded separately
  // so packages like NumPy stay off the cold authoring startup path.
  await pyodide.loadPackagesFromImports(source);

  const dictConstructor = pyodide.globals.get("dict");
  const globals = dictConstructor();
  dictConstructor.destroy();
  globals.set("__noon_dependency_source", source);
  try {
    const requiredJson = pyodide.runPython(
      `
import _manim_namespace
_manim_namespace.required_packages_json(__noon_dependency_source)
`,
      { globals },
    );
    const requiredPackages = JSON.parse(requiredJson);
    if (requiredPackages.length === 0) return;

    const requiredPackagesJson = JSON.stringify(requiredPackages);
    globals.set("__noon_required_packages_json", requiredPackagesJson);
    const missingJson = pyodide.runPython(
      `
_manim_namespace.missing_packages_json(__noon_required_packages_json)
`,
      { globals },
    );
    const missingPackages = JSON.parse(missingJson);
    if (missingPackages.length > 0) {
      await pyodide.loadPackage(missingPackages);
    }
    pyodide.runPython(
      `
_manim_namespace.bind_loaded_packages_json(__noon_required_packages_json)
`,
      { globals },
    );
  } finally {
    globals.destroy();
  }
}

async function runAuthoringSource(pyodide, source, context, requestId) {
  return withAuthoringDiagnostics(pyodide, source, context, requestId, async (globals) => {
    const run = activeAuthoringRun;
    const sceneOnly = authoringLibrariesPristine && pyodide.runPython(`
from _manim_source_execution import source_is_scene_only
source_is_scene_only(__noon_source)
`, { globals });
    if (typeof sceneOnly !== "boolean") throw new TypeError("source effect proof must be a boolean");
    authoringLibrariesPristine &&= sceneOnly;
    run.sourceSceneOnly = sceneOnly;
    await prepareAuthoringDependencies(pyodide, source);
    return pyodide.runPythonAsync(diagnosedHostCode(`
import json
import _manim_updaters
from _manim_scene import (
    execute_construct,
    await_source_barrier,
    await_module_source_barrier,
    execution_context,
)
from _manim_source_execution import (
    BARRIER_GLOBAL, MODULE_BARRIER_GLOBAL,
    authoring_source_scope, execute_authoring_module,
)
from noon import Scene

__noon_namespace = {
    "context": json.loads(__noon_context_json),
    "__name__": "__main__",
}
if __noon_portable_constructs:
    __noon_namespace[BARRIER_GLOBAL] = await_source_barrier
if __noon_portable_constructs or MODULE_BARRIER_GLOBAL in __noon_code.co_names:
    __noon_namespace[MODULE_BARRIER_GLOBAL] = await_module_source_barrier
with authoring_source_scope():
    await execute_authoring_module(__noon_code, __noon_namespace)

if "result" in __noon_namespace:
    __noon_result = __noon_namespace["result"]
else:
    __noon_scene_classes = [
        value
        for value in __noon_namespace.values()
        if isinstance(value, type)
        and issubclass(value, Scene)
        and value is not Scene
        and getattr(value, "__module__", None) == "__main__"
    ]
    if not __noon_scene_classes:
        raise RuntimeError(
            "Python authoring source must either assign result or define one Scene subclass"
        )
    if len(__noon_scene_classes) != 1:
        __noon_names = ", ".join(cls.__name__ for cls in __noon_scene_classes)
        raise RuntimeError(
            "Python authoring source defines multiple Scene subclasses; "
            f"select one explicitly via result = SceneClass(): {__noon_names}"
        )
    __noon_result = __noon_scene_classes[0]()
    await execute_construct(
        __noon_result,
        portable_constructs=__noon_portable_constructs,
    )

if isinstance(__noon_result, Scene):
    from js import noonRegisterSemanticExecution, noonSemanticContinuationGeneration
    __noon_context = execution_context(__noon_result)
    __noon_live_duration = __noon_context.liveHandoffDuration()
    __noon_semantic = {
        "context_id": str(noonRegisterSemanticExecution(__noon_context)),
        "callback_session_id": _manim_updaters.canonical_callback_session_id(__noon_result),
    }
    __noon_continuation_generation = noonSemanticContinuationGeneration(__noon_context)
    if __noon_continuation_generation is not None:
        __noon_semantic["continuation_generation"] = int(__noon_continuation_generation)
    __noon_duration = float(__noon_live_duration if __noon_live_duration is not None else __noon_context.authoredDuration())

else:
    raise TypeError("Python authoring result must be a noon.Scene")
__noon_result_json = json.dumps(
    {
        "kind": "semantic_scene",
        "semantic_execution": __noon_semantic,
        "duration": __noon_duration,
    },
    separators=(",", ":"),
    allow_nan=False,
)
`), { globals });
  });
}

async function runCanonicalCallbackPhase(pyodide, sessionId, frame, player) {
  return invokeCanonicalCallback(
    pyodide, "_run_canonical_callback_phase_json", sessionId, JSON.stringify(frame), player,
  );
}

async function finishCanonicalCallbackPhase(pyodide, sessionId, frame, committed) {
  // Wrapper finalization needs phase identity, never the callback read rows.
  return invokeCanonicalCallback(
    pyodide, "_finish_canonical_callback_phase_json", sessionId,
    JSON.stringify({ token: frame.token, region: frame.region }), committed,
  );
}

async function releaseCanonicalCallbackSession(pyodide, sessionId) {
  return invokeCanonicalCallback(pyodide, "release_session", sessionId);
}

async function invokeCanonicalCallback(pyodide, name, ...args) {
  const callable = pyodide.pyimport(`_manim_updaters.${name}`);
  try {
    // Awaiting the coroutine uses Pyodide's normal event loop, including JSPI
    // suspension where supported. Pyodide releases the scheduled awaitable;
    // this scope owns only the callable proxy, with no cross-run cache.
    return await callable(...args);
  } finally {
    callable.destroy();
  }
}

function validateRequest(request) {
  if (!isRecord(request) || request.channel !== AUTHORING_CHANNEL) {
    throw new Error("Received a message from an unknown authoring channel");
  }
  if (request.protocolVersion !== AUTHORING_PROTOCOL_VERSION) {
    throw new Error(
      `Unsupported authoring protocol version ${request.protocolVersion}`,
    );
  }
  if (!Number.isSafeInteger(request.requestId) || request.requestId < 0) {
    throw new Error("Python authoring request has an invalid request ID");
  }
  if (request.type === "run") {
    if (typeof request.source !== "string" || request.source.trim() === "") {
      throw new Error("Python authoring source must be a non-empty string");
    }
    if (!isRecord(request.context)) {
      throw new Error("Python authoring context must be an object");
    }
    if (request.preparedPlayback !== undefined && typeof request.preparedPlayback !== "boolean") {
      throw new Error("preparedPlayback must be a boolean");
    }
    if (request.preparationLookaheadSeconds !== undefined &&
        (!Number.isFinite(request.preparationLookaheadSeconds) || request.preparationLookaheadSeconds <= 0)) {
      throw new Error("preparationLookaheadSeconds must be positive and finite");
    }
    return;
  }
  if (request.type === "release_semantic_execution") {
    if (typeof request.contextId !== "string" || !request.contextId) throw new Error("invalid semantic context token");
    return;
  }
  if (request.type === "attach_semantic_execution") {
    if (request.replaceExistingEndpoint !== undefined && typeof request.replaceExistingEndpoint !== "boolean") {
      throw new Error("semantic endpoint replacement must be a boolean");
    }
    if (typeof request.contextId !== "string" || !request.contextId ||
        !(request.controlPort instanceof MessagePort) || !(request.renderPort instanceof MessagePort)) {
      throw new Error("semantic attachment requires a context and two ports");
    }
    if (request.callbackSessionId !== null && request.callbackSessionId !== undefined &&
        (!Number.isSafeInteger(request.callbackSessionId) || request.callbackSessionId < 0)) {
      throw new Error("semantic attachment has an invalid callback session ID");
    }
    if (request.continuationGeneration !== undefined &&
        (!Number.isSafeInteger(request.continuationGeneration) ||
         request.continuationGeneration <= 0)) {
      throw new Error("semantic attachment has an invalid continuation generation");
    }
    if (typeof request.initiallyPaused !== "boolean") {
      throw new Error("semantic attachment has an invalid initially-paused state");
    }
    if (request.pacing !== "realtime" && request.pacing !== "external_samples") {
      throw new Error("semantic attachment has invalid pacing");
    }
    if (request.pacing === "external_samples" && request.continuationGeneration === undefined) {
      throw new Error("external sample pacing requires a source-owned semantic continuation");
    }
    if (request.continuationGeneration !== undefined &&
        (!Number.isSafeInteger(request.continuationRunRequestId) ||
         request.continuationRunRequestId < 0)) {
      throw new Error("semantic attachment has an invalid continuation run request ID");
    }
    return;
  }
  if (request.type === "cancel_semantic_continuation") {
    if (typeof request.contextId !== "string" || !request.contextId ||
        !Number.isSafeInteger(request.continuationGeneration) ||
        request.continuationGeneration <= 0 ||
        !Number.isSafeInteger(request.continuationRunRequestId) ||
        request.continuationRunRequestId < 0 || typeof request.reason !== "string" ||
        request.reason.trim() === "") {
      throw new Error("invalid semantic continuation cancellation");
    }
    return;
  }
  throw new Error(`Unsupported Python authoring request: ${request.type}`);
}

function post(type, payload = {}) {
  if (fatalAuthoringFailure && (type !== "error" || payload.requestId !== null)) return;
  self.postMessage({
    channel: AUTHORING_CHANNEL,
    protocolVersion: AUTHORING_PROTOCOL_VERSION,
    type,
    ...payload,
  });
}

function postError(requestId, error) {
  post("error", {
    requestId,
    message: error instanceof Error ? error.message : String(error),
    diagnostic: error instanceof Error && error.stack ? { stack: error.stack } : null,
    ...authoringDiagnosticPayload(error),
  });
}

function isRecord(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
