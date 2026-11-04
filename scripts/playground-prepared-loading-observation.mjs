import { summarizeCadence } from "../web/frame-metrics.js";

export const READINESS_ARMS = ["A", "B", "C", "D"];
export const READINESS_BASELINE_ARMS = ["C", "D"];
const READINESS_STARTUP_PHASES = ["sourceRun", "historyReplay"];
export const READINESS_STRATA = ["fresh-process-warm-resource-cache",
  "reload-new-workers-warm-resource-cache", "cold-network", "reused-healthy-interpreter"];
export const READINESS_BOUND_KEYS = ["firstUsefulMs", "commandFeedbackMs", "commandAcceptanceP95Ms",
  "commandPresentationP99Ms", "bufferingEpisodeCount", "bufferingTotalMs", "bufferingEpisodeMs",
  "activeFrameGapP99Ms", "missedRefreshFraction",
  "maxActiveFrameGapMs", "peakProcessRssBytes", "liveOwnerRetirementMs"];
const READINESS_FIRST_USEFUL_PAIRS = [["A", "C"], ["B", "D"]];
export const PRODUCT_GATE_THRESHOLD_ENVS = {
  maxLatencyRatio: "NOON_PRODUCT_MAX_LATENCY_RATIO",
  latencySlackMs: "NOON_PRODUCT_LATENCY_SLACK_MS",
  minFpsRatio: "NOON_PRODUCT_MIN_FPS_RATIO",
  maxVisualDiffRatio: "NOON_PRODUCT_MAX_VISUAL_DIFF_RATIO",
};

export function readinessInstrumentationPlan(mode) {
  if (mode === "minimal-timing") return { observationPollIntervalMs: 100,
    usefulCaptureIntervalMs: 250, profilePublicationStages: false };
  if (mode === "bounded-diagnostics") return { observationPollIntervalMs: 25,
    usefulCaptureIntervalMs: 0, profilePublicationStages: true };
  return null;
}

export function summarizeColdNetworkObservations(requests, pyodideBaseUrl) {
  const external = requests.filter(request => request.external === true);
  const pyodide = external.filter(request => request.url.startsWith(pyodideBaseUrl));
  return {
    externalRequestCount: external.length,
    pyodideRequestCount: pyodide.length,
    pyodideResponseStatuses: pyodide.map(request => request.status),
    serviceWorkerResponses: external.filter(request => request.fromServiceWorker === true).length,
    cacheHeaders: external.map(({ url, age, cacheControl, cdnCacheStatus, via }) =>
      ({ url, age, cacheControl, cdnCacheStatus, via })),
    routeFailureCount: requests.filter(request => request.failure !== undefined).length,
  };
}

export function validateReusedHealthyInterpreter(seed, measured, sameWorker) {
  const errors = [];
  const before = seed?.executionDiagnostics, after = measured?.executionDiagnostics;
  if (sameWorker !== true) errors.push("healthy-interpreter run replaced its Python worker");
  if (before?.libraryTrust !== "pristine" || after?.libraryTrust !== "pristine") {
    errors.push("healthy-interpreter run did not retain pristine library trust");
  }
  if (!Number.isSafeInteger(before?.interpreterGeneration) ||
      after?.interpreterGeneration !== before.interpreterGeneration) {
    errors.push("healthy-interpreter run changed interpreter generation");
  }
  if (!Number.isSafeInteger(before?.runRequestId) || !Number.isSafeInteger(after?.runRequestId) ||
      after.runRequestId === before.runRequestId) {
    errors.push("healthy-interpreter run did not issue a distinct source request");
  }
  return errors;
}

export function readinessPresentationMatchesRun(attachedPublication, latestPresentation, originMs, runStartEpochMs) {
  return Number.isSafeInteger(attachedPublication?.session) && Number.isSafeInteger(attachedPublication?.sequence) &&
    latestPresentation?.session === attachedPublication.session &&
    Number.isSafeInteger(latestPresentation?.sequence) && latestPresentation.sequence >= attachedPublication.sequence &&
    Number.isFinite(originMs) && Number.isFinite(latestPresentation?.presentedAtMs) &&
    Number.isFinite(runStartEpochMs) && originMs + latestPresentation.presentedAtMs >= runStartEpochMs;
}

export function readinessTraceMatchesRun(trace, run, sourceId) {
  return Number.isSafeInteger(trace?.runRequestId) && trace.runRequestId >= 0 &&
    trace.context?.exampleId === sourceId && Number.isSafeInteger(run?.runGeneration) &&
    trace.context?.runGeneration === run.runGeneration;
}

export function readinessRunTimingOriginErrors(startup) {
  const errors = [];
  const accepted = startup?.runAcceptedAtEpochMs;
  const started = startup?.sourceRunStartedAtEpochMs;
  const completed = startup?.sourceRunCompletedAtEpochMs;
  if (!Number.isFinite(accepted)) errors.push("missing accepted Run timestamp");
  if (!Number.isFinite(started)) errors.push("missing source start timestamp");
  if (!Number.isFinite(completed)) errors.push("missing source completion timestamp");
  if (Number.isFinite(accepted) && Number.isFinite(started) && started < accepted) {
    errors.push("source start predates accepted Run");
  }
  if (Number.isFinite(started) && Number.isFinite(completed) && completed < started) {
    errors.push("source completion predates source start");
  }
  if (Number.isFinite(accepted) && Number.isFinite(completed) && completed < accepted) {
    errors.push("source completion predates accepted Run");
  }
  if (Number.isFinite(accepted) && Number.isFinite(completed) &&
      startup?.sourceCompleteMs !== completed - accepted) {
    errors.push("source completion duration is not measured from accepted Run");
  }
  return errors;
}

export function readinessReplayCycleEvidence(observations, { restartPublication, restartLoopCount = null, requestEpochMs,
  durationSeconds, terminalHoldStartSeconds, clockToleranceSeconds, acceptsTerminalPublication }) {
  if (!Array.isArray(observations) || !Number.isSafeInteger(restartPublication?.session) ||
      !Number.isSafeInteger(restartPublication?.sequence) || !Number.isFinite(requestEpochMs) ||
      !Number.isFinite(durationSeconds) || !Number.isFinite(terminalHoldStartSeconds) ||
      !Number.isFinite(clockToleranceSeconds) || typeof acceptsTerminalPublication !== "function") return null;
  let reset = null, terminalHold = null, nearEnd = null, wrapped = null;
  let advancedAfterReset = false;
  for (const observation of observations) {
    const controls = observation.controls;
    const clock = Number(controls?.elapsedSeconds);
    const frame = observation.metrics?.latestPresentationSample;
    const origin = observation.metrics?.performanceTimeOriginMs;
    const prepared = observation.preparedPlayback;
    const held = prepared?.observation?.heldPresentation;
    const observedTime = prepared?.observedTime;
    const heldPublicationMatches = held?.publication?.session === frame?.session &&
      held?.publication?.sequence === frame?.sequence;
    const heldTerminalStill = heldPublicationMatches && Number.isSafeInteger(restartLoopCount) &&
      prepared.observation.loopCount === restartLoopCount &&
      Number.isFinite(observedTime) && observedTime === held.time &&
      Number.isFinite(held.evaluatedTime) && observedTime >= held.evaluatedTime &&
      Number.isFinite(held.validUntil) && observedTime < held.validUntil &&
      observedTime >= terminalHoldStartSeconds && observedTime < durationSeconds;
    const presentedAfterRequest = Number.isFinite(origin) && Number.isFinite(frame?.presentedAtMs) &&
      origin + frame.presentedAtMs >= requestEpochMs;
    if (!Number.isFinite(observation.epochMs) || observation.epochMs < requestEpochMs ||
        !Number.isFinite(clock) || controls?.busy !== "false" ||
        frame?.session !== restartPublication.session || !Number.isSafeInteger(frame?.sequence) ||
        frame.sequence < restartPublication.sequence || !Number.isFinite(frame.time) ||
        !presentedAfterRequest) continue;
    const sample = { observationEpochMs: observation.epochMs, controlTime: clock,
      publication: { session: frame.session, sequence: frame.sequence, time: frame.time,
        presentedAtEpochMs: origin + frame.presentedAtMs,
        heldPresentation: heldTerminalStill ? held : null,
        observedTime: heldTerminalStill ? observedTime : null } };
    if (reset === null) {
      if (controls.playing === "false" && frame.sequence === restartPublication.sequence &&
          (restartLoopCount === null || prepared?.observation?.loopCount === restartLoopCount) &&
          clock <= clockToleranceSeconds && frame.time <= clockToleranceSeconds) reset = sample;
      continue;
    }
    const loopCount = prepared?.observation?.loopCount;
    if (restartLoopCount !== null && (!Number.isSafeInteger(loopCount) || loopCount < restartLoopCount ||
        loopCount > restartLoopCount + 1)) return null;
    if (controls.playing !== "true") continue;
    if (clock > clockToleranceSeconds) advancedAfterReset = true;
    if (advancedAfterReset && nearEnd === null && clock <= clockToleranceSeconds &&
        frame.time <= clockToleranceSeconds) return null;
    if (terminalHold === null && clock >= terminalHoldStartSeconds - clockToleranceSeconds &&
        frame.sequence > restartPublication.sequence &&
        (restartLoopCount === null || loopCount === restartLoopCount) &&
        (acceptsTerminalPublication(frame.time) || heldTerminalStill)) {
      terminalHold = sample;
    }
    if (terminalHold !== null && nearEnd === null && clock >= durationSeconds - clockToleranceSeconds &&
        (restartLoopCount === null || loopCount === restartLoopCount)) {
      nearEnd = sample;
    }
    if (nearEnd !== null && observation.epochMs > nearEnd.observationEpochMs &&
        clock <= clockToleranceSeconds && frame.time <= clockToleranceSeconds &&
        (restartLoopCount === null || loopCount === restartLoopCount + 1) &&
        frame.sequence > terminalHold.publication.sequence) {
      wrapped = sample;
      break;
    }
  }
  return reset && terminalHold && nearEnd && wrapped ? { reset, terminalHold, nearEnd, wrapped } : null;
}

export function readinessRendererSamples(stagedSamples, observations) {
  const samples = new Map();
  for (const sample of stagedSamples ?? []) {
    if (!Number.isSafeInteger(sample?.session) || !Number.isSafeInteger(sample?.sequence) ||
        !Number.isFinite(sample?.presentedAtMs) || !Number.isFinite(sample?.clockOriginMs)) continue;
    samples.set(`${sample.clockOriginMs}:${sample.session}:${sample.sequence}`, sample);
  }
  for (const observation of observations ?? []) {
    const metrics = observation.metrics ?? {};
    const sample = metrics.latestPresentationSample;
    if (!Number.isSafeInteger(sample?.session) || !Number.isSafeInteger(sample?.sequence) ||
        !Number.isFinite(sample?.presentedAtMs) || !Number.isFinite(metrics.performanceTimeOriginMs)) continue;
    const key = `${metrics.performanceTimeOriginMs}:${sample.session}:${sample.sequence}`;
    if (!samples.has(key)) samples.set(key, { ...sample, clockOriginMs: metrics.performanceTimeOriginMs });
  }
  return [...samples.values()].sort((left, right) =>
    left.clockOriginMs + left.presentedAtMs - right.clockOriginMs - right.presentedAtMs);
}

export function readinessWarmSeekTargets(entry, actualEnd = entry?.duration) {
  const duration = entry?.duration;
  if (!Number.isFinite(duration) || !Number.isFinite(actualEnd) || actualEnd <= 0) return [];
  const terminalHold = entry?.still_intervals?.findLast(([, end]) => end === duration);
  const candidates = [...(entry?.beats ?? []).map(beat => beat.time), terminalHold?.[0], actualEnd]
    .filter(Number.isFinite).filter(time => time > 0 && time <= actualEnd).sort((a, b) => a - b);
  const unique = [...new Set(candidates)];
  if (unique.length < 5) return [];
  const targets = Array.from({ length: 5 }, (_, index) => unique[Math.round(index * (unique.length - 1) / 4)]);
  return new Set(targets).size === targets.length ? targets : [];
}

export function readinessCommandReceipt(result, requestEpochMs, rendererSample = null) {
  const timing = result?.commandTiming;
  const acceptedEpochMs = Number.isFinite(timing?.acceptedEpochMs) ? timing.acceptedEpochMs : null;
  const enqueuedEpochMs = Number.isFinite(timing?.enqueuedEpochMs) ? timing.enqueuedEpochMs : null;
  const dequeuedEpochMs = Number.isFinite(timing?.dequeuedEpochMs) ? timing.dequeuedEpochMs : null;
  const publication = result?.commandPublication;
  const commandPublication = Number.isSafeInteger(publication?.session) && publication.session >= 0 &&
    Number.isSafeInteger(publication?.sequence) && publication.sequence >= 0
    ? { session: publication.session, sequence: publication.sequence } : null;
  const sampleMatches = rendererSample !== null && commandPublication !== null &&
    rendererSample.session === commandPublication.session && rendererSample.sequence === commandPublication.sequence &&
    Number.isFinite(rendererSample.time) && Number.isFinite(rendererSample.presentedAtMs) &&
    Number.isFinite(rendererSample.clockOriginMs);
  const receipt = result?.presentationReceipt;
  const matchingPresentation = Number.isSafeInteger(receipt?.session) && receipt.session >= 0 &&
    Number.isSafeInteger(receipt?.sequence) && receipt.sequence >= 0 && Number.isFinite(receipt?.time) &&
    typeof receipt?.unchanged === "boolean"
    ? { session: receipt.session, sequence: receipt.sequence, time: receipt.time, unchanged: receipt.unchanged }
    : sampleMatches ? { session: rendererSample.session, sequence: rendererSample.sequence,
      time: rendererSample.time, source: "renderer-publication-stage" } : null;
  const rendererPresentationEpochMs = sampleMatches
    ? rendererSample.clockOriginMs + rendererSample.presentedAtMs : null;
  let presentationSettledEpochMs = null;
  let presentationSettlementSource = null;
  if (Number.isFinite(timing?.presentationSettledEpochMs)) {
    presentationSettledEpochMs = matchingPresentation?.unchanged === true && Number.isFinite(acceptedEpochMs)
      ? Math.max(acceptedEpochMs, timing.presentationSettledEpochMs) : timing.presentationSettledEpochMs;
    presentationSettlementSource = matchingPresentation?.unchanged === true
      ? "endpoint no-change proof joined at command acceptance" : "endpoint presentation-settled timestamp";
  } else if (rendererPresentationEpochMs !== null) {
    presentationSettledEpochMs = rendererPresentationEpochMs;
    presentationSettlementSource = "matching renderer publication sample";
  } else if (matchingPresentation !== null && Number.isFinite(result?.receivedEpochMs)) {
    presentationSettledEpochMs = result.receivedEpochMs;
    presentationSettlementSource = "endpoint receipt arrival upper bound";
  }
  // These are endpoint wall intervals: request-to-enqueue includes port/event-loop delivery;
  // dequeue-to-acceptance includes execution and awaits, not CPU time.
  return {
    commandPublication,
    commandQueueTiming: {
      enqueuedEpochMs,
      dequeuedEpochMs,
      requestToEnqueueMs: Number.isFinite(requestEpochMs) && Number.isFinite(enqueuedEpochMs)
        ? enqueuedEpochMs - requestEpochMs : null,
      queueWaitMs: Number.isFinite(enqueuedEpochMs) && Number.isFinite(dequeuedEpochMs)
        ? dequeuedEpochMs - enqueuedEpochMs : null,
      dequeueToAcceptanceMs: Number.isFinite(dequeuedEpochMs) && Number.isFinite(acceptedEpochMs)
        ? acceptedEpochMs - dequeuedEpochMs : null,
    },
    runtimeAcceptanceEpochMs: acceptedEpochMs,
    runtimeAcceptanceMs: Number.isFinite(acceptedEpochMs) && Number.isFinite(requestEpochMs)
      ? acceptedEpochMs - requestEpochMs : null,
    presentationSettledEpochMs,
    presentationSettlementSource,
    commandPresentationMs: Number.isFinite(presentationSettledEpochMs) && Number.isFinite(requestEpochMs)
      ? presentationSettledEpochMs - requestEpochMs : null,
    matchingPresentation,
  };
}

export function readinessPreparedReaderLead(preparedPlayback) {
  const observation = preparedPlayback?.observation;
  const observedTime = preparedPlayback?.observedTime;
  const frontier = observation?.frontier;
  if (!Number.isFinite(observedTime) || !Number.isFinite(frontier)) return null;
  return {
    observedTime,
    frontier,
    leadSeconds: frontier - observedTime,
    endClosed: Number.isFinite(observation.end),
    buffering: observation.buffering === true,
  };
}

// A measurement oracle for the selected nonuniform gallery scenes, not a renderer
// or a general definition of visual usefulness. Ignore CSS decoration and alpha-zero
// bytes; require scene ink distinguishable from the dominant canvas background.
export function sceneInkPixels({ data, width, height }, margin = 8) {
  const colors = new Map();
  for (let y = margin; y < height - margin; y++) for (let x = margin; x < width - margin; x++) {
    const i = (y * width + x) * 4;
    if (data[i + 3] === 0) continue;
    const color = (data[i] << 16) | (data[i + 1] << 8) | data[i + 2];
    colors.set(color, (colors.get(color) ?? 0) + 1);
  }
  let background = 0, frequency = 0;
  for (const [color, count] of colors) if (count > frequency) { background = color; frequency = count; }
  const rgb = [background >> 16, (background >> 8) & 255, background & 255];
  let ink = 0;
  for (const [color, count] of colors) {
    if (Math.max(Math.abs((color >> 16) - rgb[0]), Math.abs(((color >> 8) & 255) - rgb[1]),
      Math.abs((color & 255) - rgb[2])) >= 8) ink += count;
  }
  return ink;
}

export function distribution(values) {
  const sorted = values.filter(Number.isFinite).sort((a, b) => a - b);
  const percentile = fraction => sorted.length ? sorted[Math.max(0, Math.ceil(sorted.length * fraction) - 1)] : null;
  return { n: sorted.length, min: sorted[0] ?? null, median: sorted.length ? (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.floor(sorted.length / 2)]) / 2 : null, p90: percentile(0.9),
    p95: percentile(0.95), p99: percentile(0.99), max: sorted.at(-1) ?? null,
    percentileMethod: "midpoint median; nearest-rank tails" };
}

export function summarizeReaderLead(observations) {
  const values = Array.isArray(observations) ? observations : [];
  return { count: values.length,
    leadSeconds: distribution(values.map(value => value.leadSeconds)),
    openSamples: values.filter(value => !value.endClosed).length,
    closedSamples: values.filter(value => value.endClosed).length,
    bufferingSamples: values.filter(value => value.buffering).length };
}

export function readinessPostTerminalReceiverSession(trace, measuredSession) {
  if (!Number.isSafeInteger(measuredSession)) return null;
  if (trace?.droppedEvents !== 0 || !Array.isArray(trace.events) || trace.events.some((event, index) =>
    !Number.isSafeInteger(event.sequence) || event.sequence !== index)) return null;
  const completed = trace.events.filter(event => event.stage === "disposition" &&
    event.disposition === "observed-complete");
  if (completed.length !== 1) return null;
  const terminal = completed[0];
  if (!Number.isSafeInteger(terminal.sequence) || terminal.session !== measuredSession ||
      terminal.publication?.session !== terminal.session || typeof terminal.contextId !== "string" ||
      !Number.isSafeInteger(terminal.continuationGeneration)) return null;
  const receivers = trace.events.filter(event => event.stage === "receiver-attached" &&
    event.sequence > terminal.sequence && event.contextId === terminal.contextId &&
    event.continuationGeneration === terminal.continuationGeneration &&
    Number.isSafeInteger(event.session) && event.publication?.session === event.session &&
    event.publication.snapshot === true);
  return receivers.length === 1 && receivers[0].session !== terminal.session ? receivers[0].session : null;
}

export function activeFrameGapEvidence(capture, { startEpochMs, endEpochMs, session,
  rendererWorkerOrdinal, targetHz, postTerminalReceiverSession = null }) {
  const errors = capture?.error ? [capture.error] : [];
  const samples = Array.isArray(capture?.samples) ? capture.samples : [];
  if (!Number.isFinite(startEpochMs) || !Number.isFinite(endEpochMs) || endEpochMs <= startEpochMs ||
      !Number.isSafeInteger(session) || !Number.isSafeInteger(rendererWorkerOrdinal)) {
    errors.push("active frame-gap window identity or bounds are missing");
  }
  if (!Number.isFinite(capture?.headerObservedAtEpochMs) ||
      !Number.isSafeInteger(capture?.capacity) || capture.capacity <= 0) {
    errors.push("raw cadence capture header is missing or malformed");
  }
  const ownerMetricsEpochMs = capture?.workers?.find(worker => worker.ordinal === rendererWorkerOrdinal)?.latestMetricsEpochMs;
  const ownerState = capture?.workers?.find(worker => worker.ordinal === rendererWorkerOrdinal);
  if (!Number.isFinite(ownerMetricsEpochMs) || ownerMetricsEpochMs < endEpochMs) {
    errors.push("active frame-gap capture did not sample through the window end");
  }
  const ownerOrdinals = samples.filter(sample => sample.workerOrdinal === rendererWorkerOrdinal)
    .map(sample => sample.ordinal);
  if (!Number.isSafeInteger(ownerState?.latestOrdinal) || ownerOrdinals.length !== ownerState.latestOrdinal ||
      ownerOrdinals.some((ordinal, index) => ordinal !== index + 1)) {
    errors.push("raw cadence interval ordinals contain a gap or missing tail");
  }
  const inWindow = samples.filter(sample => {
    const end = sample.clockOriginMs + sample.presentedAtMs;
    const start = end - sample.intervalMs;
    return sample.workerOrdinal === rendererWorkerOrdinal && sample.session === session &&
      start < endEpochMs && end > startEpochMs;
  });
  const selected = inWindow.filter(sample => {
    const end = sample.clockOriginMs + sample.presentedAtMs;
    const start = end - sample.intervalMs;
    return start >= startEpochMs && end <= endEpochMs;
  });
  if (selected.length === 0) errors.push("active frame-gap window has no complete renderer intervals");
  const ownerSamples = samples.filter(sample => sample.workerOrdinal === rendererWorkerOrdinal &&
    sample.session === session);
  const targetGenerations = [...new Set(ownerSamples.map(sample => sample.generation))];
  const generations = [...new Set(ownerSamples.filter(sample => {
    const at = sample.clockOriginMs + sample.presentedAtMs;
    return at >= startEpochMs && at <= endEpochMs;
  }).map(sample => sample.generation))];
  if (targetGenerations.some(generation => !Number.isSafeInteger(generation))) {
    errors.push("measured renderer generation is missing or malformed");
  }
  if (targetGenerations.length > 1) errors.push("renderer generation changed within the measured session");
  const ownerGenerationChanges = (capture?.generationChanges ?? []).filter(change =>
    change.workerOrdinal === rendererWorkerOrdinal);
  const headerObservedAtEpochMs = capture?.headerObservedAtEpochMs;
  const malformedGenerationTimes = ownerGenerationChanges.some(change =>
    !Number.isFinite(change.changedAtEpochMs) || !Number.isFinite(change.observedAtEpochMs) ||
    !Number.isFinite(headerObservedAtEpochMs) || change.observedAtEpochMs < headerObservedAtEpochMs ||
    change.observedAtEpochMs > ownerMetricsEpochMs || change.changedAtEpochMs > change.observedAtEpochMs);
  if (malformedGenerationTimes) errors.push("raw cadence generation timestamp is missing or invalid");
  const generationChanges = ownerGenerationChanges.filter(change =>
    change.changedAtEpochMs >= startEpochMs && change.changedAtEpochMs <= endEpochMs);
  const targetSessionChanges = generationChanges.filter(change => change.session === session);
  const validTargetSessionEstablishment = targetSessionChanges.length === 1 && (() => {
    const change = targetSessionChanges[0];
    return Number.isSafeInteger(change.from) && Number.isSafeInteger(change.to) &&
      change.to - change.from === 1 && Number.isSafeInteger(change.previousSession) &&
      change.previousSession !== session && targetGenerations.length === 1 && targetGenerations[0] === change.to;
  })();
  const unattributedGenerationChange = generationChanges.some(change => {
    const delta = change.to - change.from;
    if (change.session === session) return !validTargetSessionEstablishment;
    if (Number.isSafeInteger(postTerminalReceiverSession) &&
        change.session === postTerminalReceiverSession && postTerminalReceiverSession !== session) {
      return !Number.isSafeInteger(change.from) || !Number.isSafeInteger(change.to) || delta !== 1 ||
        change.previousSession !== session || targetGenerations.length !== 1 ||
        targetGenerations[0] !== change.from;
    }
    return true;
  });
  if (generations.length > 1 || unattributedGenerationChange) {
    errors.push("renderer generation changed during the active frame-gap window");
  }
  const intervalMs = distribution(selected.map(sample => sample.intervalMs));
  const cadence = summarizeCadence(selected.map(sample => sample.intervalMs), targetHz);
  return { window: { startEpochMs, endEpochMs, session, rendererWorkerOrdinal },
    latestMetricsEpochMs: ownerMetricsEpochMs, count: selected.length,
    intervalMs, targetHz, missedVsyncs: cadence?.missedVsyncs ?? null,
    missedRefreshFraction: cadence === null ? null : cadence.missedVsyncs /
      (selected.length + cadence.missedVsyncs), longFrameRate: cadence?.longFrameRate ?? null,
    workerOrdinals: [...new Set(inWindow.map(sample => sample.workerOrdinal))], generations,
    coverageErrors: errors, samples: selected };
}

export function validateReadinessContract(contract, { strict = false } = {}) {
  const errors = [];
  if (contract?.schemaVersion !== 1) errors.push("unsupported readiness contract schema");
  if (JSON.stringify(Object.keys(contract?.arms ?? {}).sort()) !== JSON.stringify(READINESS_ARMS) ||
      Object.values(contract?.arms ?? {}).some(value => typeof value !== "string" || value.length === 0)) {
    errors.push("readiness contract must define arms A through D");
  }
  const reference = contract?.reference;
  if (!["host", "os", "browser", "backend", "gpuMode"].every(key => typeof reference?.[key] === "string" &&
      reference[key].length > 0) || !Number.isFinite(reference?.refreshHz) ||
      !Number.isSafeInteger(reference?.viewport?.width) || !Number.isSafeInteger(reference?.viewport?.height) ||
      !Number.isFinite(reference?.deviceScaleFactor) || reference.refreshHz <= 0 ||
      reference.viewport.width <= 0 || reference.viewport.height <= 0 || reference.deviceScaleFactor <= 0) {
    errors.push("readiness hardware reference is incomplete");
  }
  if (JSON.stringify(contract?.baselineArms) !== JSON.stringify(READINESS_BASELINE_ARMS) ||
      READINESS_STRATA.some(name => typeof contract?.strata?.[name] !== "string" || contract.strata[name].length === 0) ||
      typeof contract?.strata?.["cold-network"] !== "string") {
    errors.push("readiness baseline arms or cache strata are incomplete");
  }
  const allowedRegression = contract?.allowedRegression;
  const firstUsefulRegression = allowedRegression?.firstUsefulP95;
  if (JSON.stringify(Object.keys(allowedRegression ?? {}).sort()) !== JSON.stringify(["firstUsefulP95"]) ||
      JSON.stringify(Object.keys(firstUsefulRegression ?? {}).sort()) !== JSON.stringify(["maxRatio", "slackMs"]) ||
      [firstUsefulRegression?.maxRatio, firstUsefulRegression?.slackMs].some(value => value !== null &&
        (!Number.isFinite(value) || value < 0)) ||
      (firstUsefulRegression?.maxRatio !== null && firstUsefulRegression?.maxRatio < 1)) {
    errors.push("first-useful allowed regression is malformed");
  }
  if (!Array.isArray(contract?.pendingCohorts) || contract.pendingCohorts.some(cohort =>
      typeof cohort?.id !== "string" || typeof cohort?.protocol !== "string" || cohort.protocol.length === 0) ||
      new Set(contract.pendingCohorts.map(cohort => cohort.id)).size !== contract.pendingCohorts.length) {
    errors.push("pending readiness cohorts are malformed");
  } else if (strict && contract.pendingCohorts.length > 0) {
    errors.push(`readiness cohorts remain pending: ${contract.pendingCohorts.map(cohort => cohort.id).join(", ")}`);
  }
  if (!["firstUsefulMs", "firstSubmittedMs", "commandAcceptance", "commandPresentation"].every(key =>
      typeof contract?.measurementDefinitions?.[key] === "string" &&
      contract.measurementDefinitions[key].length > 0) ||
      !contract.measurementDefinitions.firstSubmittedMs.includes("upper bound")) {
    errors.push("readiness measurement definitions are incomplete");
  }
  if (!Array.isArray(contract?.sources) || contract.sources.length !== 2 ||
      new Set(contract.sources.map(source => source?.id)).size !== 2) {
    errors.push("readiness contract must name the first and dense sources");
  } else if (contract.sources.some(source => typeof source.id !== "string" || typeof source.path !== "string" ||
      !/^[0-9a-f]{64}$/.test(source.sha256 ?? "") || !Number.isFinite(source.durationSeconds) ||
      source.durationSeconds <= 0 || source.window !== "full-authored-first-pass")) {
    errors.push("source identity or full-pass duration is malformed");
  }
  const firstUsefulBounds = contract?.bounds?.firstUsefulMs;
  const sourceIds = Array.isArray(contract?.sources) ? contract.sources.map(source => source?.id) : [];
  if (JSON.stringify(Object.keys(firstUsefulBounds ?? {}).sort()) !== JSON.stringify(sourceIds.slice().sort())) {
    errors.push("first-useful bounds must cover each source");
  } else for (const sourceId of sourceIds) {
    const strata = firstUsefulBounds[sourceId];
    if (JSON.stringify(Object.keys(strata ?? {}).sort()) !== JSON.stringify([...READINESS_STRATA].sort())) {
      errors.push(`first-useful bounds must cover each stratum for ${sourceId}`);
      continue;
    }
    for (const stratum of READINESS_STRATA) {
      const phases = strata[stratum];
      if (JSON.stringify(Object.keys(phases ?? {}).sort()) !== JSON.stringify([...READINESS_STARTUP_PHASES].sort()) ||
          Object.values(phases ?? {}).some(value => value !== null &&
            (!Number.isFinite(value) || value <= 0))) {
        errors.push(`first-useful bounds are malformed for ${sourceId}/${stratum}`);
      }
    }
  }
  const sampling = contract?.sampling;
  if (!(Number.isSafeInteger(sampling?.warmupsPerArmPerStratum) && sampling.warmupsPerArmPerStratum >= 1 &&
        Number.isSafeInteger(sampling?.measuredStartsPerArmPerStratum) && sampling.measuredStartsPerArmPerStratum >= 20 &&
        Number.isSafeInteger(sampling?.minimumCommandObservationsPerSourceArm) && sampling.minimumCommandObservationsPerSourceArm >= 100)) {
    errors.push("readiness sample plan is below the minimum coverage");
  }
  if (JSON.stringify(sampling?.startupPercentiles) !== JSON.stringify(["median", "p90", "p95"]) ||
      JSON.stringify(sampling?.commandPercentiles) !== JSON.stringify(["median", "p95", "p99"]) ||
      JSON.stringify(sampling?.activeFrameGapPercentiles) !== JSON.stringify(["median", "p95", "p99"]) ||
      sampling?.startupPopulationP99 !== false || sampling?.tailMethod !== "nearest-rank") {
    errors.push("readiness percentile plan is inconsistent with the sample size");
  }
  const anchor = contract?.productGateAnchor;
  if (!/^[0-9a-f]{40}$/.test(anchor?.sourceRevision ?? "") ||
      !/^[0-9a-f]{64}$/.test(anchor?.manifestSha256 ?? "") ||
      !/^[0-9a-f]{64}$/.test(anchor?.dependencyLockSha256 ?? "") ||
      anchor?.preserveOriginalWorkloadAndThresholds !== true ||
      !Array.isArray(anchor?.workloads) || !anchor.workloads.every(value => typeof value === "string") ||
      JSON.stringify(Object.keys(anchor?.thresholds ?? {}).sort()) !== JSON.stringify(Object.keys(PRODUCT_GATE_THRESHOLD_ENVS).sort()) ||
      Object.values(anchor?.thresholds ?? {}).some(value => !Number.isFinite(value))) {
    errors.push("Product Gate anchor metadata is malformed");
  }
  for (const name of ["bufferingEpisodeCount", "bufferingTotalMs", "bufferingEpisodeMs"]) {
    const value = contract?.bounds?.[name];
    if (value !== null && (!Number.isFinite(value) || value < 0 ||
        (name === "bufferingEpisodeCount" && !Number.isSafeInteger(value)))) {
      errors.push(`buffering bound is malformed: ${name}`);
    }
  }
  if (strict) {
    if (contract.status !== "ready") errors.push("readiness contract has not been reviewed and frozen");
    if (!contract.reference?.actualAdapterIdentity) errors.push("actual reference GPU adapter is not pinned");
    if (!Number.isFinite(firstUsefulRegression?.maxRatio) || firstUsefulRegression.maxRatio < 1 ||
        !Number.isFinite(firstUsefulRegression?.slackMs) || firstUsefulRegression.slackMs < 0) {
      errors.push("missing approved allowed regression: firstUsefulP95");
    }
    for (const name of READINESS_BOUND_KEYS) {
      if (name === "firstUsefulMs") {
        for (const sourceId of sourceIds) for (const stratum of READINESS_STRATA) {
          for (const phase of READINESS_STARTUP_PHASES) {
            const value = firstUsefulBounds?.[sourceId]?.[stratum]?.[phase];
            if (!Number.isFinite(value) || value <= 0) {
              errors.push(`missing approved bound: firstUsefulMs/${sourceId}/${stratum}/${phase}`);
            }
          }
        }
        continue;
      }
      const value = contract.bounds?.[name];
      const zeroAllowed = name === "bufferingEpisodeCount" || name === "bufferingTotalMs";
      if (!Number.isFinite(value) || (name === "missedRefreshFraction" ? value < 0 || value > 1 :
          zeroAllowed ? value < 0 : value <= 0)) {
        errors.push(`missing approved bound: ${name}`);
      }
    }
  }
  return errors;
}

export function validateReadinessTrace(trace, { sourceId, requestedProfile, attachedProfile, disposition }) {
  const errors = [];
  if (trace?.schemaVersion !== 1 || !Number.isSafeInteger(trace.interpreterGeneration) ||
      trace.interpreterGeneration < 0 || trace.runRequestId === null || trace.runRequestId === undefined ||
      trace.context?.exampleId !== sourceId || !Array.isArray(trace.events)) {
    return ["missing or invalid execution trace identity"];
  }
  if (trace.droppedEvents !== 0) errors.push("execution trace dropped events");
  if (trace.requestedProfile !== requestedProfile) errors.push("unexpected requested profile");
  const events = trace.events;
  if (events.some((event, index) => !Number.isSafeInteger(event.sequence) || event.sequence < 0 ||
      (index > 0 && event.sequence !== events[index - 1].sequence + 1))) {
    errors.push("execution trace sequence is invalid or incomplete");
  }
  const attached = events.filter(event => event.stage === "attached");
  if (attached.length !== 1) errors.push("trace must contain one initial actual attachment");
  if (attached[0]?.profile !== attachedProfile) errors.push("unexpected actual attached profile");
  for (const event of events) {
    if ((event.stage === "attached" || event.stage === "profile-change") && event.profile !== attachedProfile) {
      errors.push("unexpected actual profile transition");
      break;
    }
  }
  const sourceComplete = events.filter(event => event.stage === "source-complete");
  const terminalAdopted = events.filter(event => event.stage === "terminal-adopted");
  const finalDisposition = events.filter(event => event.stage === "disposition" && event.disposition === disposition);
  if (sourceComplete.length === 0) errors.push("trace has no source completion");
  if (terminalAdopted.length === 0) errors.push("trace has no terminal adoption");
  if (finalDisposition.length === 0) errors.push("trace has no expected disposition");
  const requiredBeforeDisposition = [...sourceComplete, ...terminalAdopted, ...attached];
  if (finalDisposition.length > 0 && requiredBeforeDisposition.some(event =>
      event.sequence >= finalDisposition.at(-1).sequence)) {
    errors.push("disposition precedes source cleanup or terminal adoption");
  }
  return [...new Set(errors)];
}

export function summarizeReadiness(cases, contract) {
  const arms = Object.keys(contract.arms);
  const startupDistribution = values => {
    const summary = distribution(values);
    delete summary.p99;
    return summary;
  };
  return contract.sources.flatMap(source => arms.flatMap(arm => READINESS_STRATA
    .map(stratum => {
      const rows = cases.filter(row => row.sourceId === source.id && row.arm === arm && row.stratum === stratum && !row.warmup);
      const observed = rows.filter(row => row.status === "observed");
      const commands = {};
      const commandPhases = {};
      for (const row of observed) for (const command of row.commands ?? []) {
        const values = commands[command.name] ??= {
          feedback: [], accepted: [], presented: [], requestToEnqueue: [], queueWait: [], dequeueToAcceptance: [],
        };
        if (command.commandPhase !== undefined) commandPhases[command.name] = command.commandPhase;
        const samples = {
          feedback: command.uiFeedbackMs ?? command.feedbackMs,
          accepted: command.runtimeAcceptanceMs ?? command.acceptedMs,
          presented: command.commandPresentationMs ?? command.presentedMs,
          requestToEnqueue: command.commandQueueTiming?.requestToEnqueueMs,
          queueWait: command.commandQueueTiming?.queueWaitMs,
          dequeueToAcceptance: command.commandQueueTiming?.dequeueToAcceptanceMs,
        };
        for (const [stage, value] of Object.entries(samples)) if (Number.isFinite(value)) values[stage].push(value);
      }
      const commandSummary = Object.fromEntries(Object.entries(commands).map(([name, values]) => [name,
        { ...Object.fromEntries(Object.entries(values).map(([stage, samples]) => [stage, distribution(samples)])),
          ...(commandPhases[name] ? { phase: commandPhases[name] } : {}) }]));
      const frameGaps = observed.flatMap(row => row.activeFrameGap?.samples ?? []);
      const frameGapDistribution = distribution(frameGaps.map(sample => sample.intervalMs));
      const frameGapCount = frameGaps.length;
      const missedVsyncs = observed.reduce((sum, row) => sum + (row.activeFrameGap?.missedVsyncs ?? 0), 0);
      const longFrames = observed.reduce((sum, row) => sum +
        (row.activeFrameGap?.longFrameRate ?? 0) * (row.activeFrameGap?.count ?? 0), 0);
      const readerLead = observed.flatMap(row => row.readerLeadObservations ?? []);
      const perRunMaxFrameGaps = observed.map(row => row.activeFrameGap?.intervalMs?.max)
        .filter(Number.isFinite);
      const buffering = observed.map(row => row.bufferingEvidence ?? { count: null, totalMs: null, maxMs: null });
      const firstUsefulPhase = arm === "B" || arm === "D" ? "historyReplay" : "sourceRun";
      return {
        sourceId: source.id, arm, stratum,
        expectedStarts: contract.sampling.measuredStartsPerArmPerStratum,
        attemptedStarts: rows.length,
        observedStarts: observed.length,
        failedStarts: rows.filter(row => row.status !== "observed").length,
        startup: Object.fromEntries(["navigationShellReadyObservedUpperMs", "shellReadyObservedUpperMs",
          "interpreterReadyObservedUpperMs", "firstControlsReadyObservedUpperMs", "sourceCompleteMs",
          "firstReaderAttachedObservedUpperMs", "firstSubmittedMs", "firstUsefulMs"].map(key =>
          [key, startupDistribution(observed.map(row => row.startup?.[key]))])),
        startupBound: { firstUsefulMs: contract.bounds.firstUsefulMs?.[source.id]?.[stratum]?.[firstUsefulPhase] ?? null,
          phase: firstUsefulPhase },
        commands: commandSummary,
        activeFrameGapDistribution: frameGapDistribution,
        activeFrameGapP99Ms: frameGapDistribution.p99,
        activeFrameGapSampleCount: frameGapCount,
        missedRefreshFraction: frameGapCount + missedVsyncs > 0 ? missedVsyncs / (frameGapCount + missedVsyncs) : null,
        longFrameRate: frameGapCount > 0 ? longFrames / frameGapCount : null,
        maxActiveFrameGapMs: frameGaps.reduce((maximum, sample) => Math.max(maximum, sample.intervalMs), 0) || null,
        perRunMaxActiveFrameGapMs: distribution(perRunMaxFrameGaps),
        readerLead: { sampleCount: readerLead.length,
          seconds: distribution(readerLead.map(sample => sample.leadSeconds)),
          openSamples: readerLead.filter(sample => !sample.endClosed).length,
          closedSamples: readerLead.filter(sample => sample.endClosed).length,
          bufferingSamples: readerLead.filter(sample => sample.buffering).length },
        buffering: { perRunEpisodes: distribution(buffering.map(value => value.count)),
          perRunTotalMs: distribution(buffering.map(value => value.totalMs)),
          perRunMaxEpisodeMs: distribution(buffering.map(value => value.maxMs)),
          incompleteRuns: buffering.filter(value => !Number.isFinite(value.count) ||
            !Number.isFinite(value.totalMs) || !Number.isFinite(value.maxMs)).length },
        ownerRetirementMs: distribution(observed.map(row => row.retirementMs)),
        processRssScope: "paired source and retained replay in one browser process",
        peakProcessRssBytes: distribution(observed.map(row => row.memory?.peakSampledRssBytes)),
      };
    })));
}

export function readinessBufferingEvidence(trace, { startEpochMs, endEpochMs, required, session }) {
  if (!required) return { capture: "not-applicable", episodes: [], count: 0, totalMs: 0, maxMs: 0, errors: [] };
  const errors = [];
  if (trace?.droppedEvents !== 0 || !Array.isArray(trace.events)) {
    return { capture: "incomplete", episodes: [], count: null, totalMs: null, maxMs: null,
      errors: ["buffering trace is missing or dropped events"] };
  }
  const events = trace?.events ?? [];
  if (events.some((event, index) => event.sequence !== index)) {
    return { capture: "incomplete", episodes: [], count: null, totalMs: null, maxMs: null,
      errors: ["buffering trace sequence is incomplete"] };
  }
  if (!Number.isSafeInteger(session)) return { capture: "missing-session", episodes: [], count: null,
    totalMs: null, maxMs: null, errors: ["buffering measurement session is missing"] };
  const markers = events.filter(event => ["attached", "receiver-attached"].includes(event.stage) &&
    event.bufferingCapture !== undefined);
  if (!Number.isFinite(startEpochMs) || !Number.isFinite(endEpochMs) || endEpochMs < startEpochMs) {
    return { capture: "invalid-window", episodes: [], count: null, totalMs: null, maxMs: null,
      errors: ["buffering trace window is missing or reversed"] };
  }
  if (markers.length === 0) return { capture: "missing", episodes: [], count: null, totalMs: null,
    maxMs: null, errors: ["prepared reader has no attached buffering capture"] };
  const matchingMarkers = markers.filter(event => event.session === session &&
    event.bufferingCapture?.session === session && event.publication?.session === session &&
    event.bufferingCapture.observedAtEpochMs <= endEpochMs);
  if (matchingMarkers.length !== 1) return { capture: "invalid", episodes: [], count: null, totalMs: null,
    maxMs: null, errors: ["prepared reader has no unique attached capture for the measured session"] };
  const markerEvent = matchingMarkers[0];
  const capture = markerEvent.bufferingCapture;
  if (typeof capture.playing !== "boolean" || typeof capture.buffering !== "boolean" ||
      typeof capture.waiting !== "boolean" || capture.waiting !== (capture.playing && capture.buffering) ||
      !Number.isFinite(capture.observedAtEpochMs)) {
    return { capture: "invalid", episodes: [], count: null, totalMs: null, maxMs: null,
      errors: ["attached buffering capture is malformed"] };
  }
  const episodes = [];
  let waiting = capture.waiting;
  let opened = waiting ? { at: capture.observedAtEpochMs, leftCensored: true,
    boundary: "receiver-attached", reason: null } : null;
  let previousObservedAt = capture.observedAtEpochMs;
  let previousSequence = markerEvent.sequence;
  const transitions = events.filter(item => item.stage === "buffering-transition" && item.session === session &&
    item.sequence > markerEvent.sequence);
  for (const transition of transitions) {
    const entered = transition.waiting === true;
    if (!Number.isSafeInteger(transition.sequence) || transition.sequence <= previousSequence ||
        typeof transition.playing !== "boolean" || typeof transition.buffering !== "boolean" ||
        typeof transition.waiting !== "boolean" || transition.waiting !== (transition.playing && transition.buffering) ||
        typeof transition.boundary !== "string" || transition.boundary.length === 0 ||
        typeof transition.reason !== "string" || transition.reason.length === 0 ||
        !Number.isFinite(transition.mutationStartedAtEpochMs) || !Number.isFinite(transition.observedAtEpochMs) ||
        transition.observedAtEpochMs < transition.mutationStartedAtEpochMs ||
        transition.mutationStartedAtEpochMs < previousObservedAt || transition.observedAtEpochMs < previousObservedAt) {
      errors.push(`buffering transition is malformed or out of order in session ${session}`);
      continue;
    }
    previousObservedAt = transition.observedAtEpochMs;
    previousSequence = transition.sequence;
    if (transition.observedAtEpochMs > endEpochMs) continue;
    if (transition.leftCensored === true) {
      if (waiting && entered && opened?.leftCensored === true) continue;
      errors.push(`buffering transition is unexpectedly left-censored in session ${session}`);
      continue;
    }
    if (entered === waiting) {
      errors.push(`buffering transition did not change waiting state in session ${session}`);
      continue;
    }
    if (entered) opened = { at: transition.mutationStartedAtEpochMs, leftCensored: false,
      boundary: transition.boundary, reason: transition.reason };
    else {
      if (opened === null) {
        errors.push(`buffering wait closed without an observed start in session ${session}`);
        waiting = false;
        continue;
      }
      const start = Math.max(opened.at, startEpochMs);
      const end = Math.min(transition.observedAtEpochMs, endEpochMs);
      if (opened.leftCensored && end >= start && transition.observedAtEpochMs >= startEpochMs) {
        errors.push(`buffering wait is left-censored in measured session ${session}`);
      }
      if (end >= start && transition.observedAtEpochMs >= startEpochMs) episodes.push({ session, startedAtEpochMs: start,
        endedAtEpochMs: end, durationMs: end - start, startBoundary: opened.boundary,
        startReason: opened.reason, endBoundary: transition.boundary, endReason: transition.reason,
        leftCensored: opened.leftCensored });
      opened = null;
    }
    waiting = entered;
  }
  if (waiting && opened !== null && opened.at <= endEpochMs) {
    const leftCensored = opened.leftCensored;
    if (opened.leftCensored && endEpochMs >= startEpochMs) {
      errors.push(`buffering wait is left-censored in measured session ${session}`);
    }
    episodes.push({ session, startedAtEpochMs: Math.max(opened.at, startEpochMs), endedAtEpochMs: null,
      durationMs: null, leftCensored, open: true });
    errors.push(`buffering wait remained open at the end of session ${session} window`);
  }
  if (errors.length) return { capture: "invalid", episodes, count: null, totalMs: null, maxMs: null, errors };
  const totalMs = episodes.reduce((sum, episode) => sum + episode.durationMs, 0);
  return { capture: "captured", episodes, count: episodes.length, totalMs,
    maxMs: episodes.reduce((maximum, episode) => Math.max(maximum, episode.durationMs), 0), errors };
}

export function readinessFirstUsefulRegressionErrors(cases, contract) {
  const { maxRatio, slackMs } = contract?.allowedRegression?.firstUsefulP95 ?? {};
  if (!Number.isFinite(maxRatio) || maxRatio < 1 || !Number.isFinite(slackMs) || slackMs < 0) return [];
  const errors = [];
  for (const source of contract.sources) for (const stratum of READINESS_STRATA) {
    for (const [candidateArm, referenceArm] of READINESS_FIRST_USEFUL_PAIRS) {
      const p95 = arm => distribution(cases.filter(row => row.sourceId === source.id && row.arm === arm &&
        row.stratum === stratum && !row.warmup && row.status === "observed")
        .map(row => row.startup?.firstUsefulMs)).p95;
      const candidateP95 = p95(candidateArm), referenceP95 = p95(referenceArm);
      if (!Number.isFinite(candidateP95) || !Number.isFinite(referenceP95)) continue;
      const allowedP95 = referenceP95 * maxRatio + slackMs;
      if (candidateP95 > allowedP95) {
        errors.push(`${source.id}/${candidateArm}/${stratum}: first useful p95 ${candidateP95}ms exceeds ` +
          `${referenceArm} p95 ${referenceP95}ms × ${maxRatio} + ${slackMs}ms (${allowedP95}ms)`);
      }
    }
  }
  return errors;
}

export function validateReadinessResults(cases, contract) {
  const errors = [];
  errors.push(...readinessFirstUsefulRegressionErrors(cases, contract));
  const expected = contract.sampling.measuredStartsPerArmPerStratum;
  for (const source of contract.sources) for (const arm of Object.keys(contract.arms)) {
    for (const stratum of READINESS_STRATA) {
      const rows = cases.filter(row => row.sourceId === source.id && row.arm === arm &&
        row.stratum === stratum && !row.warmup);
      if (rows.length < expected || rows.some(row => row.status !== "observed")) {
        errors.push(`${source.id}/${arm}/${stratum}: incomplete starts`);
      }
      for (const row of rows) {
        if ((row.sourceInvocationIds ?? []).length !== 1) errors.push(`${source.id}/${arm}: source invocation identity changed`);
        if ((row.profileValidationErrors ?? []).length) errors.push(`${source.id}/${arm}: profile trace invalid`);
        const startupFields = ["firstUsefulMs", "firstSubmittedMs"];
        if (row.arm === "A" || row.arm === "C") startupFields.push(
          "navigationShellReadyObservedUpperMs", "shellReadyObservedUpperMs",
          "interpreterReadyObservedUpperMs", "firstControlsReadyObservedUpperMs",
          "runAcceptedAtEpochMs", "sourceRunStartedAtEpochMs", "sourceRunCompletedAtEpochMs",
          "sourceCompleteMs");
        if (row.arm === "A" || row.arm === "C") {
          errors.push(...readinessRunTimingOriginErrors(row.startup)
            .map(error => `${source.id}/${arm}: ${error}`));
        }
        if (row.arm === "A") startupFields.push("firstReaderAttachedObservedUpperMs");
        for (const key of startupFields) {
          if (!Number.isFinite(row.startup?.[key])) errors.push(`${source.id}/${arm}: missing ${key}`);
        }
        if (!Number.isFinite(row.retirementMs)) errors.push(`${source.id}/${arm}: missing owner retirement observation`);
        if (!Number.isFinite(row.memory?.peakSampledRssBytes)) errors.push(`${source.id}/${arm}: missing process RSS samples`);
        const frameGap = row.activeFrameGap;
        if (!frameGap || !Array.isArray(frameGap.samples) || frameGap.samples.length === 0 ||
            frameGap.count !== frameGap.samples.length || frameGap.intervalMs?.n !== frameGap.count ||
            ![frameGap.intervalMs?.median, frameGap.intervalMs?.p95, frameGap.intervalMs?.p99,
              frameGap.intervalMs?.max, frameGap.missedRefreshFraction, frameGap.longFrameRate].every(Number.isFinite)) {
          errors.push(`${source.id}/${arm}: missing or malformed full active frame-gap samples`);
        }
        if ((frameGap?.coverageErrors ?? []).length > 0) {
          errors.push(`${source.id}/${arm}: incomplete active frame-gap coverage`);
        }
        const rawCoverage = row.rawCadenceCoverage;
        if (!Number.isSafeInteger(row.rendererWorkerOrdinal) ||
            frameGap?.window?.rendererWorkerOrdinal !== row.rendererWorkerOrdinal ||
            !Number.isSafeInteger(rawCoverage?.capacity) || rawCoverage.capacity <= 0 ||
            !Number.isFinite(rawCoverage?.headerObservedAtEpochMs) ||
            !Number.isFinite(rawCoverage?.latestMetricsEpochMs) ||
            rawCoverage.latestMetricsEpochMs < frameGap?.window?.endEpochMs) {
          errors.push(`${source.id}/${arm}: missing owner-bound raw cadence header or fresh final sample`);
        }
        if (row.arm === "A" || row.arm === "B") {
          const leads = row.readerLeadObservations;
          if (!Array.isArray(leads) || leads.length === 0 || leads.length > 4096 ||
              leads.some(sample => !Number.isFinite(sample.leadSeconds) || !Number.isFinite(sample.observedTime) ||
                !Number.isFinite(sample.frontier) || typeof sample.endClosed !== "boolean" ||
                typeof sample.buffering !== "boolean")) {
            errors.push(`${source.id}/${arm}: missing or malformed prepared reader-lead samples`);
          }
        }
        for (const command of row.commands ?? []) {
          if (command.name === "seek" && command.commandPhase === "completed-history-warm") {
            const roundoff = 8 * Number.EPSILON * Math.max(1, row.durationSeconds);
            if (!Number.isFinite(row.actualClosedEnd) || !Number.isFinite(row.durationSeconds) ||
                Math.abs(row.actualClosedEnd - row.durationSeconds) > roundoff) {
              errors.push(`${source.id}/${arm}: actual closed endpoint differs from the storyboard duration`);
            }
            if (!Number.isFinite(command.targetTime) || command.targetTime < 0 ||
                !Number.isFinite(row.actualClosedEnd) || command.targetTime > row.actualClosedEnd) {
              errors.push(`${source.id}/${arm}: completed-history seek target is outside the actual closed range`);
            }
            if (command.matchingPresentation?.time !== command.targetTime || command.receiptTime !== command.targetTime) {
              errors.push(`${source.id}/${arm}: completed-history seek receipt does not match its exact target`);
            }
          }
          if (!Number.isFinite(command.uiFeedbackMs)) errors.push(`${source.id}/${arm}: missing UI feedback timing`);
          if (!Number.isFinite(command.commandCompletionUpperMs)) errors.push(`${source.id}/${arm}: missing command completion timing`);
          if (!Number.isFinite(command.runtimeAcceptanceEpochMs)) errors.push(`${source.id}/${arm}: missing command acceptance receipt`);
          if (Number.isFinite(command.requestEpochMs) && Number.isFinite(command.runtimeAcceptanceEpochMs) &&
              command.runtimeAcceptanceEpochMs < command.requestEpochMs) {
            errors.push(`${source.id}/${arm}: command acceptance predates its DOM request`);
          }
          if (!Number.isFinite(command.presentationSettledEpochMs) ||
              command.presentationSettledEpochMs < command.runtimeAcceptanceEpochMs) {
            errors.push(`${source.id}/${arm}: missing ordered presentation-settled receipt`);
          }
          if (!Number.isFinite(command.commandPresentationMs)) errors.push(`${source.id}/${arm}: missing command presentation timing`);
          const queueTiming = command.commandQueueTiming;
          if (![queueTiming?.enqueuedEpochMs, queueTiming?.dequeuedEpochMs,
            queueTiming?.requestToEnqueueMs, queueTiming?.queueWaitMs,
            queueTiming?.dequeueToAcceptanceMs].every(Number.isFinite)) {
            errors.push(`${source.id}/${arm}: missing command queue timing`);
          } else if (!Number.isFinite(command.requestEpochMs) || !Number.isFinite(command.runtimeAcceptanceEpochMs) ||
              queueTiming.enqueuedEpochMs < command.requestEpochMs ||
              queueTiming.dequeuedEpochMs < queueTiming.enqueuedEpochMs ||
              command.runtimeAcceptanceEpochMs < queueTiming.dequeuedEpochMs ||
              queueTiming.requestToEnqueueMs < 0 || queueTiming.queueWaitMs < 0 ||
              queueTiming.dequeueToAcceptanceMs < 0) {
            errors.push(`${source.id}/${arm}: command queue timing is out of order`);
          }
          if (!command.matchingPresentation) {
            errors.push(`${source.id}/${arm}: command has no command-correlated renderer receipt`);
          } else {
            const receipt = command.matchingPresentation;
            if (!Number.isSafeInteger(receipt.session) || receipt.session < 0 ||
                !Number.isSafeInteger(receipt.sequence) || receipt.sequence < 0 || !Number.isFinite(receipt.time)) {
              errors.push(`${source.id}/${arm}: malformed command presentation receipt`);
            } else if (command.commandPublication &&
                (receipt.session !== command.commandPublication.session ||
                 receipt.sequence !== command.commandPublication.sequence)) {
              errors.push(`${source.id}/${arm}: renderer presentation does not match the command publication`);
            }
          }
        }
        const buffering = row.bufferingEvidence;
        if (!buffering || !Array.isArray(buffering.episodes) ||
            !Number.isSafeInteger(buffering.count) || !Number.isFinite(buffering.totalMs) ||
            !Number.isFinite(buffering.maxMs) || buffering.capture !== (arm === "A" || arm === "B"
              ? "captured" : "not-applicable")) {
          errors.push(`${source.id}/${arm}: missing or malformed Rust buffering capture`);
        } else {
          if (buffering.episodes.some(episode => !Number.isFinite(episode.durationMs) || episode.durationMs < 0 ||
              episode.open === true || episode.leftCensored === true)) {
            errors.push(`${source.id}/${arm}: incomplete buffering episode`);
          }
          if (buffering.count !== buffering.episodes.length ||
              buffering.totalMs !== buffering.episodes.reduce((sum, episode) => sum + episode.durationMs, 0) ||
              buffering.maxMs !== buffering.episodes.reduce((maximum, episode) => Math.max(maximum, episode.durationMs), 0)) {
            errors.push(`${source.id}/${arm}: buffering episode totals are inconsistent`);
          }
          if (Number.isFinite(contract.bounds.bufferingEpisodeCount) &&
              buffering.count > contract.bounds.bufferingEpisodeCount) {
            errors.push(`${source.id}/${arm}/${stratum}: buffering episode count exceeds its bound`);
          }
          if (Number.isFinite(contract.bounds.bufferingTotalMs) &&
              buffering.totalMs > contract.bounds.bufferingTotalMs) {
            errors.push(`${source.id}/${arm}/${stratum}: total buffering time exceeds its bound`);
          }
          if (Number.isFinite(contract.bounds.bufferingEpisodeMs) && buffering.maxMs > contract.bounds.bufferingEpisodeMs) {
            errors.push(`${source.id}/${arm}/${stratum}: buffering episode exceeds its bound`);
          }
        }
        if (Number.isFinite(contract.bounds.liveOwnerRetirementMs) &&
            (!Number.isFinite(row.retirementMs) || row.retirementMs > contract.bounds.liveOwnerRetirementMs)) {
          errors.push(`${source.id}/${arm}/${stratum}: owner retirement exceeds its bound`);
        }
      }
      const firstUsefulPhase = arm === "B" || arm === "D" ? "historyReplay" : "sourceRun";
      const usefulBound = contract.bounds.firstUsefulMs?.[source.id]?.[stratum]?.[firstUsefulPhase];
      const useful = distribution(rows.filter(row => row.status === "observed")
        .map(row => row.startup?.firstUsefulMs));
      if (Number.isFinite(usefulBound) && useful.p95 > usefulBound) {
        errors.push(`${source.id}/${arm}/${stratum}: first useful p95 ${useful.p95}ms exceeds ${firstUsefulPhase} bound ${usefulBound}ms`);
      }
      const gapSamples = rows.flatMap(row => row.activeFrameGap?.samples ?? []);
      const gapSummary = distribution(gapSamples.map(sample => sample.intervalMs));
      if (Number.isFinite(contract.bounds.activeFrameGapP99Ms) &&
          gapSummary.p99 > contract.bounds.activeFrameGapP99Ms) {
        errors.push(`${source.id}/${arm}/${stratum}: active frame-gap p99 exceeds its bound`);
      }
      const gapCount = gapSamples.length;
      const missedVsyncs = rows.reduce((sum, row) => sum + (row.activeFrameGap?.missedVsyncs ?? 0), 0);
      const missedRefreshFraction = gapCount + missedVsyncs > 0 ? missedVsyncs / (gapCount + missedVsyncs) : null;
      if (Number.isFinite(contract.bounds.missedRefreshFraction) &&
          (!Number.isFinite(missedRefreshFraction) || missedRefreshFraction > contract.bounds.missedRefreshFraction)) {
        errors.push(`${source.id}/${arm}/${stratum}: missed refresh fraction exceeds its bound`);
      }
      const maxActiveFrameGapMs = gapSamples.reduce((maximum, sample) => Math.max(maximum, sample.intervalMs), 0);
      if (Number.isFinite(contract.bounds.maxActiveFrameGapMs) &&
          (!Number.isFinite(maxActiveFrameGapMs) || maxActiveFrameGapMs > contract.bounds.maxActiveFrameGapMs)) {
        errors.push(`${source.id}/${arm}/${stratum}: maximum active frame gap exceeds its bound`);
      }
      const memory = distribution(rows.map(row => row.memory?.peakSampledRssBytes));
      if (Number.isFinite(contract.bounds.peakProcessRssBytes) && memory.p95 > contract.bounds.peakProcessRssBytes) {
        errors.push(`${source.id}/${arm}/${stratum}: process RSS p95 exceeds its bound`);
      }
    }
    for (const arm of Object.keys(contract.arms)) {
      const commands = cases.filter(row => row.sourceId === source.id && row.arm === arm && !row.warmup)
        .flatMap(row => row.commands ?? []);
      const byStratum = READINESS_STRATA.map(stratum => cases.filter(row => row.sourceId === source.id &&
        row.arm === arm && row.stratum === stratum && !row.warmup).flatMap(row => row.commands ?? []));
      if (byStratum.some(samples => samples.length === 0) ||
          commands.length < contract.sampling.minimumCommandObservationsPerSourceArm) {
        errors.push(`${source.id}/${arm}: too few command observations or a missing stratum`);
      }
      if (arm === "B" || arm === "D") for (const stratum of READINESS_STRATA) {
        const seeks = cases.filter(row => row.sourceId === source.id && row.arm === arm &&
          row.stratum === stratum && !row.warmup).flatMap(row => row.commands ?? [])
          .filter(command => command.name === "seek" && command.commandPhase === "completed-history-warm");
        if (seeks.length < contract.sampling.minimumCommandObservationsPerSourceArm) {
          errors.push(`${source.id}/${arm}/${stratum}: too few completed-history seek observations`);
        }
        const seekAcceptance = distribution(seeks.map(command => command.runtimeAcceptanceMs));
        if (Number.isFinite(contract.bounds.commandAcceptanceP95Ms) &&
            seekAcceptance.p95 > contract.bounds.commandAcceptanceP95Ms) {
          errors.push(`${source.id}/${arm}/${stratum}: completed-history seek acceptance p95 exceeds its bound`);
        }
        const seekPresentation = distribution(seeks.map(command => command.commandPresentationMs));
        if (Number.isFinite(contract.bounds.commandPresentationP99Ms) &&
            seekPresentation.p99 > contract.bounds.commandPresentationP99Ms) {
          errors.push(`${source.id}/${arm}/${stratum}: completed-history seek presentation p99 exceeds its bound`);
        }
      }
      const feedback = distribution(commands.map(command => command.uiFeedbackMs));
      if (Number.isFinite(contract.bounds.commandFeedbackMs) && feedback.p95 > contract.bounds.commandFeedbackMs) {
        errors.push(`${source.id}/${arm}: command feedback p95 exceeds its bound`);
      }
      const acceptance = distribution(commands.map(command => command.runtimeAcceptanceMs));
      if (Number.isFinite(contract.bounds.commandAcceptanceP95Ms) && acceptance.p95 > contract.bounds.commandAcceptanceP95Ms) {
        errors.push(`${source.id}/${arm}: command acceptance p95 exceeds its bound`);
      }
      const presentation = distribution(commands.map(command => command.commandPresentationMs));
      if (Number.isFinite(contract.bounds.commandPresentationP99Ms) && presentation.p99 > contract.bounds.commandPresentationP99Ms) {
        errors.push(`${source.id}/${arm}: command presentation p99 exceeds its bound`);
      }
    }
  }
  return [...new Set(errors)];
}
