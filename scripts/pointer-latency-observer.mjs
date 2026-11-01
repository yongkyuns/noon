// Qualification only (#2298). Install in the Window before physical gestures.
// Observe the normal collector and MessagePort boundary without driving input,
// retrying a request, changing a clock, or choosing a presentation for a control.
export function observePointerLatency(ExecutionWorkerClient, { clockId, capacity = 10000 } = {}) {
  if (typeof clockId !== "string" || !clockId || !Number.isSafeInteger(capacity) || capacity < 1) {
    throw new TypeError("a navigation clock ID and positive observation capacity are required");
  }
  const attempts = [], presentations = [], observerErrors = [];
  const inputs = new Map(), requests = new Map(), controls = new Map();
  const pictures = new Map(), awaitingPictures = new Map();
  const ports = new Map();
  let current = null, action = "unlabelled", serial = 0, dropped = 0, stopped = false;
  const now = () => performance.now();
  const inputKey = input => JSON.stringify(input);
  const requestKey = (port, id) => `${port}:${id}`;
  const pictureKey = (port, receipt) => {
    const fields = ["session", "sequence", "presentation", "view_revision"];
    if (!receipt || !fields.every(field => Number.isSafeInteger(receipt[field]) && receipt[field] >= 0) ||
        receipt.presentation === 0) throw new Error("incomplete control presentation receipt");
    return JSON.stringify([port, ...fields.map(field => receipt[field])]);
  };
  const originalSubmit = ExecutionWorkerClient.prototype.submitBrowserPointerInput;
  const originalPost = MessagePort.prototype.postMessage;
  const originalListen = MessagePort.prototype.addEventListener;
  const listenDescriptor = Object.getOwnPropertyDescriptor(MessagePort.prototype, "addEventListener");
  const error = value => {
    if (observerErrors.length < 16) observerErrors.push(String(value));
    else dropped++;
  };
  // An observer failure must never replace a product return value or rejection.
  const observe = operation => { try { operation(); } catch (failure) { error(failure); } };
  const record = (array, value) => {
    if (array.length >= capacity) { dropped++; return false; }
    array.push(value); return true;
  };
  const onDom = event => observe(() => {
    if (stopped || !event.target?.closest?.("canvas") || event.isPrimary !== true || event.button !== 0) return;
    const kind = event.type === "pointerdown" ? "press" : "release";
    const row = { clockId, attemptId: `${action}:${++serial}:${kind}`, action, kind,
      endpoint: "not-posted", domMs: now(), trusted: event.isTrusted === true, outcome: "pending" };
    if (!record(attempts, row)) return;
    current = { event, row };
  });
  for (const type of ["pointerdown", "pointerup"]) document.addEventListener(type, onDom, true);

  function joinPicture(row, observation) {
    if (observation.atMs < row.postedMs) {
      row.preexistingPresentation = { atMs: observation.atMs, receipt: observation.receipt };
      row.presentationLink = "already-visible";
    } else {
      row.presentedMs = observation.atMs;
      row.observedReceipt = observation.receipt;
      row.presentationLink = "matched";
    }
  }

  function onReply(port, message) {
    if (message?.channel !== "noon.engine") return;
    const atMs = now();
    if (message.type === "pointer_presented") {
      const observation = { endpoint: port, clockId, atMs, receipt: structuredClone(message.receipt) };
      if (!record(presentations, observation)) return;
      const key = pictureKey(port, observation.receipt);
      if (pictures.has(key)) { error("duplicate presentation receipt"); return; }
      pictures.set(key, observation);
      for (const row of awaitingPictures.get(key) ?? []) joinPicture(row, observation);
      awaitingPictures.delete(key);
      return;
    }
    const key = requestKey(port, message.requestId);
    const pending = requests.get(key);
    if (!pending) return; // Unissued/duplicate replies never complete an attempt.
    requests.delete(key);
    const { row, type } = pending;
    if (message.type === "error") {
      row.outcome = "error"; row.error = message.message; return;
    }
    if (message.type !== type) { error("mismatched response type " + key); return; }
    if (type === "browser_pointer_input") {
      row.replyMs = atMs;
      if (typeof message.pointerInputAccepted !== "boolean") { error("missing input outcome " + key); return; }
      row.outcome = message.pointerInputAccepted ? "accepted" : "rejected";
      if (message.sceneControl !== undefined) {
        const control = requestKey(port, message.sceneControl.sequence);
        if (controls.has(control)) { error("duplicate pending control " + control); return; }
        controls.set(control, row);
      }
    }
    if (typeof message.sceneControlApplied === "boolean") {
      row.applied = message.sceneControlApplied;
      row.controlReplyMs = atMs; // Window receipt, never internal worker apply time.
      if (row.applied === true) {
        const receipt = message.sceneControlPresentation;
        row.presentationLink = receipt === undefined ? "protocol-absent" : receipt === null ? "unavailable" : "pending";
        if (receipt != null) {
          row.expectedReceipt = structuredClone(receipt);
          const key = pictureKey(port, receipt);
          const observation = pictures.get(key);
          if (observation) joinPicture(row, observation);
          else {
            const rows = awaitingPictures.get(key) ?? [];
            rows.push(row); awaitingPictures.set(key, rows);
          }
        }
      }
    }
  }

  function portId(port) {
    if (ports.has(port)) return ports.get(port).id;
    if (ports.size >= 64) { dropped++; return null; }
    const id = `engine-port-${ports.size + 1}`;
    const listener = event => observe(() => onReply(id, event.data));
    ports.set(port, { id, listener });
    originalListen.call(port, "message", listener);
    return id;
  }

  function submit(input) {
    let row = null;
    observe(() => {
      const occurrence = current;
      if (!occurrence || occurrence.row.kind !== input.kind || occurrence.event.pointerId !== input.pointer_id ||
          occurrence.event.target.closest("canvas") !== this.canvas ||
          occurrence.event.currentTarget !== this.canvas || occurrence.event.eventPhase === 0) return;
      // A browser may run microtasks between capture and target listeners.
      // Check the actual dispatch, rather than clearing association in a
      // microtask or allowing a later async call to inherit the last event.
      current = null;
      row = occurrence.row;
      row.input = structuredClone(input);
      const key = inputKey(input);
      const matches = inputs.get(key) ?? [];
      matches.push(row);
      inputs.set(key, matches);
    });
    let result;
    try { result = originalSubmit.call(this, input); }
    catch (failure) {
      if (row) observe(() => { row.outcome = "error"; row.error = String(failure); });
      throw failure;
    }
    // Observe settlement without replacing the promise used by the product.
    if (row) void Promise.resolve(result).then(() => {}, failure => observe(() => {
      row.outcome = failure?.name === "AbortError" ? "cancelled" : "error";
      row.error = String(failure);
    }));
    return result;
  }

  function post(message, ...args) {
    observe(() => {
      if (stopped || message?.channel !== "noon.engine") return;
      if (!ports.has(this)) error("observer installed after endpoint message handlers; reply ordering is unknown");
      const port = portId(this);
      if (port === null) return;
      let row;
      if (message.type === "browser_pointer_input" && ["press", "release"].includes(message.input?.kind)) {
        const key = inputKey(message.input);
        const matches = inputs.get(key);
        inputs.delete(key);
        if (!matches || matches.length !== 1) {
          error("input post lacks one exact collected occurrence"); return;
        }
        row = matches[0];
        row.endpoint = port; row.requestId = message.requestId; row.postedMs = now();
      } else if (message.type === "scene_control") {
        const key = requestKey(port, message.sequence);
        row = controls.get(key);
        controls.delete(key);
        if (!row) return; // An unrelated control/cancellation has no DOM join.
        row.controlRequestId = message.requestId;
        row.controlPostedMs = now();
      } else return;
      const key = requestKey(port, message.requestId);
      if (requests.has(key)) { error("duplicate issued request " + key); return; }
      requests.set(key, { row, type: message.type });
    });
    try { return originalPost.call(this, message, ...args); }
    catch (failure) {
      observe(() => {
        const port = ports.get(this)?.id;
        const key = requestKey(port, message?.requestId);
        const pending = requests.get(key);
        if (!pending) return;
        requests.delete(key);
        const { row, type } = pending;
        row.outcome = "error"; row.postError = String(failure);
        if (type === "browser_pointer_input") {
          row.postAttemptMs = row.postedMs; row.issuedRequestId = row.requestId;
          delete row.postedMs; delete row.requestId;
        } else {
          row.controlPostAttemptMs = row.controlPostedMs;
          delete row.controlPostedMs;
        }
      });
      throw failure;
    }
  }
  function listen(type, listener, ...args) {
    // Promise continuations can run between browser event listeners. Observe
    // the reply before the client can issue its deferred scene-control request.
    if (type === "message") observe(() => portId(this));
    return originalListen.call(this, type, listener, ...args);
  }
  ExecutionWorkerClient.prototype.submitBrowserPointerInput = submit;
  MessagePort.prototype.postMessage = post;
  MessagePort.prototype.addEventListener = listen;
  return {
    action(name) {
      if (typeof name !== "string" || !name) throw new TypeError("named physical action required");
      action = name;
    },
    snapshot(metadata) {
      return structuredClone({ ...metadata, schema: 1, clock: "window-performance-now", clockId,
        attempts, presentations, dropped, observerErrors,
        presentationJoin: "exact-receipt-when-supplied", replyObservation: "before-existing-client-message-handler" });
    },
    stop() {
      if (stopped) return;
      stopped = true;
      current = null;
      for (const type of ["pointerdown", "pointerup"]) document.removeEventListener(type, onDom, true);
      for (const [port, { listener }] of ports) port.removeEventListener("message", listener);
      if (ExecutionWorkerClient.prototype.submitBrowserPointerInput === submit) {
        ExecutionWorkerClient.prototype.submitBrowserPointerInput = originalSubmit;
      } else error("submit observer ownership changed before removal");
      if (MessagePort.prototype.postMessage === post) MessagePort.prototype.postMessage = originalPost;
      else error("port observer ownership changed before removal");
      if (MessagePort.prototype.addEventListener === listen) {
        if (listenDescriptor) Object.defineProperty(MessagePort.prototype, "addEventListener", listenDescriptor);
        else delete MessagePort.prototype.addEventListener;
      } else error("listener observer ownership changed before removal");
    },
  };
}
