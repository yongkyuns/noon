import assert from "node:assert/strict";
import { after, beforeEach, test } from "node:test";

class FakeCanvas {
  constructor() {
    this.width = 800;
    this.height = 450;
    this.clientWidth = 800;
    this.clientHeight = 450;
    this.replacement = null;
  }

  cloneNode() {
    const replacement = new FakeCanvas();
    replacement.clientWidth = this.clientWidth;
    replacement.clientHeight = this.clientHeight;
    return replacement;
  }

  replaceWith(replacement) {
    this.replacement = replacement;
  }
}

const activeObservers = new Set();
class FakeResizeObserver {
  constructor(callback) {
    this.callback = callback;
  }

  observe(target) {
    this.target = target;
    activeObservers.add(this);
  }

  disconnect() {
    activeObservers.delete(this);
    this.target = null;
  }
}

const originalCanvas = globalThis.HTMLCanvasElement;
const originalResizeObserver = globalThis.ResizeObserver;
const originalWindow = globalThis.window;
globalThis.HTMLCanvasElement = FakeCanvas;
globalThis.ResizeObserver = FakeResizeObserver;
globalThis.window = { devicePixelRatio: 1 };

const {
  AUTHORING_EXECUTION_LEGACY,
  AUTHORING_EXECUTION_RETAINED,
  AuthoringExecutionClient,
} = await import("./authoring-execution-client.js");

after(() => {
  if (originalCanvas === undefined) delete globalThis.HTMLCanvasElement;
  else globalThis.HTMLCanvasElement = originalCanvas;
  if (originalResizeObserver === undefined) delete globalThis.ResizeObserver;
  else globalThis.ResizeObserver = originalResizeObserver;
  if (originalWindow === undefined) delete globalThis.window;
  else globalThis.window = originalWindow;
});

beforeEach(() => {
  assert.equal(activeObservers.size, 0, "previous lifecycle test leaked a resize observer");
});

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function ready(backend = "TestGPU") {
  return {
    render: { backend },
    transportMode: "transferable",
  };
}

class ControlledExecutionClient {
  constructor(
    canvas,
    {
      startGate = null,
      restartGate = null,
      stateGate = null,
      backend = "TestGPU",
      stateValue = { time: 0, playing: true },
    } = {},
  ) {
    this.canvas = canvas;
    this.startGate = startGate;
    this.restartGate = restartGate;
    this.stateGate = stateGate;
    this.backend = backend;
    this.stateValue = stateValue;
    this.terminateCount = 0;
    this.resizeCount = 0;
  }

  async start() {
    if (this.startGate !== null) await this.startGate.promise;
    return ready(this.backend);
  }

  async configureHostCallbacks() {}

  async state() {
    if (this.stateGate !== null) await this.stateGate.promise;
    return this.stateValue;
  }

  async restart() {
    if (this.restartGate !== null) await this.restartGate.promise;
    return ready(this.backend);
  }

  resize() {
    this.resizeCount += 1;
  }

  terminate() {
    this.terminateCount += 1;
  }
}

function clientFactories({ legacy = [], retained = [] } = {}) {
  const created = { legacy: [], retained: [] };
  return {
    created,
    factories: {
      legacy(canvas) {
        const spec = legacy.shift();
        assert.ok(spec !== undefined, "unexpected legacy execution client creation");
        const client = new ControlledExecutionClient(canvas, spec);
        created.legacy.push(client);
        return client;
      },
      retained(canvas) {
        const spec = retained.shift();
        assert.ok(spec !== undefined, "unexpected retained execution client creation");
        const client = new ControlledExecutionClient(canvas, spec);
        created.retained.push(client);
        return client;
      },
    },
  };
}

function retainedDocument(objects) {
  return JSON.stringify({ channel: "noon.authoring.retained", objects });
}

async function assertUnstarted(client) {
  assert.equal(client.mode, null);
  assert.equal(client.rendererBackend, "");
  assert.equal(client.transportMode, null);
  await assert.rejects(client.state(), /has not been started/);
  assert.equal(activeObservers.size, 0, "terminated client must not re-observe a stale canvas");
}

test("terminate during initial start prevents stale worker publication", async () => {
  const gate = deferred();
  const { factories, created } = clientFactories({ legacy: [{ startGate: gate }] });
  const client = new AuthoringExecutionClient(new FakeCanvas(), { clientFactories: factories });
  const pending = client.start('{"version":1,"objects":[],"tracks":[]}');
  const candidate = created.legacy[0];

  client.terminate();
  gate.resolve();

  await assert.rejects(pending, /terminated during an asynchronous execution operation/);
  assert.equal(candidate.terminateCount, 1, "stale unpublished candidate must terminate exactly once");
  await assertUnstarted(client);
});

test("terminate during legacy to retained rebuild prevents stale publication", async () => {
  const retainedGate = deferred();
  const { factories, created } = clientFactories({
    legacy: [{}],
    retained: [{ startGate: retainedGate, stateValue: { retained: true } }],
  });
  const client = new AuthoringExecutionClient(new FakeCanvas(), { clientFactories: factories });
  await client.start('{"version":1,"objects":[],"tracks":[]}');
  assert.equal(client.mode, AUTHORING_EXECUTION_LEGACY);

  const pending = client.reconcileScene('{"version":1,"objects":[],"tracks":[]}', {
    retainedDocumentJson: retainedDocument([{}]),
  });
  const oldLegacy = created.legacy[0];
  const candidate = created.retained[0];
  assert.equal(oldLegacy.terminateCount, 1, "mode transition must retire the published legacy client");

  client.terminate();
  retainedGate.resolve();

  await assert.rejects(pending, /terminated during an asynchronous execution operation/);
  assert.equal(candidate.terminateCount, 1, "stale retained candidate must terminate exactly once");
  await assertUnstarted(client);
});

test("terminate during retained to legacy rebuild prevents stale publication", async () => {
  const legacyGate = deferred();
  const { factories, created } = clientFactories({
    legacy: [{}, { startGate: legacyGate, stateValue: { legacy: true } }],
    retained: [{ stateValue: { retained: true } }],
  });
  const client = new AuthoringExecutionClient(new FakeCanvas(), { clientFactories: factories });
  await client.start('{"version":1,"objects":[],"tracks":[]}');
  await client.reconcileScene('{"version":1,"objects":[],"tracks":[]}', {
    retainedDocumentJson: retainedDocument([{}]),
  });
  assert.equal(client.mode, AUTHORING_EXECUTION_RETAINED);

  const pending = client.reconcileScene('{"version":1,"objects":[],"tracks":[]}', {
    retainedDocumentJson: retainedDocument([]),
  });
  const oldRetained = created.retained[0];
  const candidate = created.legacy[1];
  assert.equal(oldRetained.terminateCount, 1, "mode transition must retire the published retained client");

  client.terminate();
  legacyGate.resolve();

  await assert.rejects(pending, /terminated during an asynchronous execution operation/);
  assert.equal(candidate.terminateCount, 1, "stale legacy candidate must terminate exactly once");
  await assertUnstarted(client);
});

test("terminate during restart prevents stale metadata publication", async () => {
  const restartGate = deferred();
  const { factories, created } = clientFactories({ legacy: [{ restartGate }] });
  const client = new AuthoringExecutionClient(new FakeCanvas(), { clientFactories: factories });
  await client.start('{"version":1,"objects":[],"tracks":[]}');
  const player = created.legacy[0];

  const pending = client.restart();
  client.terminate();
  restartGate.resolve();

  await assert.rejects(pending, /terminated during an asynchronous execution operation/);
  assert.equal(player.terminateCount, 1, "published player is owned by terminate, not stale restart");
  await assertUnstarted(client);
});
