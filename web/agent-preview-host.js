import { PythonAuthoringClient } from "./authoring-client.js";
import { AuthoringExecutionClient } from "./authoring-execution-client.js";
import { SemanticPreviewSession } from "./semantic-preview-session.js";

const canvas = document.querySelector("#scene");
let preview = null;
let closed = false;

function waitForPaint() {
  return new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
}

async function open(source, loopDurationSeconds = 4) {
  if (closed) throw new Error("agent preview host is closed");
  if (preview !== null) throw new Error("agent preview host opens one scene per page");
  preview = new SemanticPreviewSession({
    createAuthoringClient: () => new PythonAuthoringClient(),
    createExecutionClient: (options) => new AuthoringExecutionClient(canvas, options),
  });
  const snapshot = await preview.open(source, { loopDurationSeconds });
  await waitForPaint();
  return snapshot;
}

async function sample(timeSeconds) {
  if (closed) throw new Error("agent preview host is closed");
  if (preview === null) throw new Error("agent preview host has no scene");
  const snapshot = await preview.sample(timeSeconds);
  await waitForPaint();
  return snapshot;
}

function status() {
  return preview?.snapshot ?? null;
}

function close(reason = "agent preview host closed") {
  if (closed) return status();
  closed = true;
  return preview?.close(reason) ?? null;
}

window.noonAgentPreviewHost = Object.freeze({ open, sample, status, close });
