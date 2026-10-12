import {
  EXECUTION_TRANSPORT_SHARED,
  EXECUTION_TRANSPORT_TRANSFERABLE,
  SharedExecutionDeltaWriter,
  TransferableExecutionDeltaSender,
  createSharedExecutionMailbox,
} from "../web/execution-transport.js";

// One injected rejection can arrive asynchronously on both worker channels.
// Keep it separate from unexpected failures instead of clearing the first
// delivery and misclassifying a later delivery of the same rejection.
export function createGlowWorkerErrorCollector() {
  const errors = [], rejections = [];
  let expected = null;
  return {
    errors, rejections,
    expectRejection(message) {
      if (expected !== null || errors.length || typeof message !== "string" || !message) {
        throw new Error("glow negative control requires one explicit rejection and no prior failures");
      }
      expected = message;
    },
    collect(message) {
      // ErrorEvent adds "Uncaught "; port diagnostics carry the raw Rust error.
      (expected !== null && (message === expected || message === `Uncaught ${expected}`)
        ? rejections : errors).push(message);
    },
  };
}

// Fixture producer only: the production semantic endpoint owns its own bootstrap.
// Keep the real mailbox/sender implementations and their backpressure semantics.
export function createGlowWorkerTransport(port, mode, resourceBytes) {
  if (![EXECUTION_TRANSPORT_SHARED, EXECUTION_TRANSPORT_TRANSFERABLE].includes(mode)) {
    throw new Error(`unsupported glow worker transport ${mode}`);
  }
  // Each worker (including recreation) needs its own transferable resource copy.
  const resources = new Uint8Array(resourceBytes);
  if (resources.byteLength === 0) {
    throw new Error("glow worker fixture requires a non-empty retained resource bundle");
  }
  const mailbox = mode === EXECUTION_TRANSPORT_SHARED
    ? createSharedExecutionMailbox(1024 * 1024) : null;
  const sender = mailbox === null
    ? new TransferableExecutionDeltaSender(port) : new SharedExecutionDeltaWriter(mailbox);

  // The renderer drains at transport_setup, not only at shared_delta. The first
  // snapshot can already be visible in shared memory when setup is processed.
  // Match semantic-engine-endpoint.js: queue resources BEFORE exposing the mailbox.
  port.postMessage({ type: "retained_resources", bytes: resources }, [resources.buffer]);
  if (mailbox !== null) {
    port.postMessage({ type: "transport_setup", mode, mailbox });
  }
  return sender;
}
