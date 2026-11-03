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

export function validateReadinessContract(contract, { strict = false } = {}) {
  const errors = [];
  const expectedSources = [
    ["showcase-first-scene", "python/examples/showcase_first_scene.py",
      "1a6ba47220c25148f6c24d1fc82733e728690c7548ab1183e8f9905d0396f7e8", 8],
    ["showcase-dynamic-scene", "python/examples/showcase_dynamic_scene.py",
      "bcc689a8cb5d71eefc2afdf69b187260ebf0df5029cc67cca08db43d01541dca", 19],
  ];
  if (contract?.schemaVersion !== 1) errors.push("unsupported readiness contract schema");
  const expectedArms = {
    A: "default-incremental-first-pass",
    B: "default-completed-history-replay",
    C: "explicit-observed-first-pass",
    D: "observed-completed-history-replay",
  };
  if (JSON.stringify(contract?.arms) !== JSON.stringify(expectedArms)) {
    errors.push("readiness contract arm definitions changed");
  }
  const reference = contract?.reference;
  if (reference?.host !== "Apple M6, 24 GB RAM, 12 GPU cores" || reference.os !== "macOS 27.0.1" ||
      reference.browser !== "Chromium 151.0.7922.34" || reference.refreshHz !== 60 ||
      reference.viewport?.width !== 1280 || reference.viewport?.height !== 900 ||
      reference.deviceScaleFactor !== 1 || reference.backend !== "WebGPU" || reference.gpuMode !== "hardware") {
    errors.push("reference hardware target changed");
  }
  if (JSON.stringify(contract?.baselineArms) !== JSON.stringify(["C", "D"]) ||
      JSON.stringify(Object.keys(contract?.strata ?? {}).sort()) !==
        JSON.stringify(["cold-network", "fresh-process-warm-resource-cache", "reload-new-workers-warm-resource-cache"])) {
    errors.push("readiness baseline arms or cache strata changed");
  }
  if (!Array.isArray(contract?.sources) || contract.sources.length !== 2) {
    errors.push("readiness contract must name the first and dense sources");
  } else if (contract.sources.some((source, index) => {
    const [id, path, sha256, duration] = expectedSources[index];
    return source.id !== id || source.path !== path || source.sha256 !== sha256 ||
      source.durationSeconds !== duration || source.window !== "full-authored-first-pass";
  })) {
    errors.push("source IDs, paths, hashes, durations and full-pass windows must match the unchanged catalog");
  }
  const sampling = contract?.sampling;
  if (!(Number.isSafeInteger(sampling?.warmupsPerArmPerStratum) && sampling.warmupsPerArmPerStratum >= 1 &&
        Number.isSafeInteger(sampling?.measuredStartsPerArmPerStratum) && sampling.measuredStartsPerArmPerStratum >= 20 &&
        Number.isSafeInteger(sampling?.minimumCommandObservationsPerSource) && sampling.minimumCommandObservationsPerSource >= 100)) {
    errors.push("readiness sample plan is below the minimum coverage");
  }
  if (JSON.stringify(sampling?.startupPercentiles) !== JSON.stringify(["median", "p90", "p95"]) ||
      JSON.stringify(sampling?.commandPercentiles) !== JSON.stringify(["median", "p95", "p99"]) ||
      JSON.stringify(sampling?.presentationGapPercentiles) !== JSON.stringify(["median", "p95", "p99"]) ||
      sampling?.startupPopulationP99 !== false || sampling?.tailMethod !== "nearest-rank") {
    errors.push("readiness percentile plan is inconsistent with the sample size");
  }
  const anchor = contract?.productGateAnchor;
  if (anchor?.preserveOriginalWorkloadAndThresholds !== true ||
      anchor.sourceRevision !== "9a5781ebf09f3029e402a331528c657419d1bba5" ||
      anchor.runnerSha256 !== "19ceae9aca559ccec9324094bf71a5c9d941e6ecf720369b1641b7f4813e7d2b" ||
      anchor.workloadSha256 !== "5137eef8aa71b8d70df845ad769278d934fd8eca53f585ca09c4f8d5e3ddbe96" ||
      anchor.comparisonSha256 !== "94cf8fd9ecec1d4c7d4f7fe4af123595e7d0b7f788ed422187e650bda64125a0" ||
      JSON.stringify(anchor.thresholds) !== JSON.stringify({ maxLatencyRatio: 1.25, latencySlackMs: 350,
        minFpsRatio: 0.8, maxVisualDiffRatio: 0.015 })) {
    errors.push("Product Gate anchor, workload and thresholds must remain unchanged");
  }
  if (strict) {
    if (contract.status !== "ready") errors.push("readiness contract has not been reviewed and frozen");
    if (!contract.reference?.actualAdapterIdentity) errors.push("actual reference GPU adapter is not pinned");
    for (const [name, value] of Object.entries(contract.bounds ?? {})) {
      if (!Number.isFinite(value) || value <= 0) errors.push(`missing approved bound: ${name}`);
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
  return contract.sources.flatMap(source => arms.flatMap(arm => Object.keys(contract.strata)
    .filter(stratum => stratum !== "cold-network")
    .map(stratum => {
      const rows = cases.filter(row => row.sourceId === source.id && row.arm === arm && row.stratum === stratum && !row.warmup);
      const observed = rows.filter(row => row.status === "observed");
      const commands = {};
      for (const row of observed) for (const command of row.commands ?? []) {
        const values = commands[command.name] ??= { feedback: [], accepted: [], presented: [] };
        const samples = {
          feedback: command.uiFeedbackMs ?? command.feedbackMs,
          accepted: command.runtimeAcceptanceMs ?? command.acceptedMs,
          presented: command.commandCompletionUpperMs ?? command.presentedMs,
        };
        for (const [stage, value] of Object.entries(samples)) if (Number.isFinite(value)) values[stage].push(value);
      }
      const commandSummary = Object.fromEntries(Object.entries(commands).map(([name, values]) => [name,
        Object.fromEntries(Object.entries(values).map(([stage, samples]) => [stage, distribution(samples)]))]));
      const gaps = observed.flatMap(row => row.presentationGapsMs ?? []);
      const buffering = observed.flatMap(row => row.bufferingEpisodes ?? []);
      return {
        sourceId: source.id, arm, stratum,
        expectedStarts: contract.sampling.measuredStartsPerArmPerStratum,
        attemptedStarts: rows.length,
        observedStarts: observed.length,
        failedStarts: rows.filter(row => row.status !== "observed").length,
        startup: Object.fromEntries(["shellReadyMs", "interpreterReadyMs", "firstReaderMs",
          "firstSubmittedMs", "runtimeAcceptanceMs", "firstRendererReportedFrameMs", "firstUsefulMs"].map(key =>
          [key, startupDistribution(observed.map(row => row.startup?.[key]))])),
        commands: commandSummary,
        presentationGapMs: distribution(gaps),
        buffering: { episodes: buffering.length,
          durationMs: distribution(buffering.map(episode => episode.durationMs)) },
        ownerRetirementMs: distribution(observed.map(row => row.retirementMs)),
        processRssScope: "paired source and retained replay in one browser process",
        peakProcessRssBytes: distribution(observed.map(row => row.memory?.peakSampledRssBytes)),
      };
    })));
}

export function validateReadinessResults(cases, contract) {
  const errors = [];
  const expected = contract.sampling.measuredStartsPerArmPerStratum;
  for (const source of contract.sources) for (const arm of Object.keys(contract.arms)) {
    for (const stratum of ["fresh-process-warm-resource-cache", "reload-new-workers-warm-resource-cache"]) {
      const rows = cases.filter(row => row.sourceId === source.id && row.arm === arm &&
        row.stratum === stratum && !row.warmup);
      if (rows.length < expected || rows.some(row => row.status !== "observed")) {
        errors.push(`${source.id}/${arm}/${stratum}: incomplete starts`);
      }
      for (const row of rows) {
        if ((row.sourceInvocationIds ?? []).length !== 1) errors.push(`${source.id}/${arm}: source invocation identity changed`);
        if ((row.profileValidationErrors ?? []).length) errors.push(`${source.id}/${arm}: profile trace invalid`);
        const startupFields = ["firstUsefulMs", "firstSubmittedMs", "runtimeAcceptanceMs"];
        if (row.arm === "A" || row.arm === "C") startupFields.push(
          "shellReadyMs", "interpreterReadyMs", "firstReaderMs", "firstRendererReportedFrameMs");
        for (const key of startupFields) {
          if (!Number.isFinite(row.startup?.[key])) errors.push(`${source.id}/${arm}: missing ${key}`);
        }
        if (!Number.isFinite(row.retirementMs)) errors.push(`${source.id}/${arm}: missing owner retirement observation`);
        if (!Number.isFinite(row.memory?.peakSampledRssBytes)) errors.push(`${source.id}/${arm}: missing process RSS samples`);
        if (!Array.isArray(row.presentationGapsMs) || row.presentationGapsMs.length === 0) {
          errors.push(`${source.id}/${arm}: missing renderer presentation gap samples`);
        }
        for (const command of row.commands ?? []) {
          if (!Number.isFinite(command.uiFeedbackMs)) errors.push(`${source.id}/${arm}: missing UI feedback timing`);
          if (!Number.isFinite(command.commandCompletionUpperMs)) errors.push(`${source.id}/${arm}: missing command completion timing`);
          if (!Number.isFinite(command.runtimeAcceptanceEpochMs)) errors.push(`${source.id}/${arm}: missing command acceptance receipt`);
          if (command.presentedStateMatched !== true && !command.matchingPresentation) {
            errors.push(`${source.id}/${arm}: command has no matching presentation evidence`);
          }
          const acceptanceMs = command.runtimeAcceptanceMs ?? command.acceptedMs;
          if (Number.isFinite(contract.bounds.commandAcceptanceMs) &&
              Number.isFinite(acceptanceMs) && acceptanceMs > contract.bounds.commandAcceptanceMs) {
            errors.push(`${source.id}/${arm}/${stratum}: command acceptance exceeds its bound`);
          }
          const presentationMs = command.commandCompletionUpperMs ?? command.presentedMs;
          if (Number.isFinite(contract.bounds.commandPresentationMs) &&
              Number.isFinite(presentationMs) && presentationMs > contract.bounds.commandPresentationMs) {
            errors.push(`${source.id}/${arm}/${stratum}: command presentation exceeds its bound`);
          }
        }
        if ((row.bufferingEpisodes ?? []).some(episode => !Number.isFinite(episode.durationMs))) {
          errors.push(`${source.id}/${arm}: incomplete buffering episode`);
        }
        if (Number.isFinite(contract.bounds.bufferingEpisodeMs) &&
            (row.bufferingEpisodes ?? []).some(episode => episode.durationMs > contract.bounds.bufferingEpisodeMs)) {
          errors.push(`${source.id}/${arm}/${stratum}: buffering episode exceeds its bound`);
        }
        if (Number.isFinite(contract.bounds.liveOwnerRetirementMs) &&
            (!Number.isFinite(row.retirementMs) || row.retirementMs > contract.bounds.liveOwnerRetirementMs)) {
          errors.push(`${source.id}/${arm}/${stratum}: owner retirement exceeds its bound`);
        }
      }
      const useful = distribution(rows.filter(row => row.status === "observed")
        .map(row => row.startup?.firstUsefulMs));
      if (Number.isFinite(contract.bounds.firstUsefulMs) && useful.p95 > contract.bounds.firstUsefulMs) {
        errors.push(`${source.id}/${arm}/${stratum}: first useful p95 exceeds its bound`);
      }
      const gaps = distribution(rows.flatMap(row => row.presentationGapsMs ?? []));
      if (Number.isFinite(contract.bounds.frameGapMs) && gaps.p99 > contract.bounds.frameGapMs) {
        errors.push(`${source.id}/${arm}/${stratum}: presentation gap p99 exceeds its bound`);
      }
      const memory = distribution(rows.map(row => row.memory?.peakSampledRssBytes));
      if (Number.isFinite(contract.bounds.peakProcessRssBytes) && memory.p95 > contract.bounds.peakProcessRssBytes) {
        errors.push(`${source.id}/${arm}/${stratum}: process RSS p95 exceeds its bound`);
      }
    }
    for (const stratum of ["fresh-process-warm-resource-cache", "reload-new-workers-warm-resource-cache"]) {
      const commands = cases.filter(row => row.sourceId === source.id && row.stratum === stratum && !row.warmup)
        .flatMap(row => row.commands ?? []);
      if (commands.length < contract.sampling.minimumCommandObservationsPerSource) {
        errors.push(`${source.id}/${stratum}: too few command observations`);
      }
      const feedback = distribution(commands.map(command => command.uiFeedbackMs));
      const completion = distribution(commands.map(command => command.commandCompletionUpperMs));
      if (Number.isFinite(contract.bounds.commandFeedbackMs) && feedback.p95 > contract.bounds.commandFeedbackMs) {
        errors.push(`${source.id}/${stratum}: command feedback p95 exceeds its bound`);
      }
      if (Number.isFinite(contract.bounds.commandPresentationMs) && completion.p99 > contract.bounds.commandPresentationMs) {
        errors.push(`${source.id}/${stratum}: command presentation p99 exceeds its bound`);
      }
    }
  }
  return [...new Set(errors)];
}
