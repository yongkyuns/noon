// Browser qualification helpers, not a player or source-execution implementation.
// Each exported function is self-contained for Playwright page.evaluate().
export async function startCallbackProof({ source, canvasId, pacing, sampleTime = null }) {
  const harness = window.sharedAuthoringSmoke;
  if (harness.callbackProof) throw new Error("previous callback proof is still active");
  if (!["external_samples", "realtime"].includes(pacing) ||
      (pacing === "external_samples" && (!Number.isFinite(sampleTime) || sampleTime <= 0)) ||
      (pacing === "realtime" && sampleTime !== null)) {
    throw new TypeError("invalid callback proof pacing/sample");
  }
  const canvas = document.createElement("canvas");
  canvas.id = canvasId;
  canvas.width = 640;
  canvas.height = 360;
  document.body.append(canvas);
  let resolveAttached;
  const attached = new Promise(resolve => { resolveAttached = resolve; });
  let rejectFailure;
  const failed = new Promise((_, reject) => { rejectFailure = reject; });
  // Keep the failure observed between separate screenshot/evaluate calls. Every
  // subsequent proof operation still rejects it; this is not a recovery path.
  void failed.catch(() => {});
  const proof = {
    canvas, pacing, execution: null, registration: null, settled: false,
    closed: false, failure: null, phase: "source registration",
    fail(error) {
      if (this.failure !== null) return;
      this.failure = error instanceof Error ? error : new Error(String(error));
      rejectFailure(this.failure);
    },
    async wait(operation, phase) {
      this.phase = phase;
      let timer;
      try {
        return await Promise.race([
          operation, failed,
          new Promise((_, reject) => {
            timer = setTimeout(() => reject(new Error(`${canvasId}: timed out during ${phase}`)), 30_000);
          }),
        ]);
      } finally {
        clearTimeout(timer);
      }
    },
    dispose() {
      if (this.closed) return;
      this.closed = true;
      try { this.execution?.terminate(); }
      finally {
        canvas.remove();
        if (harness.callbackProof === this) harness.callbackProof = null;
      }
    },
  };
  harness.callbackProof = proof;
  try {
    proof.authored = Promise.resolve().then(() => harness.authoring.run(source, {}, {
      async onSemanticContinuation(registration) {
        if (proof.closed) throw new Error(`${canvasId}: late registration after disposal`);
        if (proof.registration !== null) throw new Error(`${canvasId}: duplicate semantic context`);
        proof.registration = registration;
        proof.execution = new harness.AuthoringExecutionClient(canvas, {
          onError(error, owner) { proof.fail(new Error(`${canvasId} ${owner}: ${error}`)); },
        });
        await proof.execution.startSemanticExecution(registration.semanticExecution, {
          authoringClient: harness.authoring,
          loopDurationSeconds: Math.max(1, registration.duration),
          transportMode: "transferable", pacing,
        });
        if (proof.closed) throw new Error(`${canvasId}: attachment completed after disposal`);
        resolveAttached();
      },
    }));
    void proof.authored.then(() => {
      proof.settled = true;
      if (proof.registration === null) proof.fail(new Error(`${canvasId}: source did not register a continuation`));
    }, error => { proof.settled = true; proof.fail(error); });
    await proof.wait(attached, "attachment");
    let midpoint = null;
    if (pacing === "external_samples") {
      // The ordinary external-sample API drives real Rust callback barriers and
      // waits for renderer observation. No guessed wall-time window or seek.
      for (const time of [0, sampleTime]) {
        if (proof.settled) throw new Error(`${canvasId}: source completed before midpoint`);
        await proof.wait(proof.execution.sampleToAuthoredTime(time), `sample ${time}`);
        midpoint = await proof.wait(proof.execution.state(), `state ${time}`);
        if (proof.settled) throw new Error(`${canvasId}: source completed before midpoint`);
        if (midpoint.time !== time) {
          throw new Error(`${canvasId}: expected authored time ${time}, received ${midpoint.time}`);
        }
      }
    }
    return { canvasId, midpoint, registration: proof.registration, pacing };
  } catch (error) {
    proof.dispose();
    throw error;
  }
}

export async function finishCallbackProof(endpoint) {
  const proof = window.sharedAuthoringSmoke.callbackProof;
  if (!proof || proof.closed) throw new Error("no active callback proof");
  try {
    if (!Number.isFinite(endpoint) || endpoint <= 0) throw new TypeError("invalid callback endpoint");
    // Realtime qualification never sends an external sample, seek, or advance:
    // it must finish using the normal autonomous driver, with the same fixture.
    if (proof.pacing === "external_samples") {
      await proof.wait(proof.execution.sampleToAuthoredTime(endpoint), `sample endpoint ${endpoint}`);
    }
    const authored = await proof.wait(proof.authored, `${proof.pacing} source completion`);
    if (authored.duration !== endpoint ||
        authored.semanticExecution.contextId !== proof.registration.semanticExecution.contextId ||
        authored.semanticExecution.continuationGeneration !== proof.registration.generation ||
        !Number.isSafeInteger(authored.semanticExecution.callbackSessionId)) {
      throw new Error("callback completion changed duration, context, generation, or callback identity");
    }
    const metrics = (await proof.wait(proof.execution.metrics(), "final renderer observation")).metrics;
    return { authored, metrics };
  } catch (error) {
    proof.dispose();
    throw error;
  }
}

export function disposeCallbackProof() {
  const proof = window.sharedAuthoringSmoke.callbackProof;
  const failure = proof?.failure;
  proof?.dispose();
  if (failure) throw failure;
}
