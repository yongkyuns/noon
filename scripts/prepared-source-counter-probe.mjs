// Correctness-only observation of owned Python counters. User source performs
// no host I/O. This probe never drives a segment, supplies time or acknowledges
// presentation; timing qualification must use the unmodified worker.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";

export async function installPreparedSourceCounterProbe(context, channel, evidence) {
  await context.route(url => new URL(url).pathname.endsWith("/web/python-worker.js"), async route => {
    if (!["script", "worker"].includes(route.request().resourceType())) return route.continue();
    const response = await route.fetch();
    const original = await response.text();
    const begin = original.indexOf("async function runAuthoringSource("), end = original.indexOf("async function runCanonicalCallbackPhase", begin);
    assert.ok(begin >= 0 && end > begin);
    const section = original.slice(begin, end);
    const needle = "return pyodide.runPythonAsync(";
    const ending = "`), { globals });";
    assert.equal(section.split(needle).length, 2);
    assert.equal(section.split(ending).length, 2);
    const observed = section.replace(needle, "return observeQualificationRun(globals, () => pyodide.runPythonAsync(")
      .replace(ending, "`), { globals }));");
    const ready = "  const bindingsReadyAt = performance.now();";
    assert.equal(original.split(ready).length, 2);
    let body = (original.slice(0, begin) + observed + original.slice(end)).replace(ready, `
  for (const name of ["noonAwaitSemanticContinuation", "noonCompleteSemanticContinuationCallback",
    "noonAcknowledgeSemanticContinuationCallback"]) {
    const original = self[name];
    self[name] = (...args) => { observeQualificationCounters(); return original(...args); };
  }
${ready}`) + `
let qualificationGlobals = null;
function observeQualificationCounters() {
  if (!qualificationGlobals) return;
  const namespace = qualificationGlobals.get("__noon_namespace");
  if (!namespace) return;
  const events = namespace.get("events"), samples = namespace.get("samples");
  try {
    self.postMessage({ channel: ${JSON.stringify(channel)},
      events: events?.toJs() ?? [], samples: samples?.toJs() ?? [] });
  } finally { samples?.destroy(); events?.destroy(); namespace.destroy(); }
}
async function observeQualificationRun(globals, execute) {
  qualificationGlobals = globals;
  try { return await execute(); }
  finally { observeQualificationCounters(); qualificationGlobals = null; }
}
`;
    if (channel === null) {
      body = body.replace("let qualificationGlobals = null;", "let qualificationGlobals = null, qualificationObservedCount = 0;")
        .replace("qualificationGlobals = globals;", "qualificationGlobals = globals; qualificationObservedCount = 0;")
        .replace(`self.postMessage({ channel: null,
      events: events?.toJs() ?? [], samples: samples?.toJs() ?? [] });`, `const values = events?.toJs() ?? [];
    for (const value of values.slice(qualificationObservedCount)) self.console.log(value);
    qualificationObservedCount = values.length;`);
    }
    const sha = value => createHash("sha256").update(value).digest("hex");
    evidence.counterProbe = { scope: "read-only Python counter probe; no timing claim or source I/O exemption",
      originalWorkerSha256: sha(original), executedWorkerSha256: sha(body) };
    const headers = { ...response.headers() }; delete headers["content-length"]; delete headers["content-encoding"];
    await route.fulfill({ response, headers, body });
  });
}
