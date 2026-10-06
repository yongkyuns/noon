// Browser capability qualification only; production host bindings stay untouched.
import assert from "node:assert/strict";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";

export const unsupportedJspiSource = `from noon import *
import builtins
import pyodide.ffi
import _noon_host

assert not pyodide.ffi.can_run_sync(), "JSPI must be disabled before worker initialization"
assert not _noon_host.can_wait_sync(), "Noon must bind the no-JSPI host capability"

class UnsupportedJspiContinuation(Scene):
    def construct(self):
        self.circle = Circle(radius=0.4)
        self.add(self.circle)
        builtins._noon_unsupported_jspi_scene = self
        self.animate_circle()

    def animate_circle(self):
        # A helper barrier must keep its original synchronous stack, not lower
        # to portable async and accidentally bypass the capability under test.
        self.play(self.circle.animate.shift((2.0, 0.0, 0.0)), run_time=1.0, rate_func=linear)
`;

export const verifyUnsupportedJspiSource = `from noon import *
import builtins
import _noon_host

try:
    assert not _noon_host.can_wait_sync()
    scene = builtins._noon_unsupported_jspi_scene
    context = scene._canonical_authoring_context
    assert context.liveExecutionOwnership() == "none"
    assert context.authoredDuration() == 0.0
    assert scene.time == 0.0
    assert scene.circle.get_center() == (0.0, 0.0)
finally:
    del builtins._noon_unsupported_jspi_scene

result = Scene()
`;

// Separate browser/worker ownership prevents this negative capability case from
// altering the shared worker used by the remaining visual/lifecycle scenarios.
export async function qualifyUnsupportedJspi(browser, baseUrl) {
  const context = await browser.newContext({ viewport: { width: 800, height: 500 } });
  try {
    // Removal happens before Pyodide and Noon resolve their immutable host APIs.
    await disableAuthoringJspi(context, { crossOriginIsolated: true });
    const page = await context.newPage();
    const pageErrors = [];
    page.on("pageerror", error => pageErrors.push(String(error)));
    await page.goto(`${baseUrl}/web/execution-worker-smoke.html`, { waitUntil: "load" });
    const wrapped = await page.evaluate(async () => {
      const { PythonAuthoringClient } = await import("./authoring-client.js");
      const authoring = new PythonAuthoringClient();
      window.noonUnsupportedJspi = authoring;
      await authoring.ready();
      return window.__noonNoJspiWorkerWrapped === true;
    });
    assert.equal(wrapped, true, "no-JSPI qualification did not wrap the production worker");
    const rejected = await page.evaluate(async (source) => {
      const authoring = window.noonUnsupportedJspi;
      let error = null;
      try {
        await authoring.run(source, {});
      } catch (failure) {
        error = String(failure);
      }
      return { error, terminated: authoring.terminated };
    }, unsupportedJspiSource);

    // Check the original failure BEFORE another request can replace its error.
    assert.match(rejected.error ?? "",
      /ordinary synchronous canonical play\/wait requires Pyodide JS Promise Integration/,
      "synchronous helper did not fail at the JSPI admission gate");
    assert.equal(rejected.terminated, false,
      "JSPI admission rejection must leave its Python authoring worker usable");

    const recovered = await page.evaluate(async (source) => {
      const authoring = window.noonUnsupportedJspi;
      const result = await authoring.run(source, {});
      return { semanticExecution: Object.hasOwn(result, "semanticExecution"),
        terminated: authoring.terminated };
    }, verifyUnsupportedJspiSource);
    assert.deepEqual(recovered, { semanticExecution: true, terminated: false },
      "JSPI rejection changed scene state or prevented another source invocation");
    assert.deepEqual(pageErrors, [], "unexpected no-JSPI page error");
  } finally {
    await context.close();
  }
}
