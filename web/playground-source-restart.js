// Source replacement is a host lifecycle operation, not an animation scheduler.
// Stop on the first edit; debounce only the next full-source Run. Each delayed
// request checks both edit freshness and selection identity after cancellation.
export function createSourceRestart({
  stop,
  run,
  currentSelection,
  onError,
  delayMs = 500,
  setTimer = setTimeout,
  clearTimer = clearTimeout,
}) {
  let version = 0;
  let timer = null;
  let stopping = null;
  let disposed = false;
  let explicitStart = null;
  let reportedVersion = -1;

  function cancel() {
    version += 1;
    if (timer !== null) clearTimer(timer);
    timer = null;
  }

  function stopCurrent() {
    if (stopping !== null) return stopping;
    // Calling stop synchronously invalidates the current run before an older
    // authoring/reconciliation/metrics promise can commit another result.
    let task;
    try { task = Promise.resolve(stop()); }
    catch (error) { task = Promise.reject(error); }
    stopping = task;
    void task.then(
      () => { if (stopping === task) stopping = null; },
      () => { if (stopping === task) stopping = null; },
    );
    return task;
  }

  function report(error, requestVersion) {
    if (disposed || version !== requestVersion || reportedVersion === requestVersion) return;
    reportedVersion = requestVersion;
    onError(error);
  }

  async function start(requestVersion, selection, retirement, onStarted = () => {}) {
    try {
      await retirement;
      if (disposed || version !== requestVersion || currentSelection() !== selection) return;
      onStarted();
      return await run();
    } catch (error) {
      report(error, requestVersion);
    }
  }

  function edited({ composing = false } = {}) {
    if (disposed) return;
    cancel();
    const requestVersion = version;
    const selection = currentSelection();
    const retirement = stopCurrent();
    // Attach a rejection handler immediately, even while the user is composing.
    void retirement.catch((error) => {
      report(error, requestVersion);
    });
    if (composing) return;
    timer = setTimer(() => {
      timer = null;
      void start(requestVersion, selection, retirement);
    }, delayMs);
  }

  function runNow() {
    if (disposed) return Promise.resolve();
    if (explicitStart?.version === version) return explicitStart.promise;
    cancel();
    const gate = { version, promise: null };
    const release = () => { if (explicitStart === gate) explicitStart = null; };
    gate.promise = start(version, currentSelection(), stopping, release).finally(release);
    explicitStart = gate;
    return gate.promise;
  }

  function dispose() {
    disposed = true;
    cancel();
  }

  return Object.freeze({ edited, runNow, cancel, dispose });
}
