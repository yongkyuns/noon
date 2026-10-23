// Observe committed editor input after target listeners, in the same browser
// task. A later Playwright round trip may already see the automatic rebuild.
export function installStressObservation(target = globalThis) {
  const document = target.document;
  let pendingEdit = null;
  function readState() {
    const status = document.querySelector("#status");
    const patch = document.querySelector("#patch-status");
    const pane = document.querySelector(".editor-pane");
    const scroller = document.querySelector("#scene-editor-panel .cm-scroller");
    return {
      runtimeState: status?.dataset.state ?? "",
      runtimeStartup: status?.dataset.runtimeStartup ?? "",
      liveAuthoring: status?.dataset.liveAuthoring ?? "",
      rendererBackend: status?.dataset.rendererBackend ?? "",
      executionMode: status?.dataset.executionMode ?? "",
      presentedFrames: Number(status?.dataset.presentedFrames ?? "0"),
      patchState: patch?.dataset.state ?? "",
      patchText: patch?.value ?? patch?.textContent ?? "",
      patchOperation: patch?.dataset.operation ?? "",
      patchSequence: patch?.dataset.sequence ?? "",
      patchRunGeneration: patch?.dataset.runGeneration ?? "",
      exampleId: patch?.dataset.exampleId ?? "",
      objectCount: document.querySelector("#metric-objects")?.value ?? "",
      runInFlight: target.__noonExampleGallery?.runInFlight ?? false,
      runGeneration: target.__noonExampleGallery?.generationDiagnostics.runGeneration ?? null,
      runDisabled: document.querySelector("#replace-scene")?.disabled ?? true,
      editorHeight: pane?.getBoundingClientRect().height ?? 0,
      bodyHeight: document.body.scrollHeight,
      editorScrollHeight: scroller?.scrollHeight ?? 0,
      editorClientHeight: scroller?.clientHeight ?? 0,
      enhanced: document.querySelector("#scene-editor-panel .python-code-editor[data-editor-ready='true']") !== null,
      textareaHidden: document.querySelector("#python-scene-source")?.hidden ?? false,
    };
  }
  target.__noonStressObservation = Object.freeze({
    snapshot({ reset = false } = {}) {
      if (reset) document.querySelector(".reset-example").click();
      return readState();
    },
    expectSourceEdit(source) { pendingEdit = { source, snapshot: null }; },
    editedSnapshot() {
      if (!pendingEdit?.snapshot) throw new Error("No matching committed source input was observed");
      return pendingEdit.snapshot;
    },
  });
  document.addEventListener("input", (event) => {
    if (pendingEdit && pendingEdit.snapshot === null && event.target?.id === "python-scene-source"
        && event.target.value === pendingEdit.source) {
      pendingEdit.snapshot = readState();
    }
  });
}
