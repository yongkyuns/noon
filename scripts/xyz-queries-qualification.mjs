import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs, resolveQualifiedBackend } from "./manim-raster-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.resolve(process.env.NOON_XYZ_QUERY_ARTIFACTS ?? "browser-smoke-artifacts/xyz-queries");
await mkdir(output, { recursive: true });
const sha = value => createHash("sha256").update(value).digest("hex");
const fixtureText = await readFile(path.join(root, "fixtures/manim-v0.21/xyz-queries.json"), "utf8");
const fixture = JSON.parse(fixtureText);
const observer = await readFile(path.join(root, "scripts/manim_xyz_query_observation.py"), "utf8");
const generator = await readFile(path.join(root, "scripts/manim-xyz-queries-reference.py"), "utf8");
const helper = await readFile(path.join(root, "scripts/manim_depth_affine_observation.py"), "utf8");
assert.equal(fixture.schemaVersion, 1);
assert.equal(fixture.manimVersion, "0.21.0");
assert.equal(fixture.observerSha256, sha(observer));
assert.equal(fixture.generatorSha256, sha(generator));
assert.equal(fixture.helperSha256, sha(helper));
assert.equal(fixture.cases.length, 40);
assert.deepEqual(fixture.phases, ["authored", "bound", "edited", "removed"]);
const localObserver = observer.replace("from manim_depth_affine_observation import POINTS", "");
assert.notEqual(localObserver, observer);
const marker = "NOON_BOUND_XYZ_RESULT:";

function sourceFor(spec) {
  return `${helper}
${localObserver}
import json
import noon as m
SPEC = json.loads(${JSON.stringify(JSON.stringify(spec))})
class BoundXYZQueries(m.Scene):
    async def construct(self):
        phases = []
        phase = 'authored'
        try:
            root = prepare_root(m, SPEC)
            phases.append({'phase': phase, 'value': collect_queries(root, SPEC)})
            phase = 'bound'
            self.add(root)
            await self.wait(.001)
            phases.append({'phase': phase, 'value': collect_queries(root, SPEC)})
            phase = 'edited'
            root.shift(np.array([.5, .25, 1.125]))
            phases.append({'phase': phase, 'value': collect_queries(root, SPEC)})
            phase = 'removed'
            self.remove(root)
            await self.wait(.001)
            phases.append({'phase': phase, 'value': collect_queries(root, SPEC)})
            failure = None
        except Exception as error:
            failure = {'phase': phase, 'type': type(error).__name__, 'message': str(error)}
        payload = {'spec': SPEC, 'phases': phases, 'failure': failure}
        raise RuntimeError('${marker}' + json.dumps(payload, separators=(',', ':')))
`;
}

function equivalent(actual, expected) {
  if (typeof actual === "number" && typeof expected === "number") {
    return Math.abs(actual - expected) <= 1e-12 * (1 + Math.abs(expected));
  }
  if (Array.isArray(actual) || Array.isArray(expected)) {
    return Array.isArray(actual) && Array.isArray(expected) && actual.length === expected.length
      && actual.every((value, index) => equivalent(value, expected[index]));
  }
  if (actual && expected && typeof actual === "object" && typeof expected === "object") {
    return Object.keys(actual).length === Object.keys(expected).length
      && Object.keys(expected).every(key => equivalent(actual[key], expected[key]));
  }
  return actual === expected;
}

const reportPath = path.join(output, "xyz-queries-report.json");
const report = {
  fixtureSha256: sha(fixtureText), observerSha256: sha(observer),
  generatorSha256: sha(generator), helperSha256: sha(helper),
  qualificationSha256: sha(await readFile(fileURLToPath(import.meta.url))),
  runtimeIdentity: JSON.parse(await readFile(path.join(root, "web/runtime-build-identity.json"), "utf8")),
  scope: "Actual CE authored, Scene.add/update, fullXYZ edit and Scene.remove/update controls from the pinned observer. Noon wait(.001) establishes a coherent publication. One fresh scene per profile, exact output types/shapes/errors/reducer traces and fullXYZ values. No staged-callback, timeline, writable-array or broad-class completion claim.",
  backends: [],
};
await writeFile(reportPath, JSON.stringify(report, null, 2) + "\n", { flag: "wx" });
const cache = createPyodideResourceCache(await readFile(path.join(root, "web/python-worker.js"), "utf8"));
const server = await serveRepository(root, 0);
try {
  for (const backend of ["webgpu", "webgl"]) {
    const browser = await chromium.launch({ channel: "chromium", headless: true, args: browserArgs(backend) });
    try {
      const context = await browser.newContext();
      await cache.install(context);
      const page = await context.newPage();
      page.setDefaultTimeout(120000);
      const pageErrors = [];
      page.on("pageerror", error => pageErrors.push(String(error)));
      await page.goto(`${server.baseUrl}/web/manim-compat-smoke.html`, { waitUntil: "load" });
      await page.waitForFunction(() => window.noonManimCompat);
      const control = await page.evaluate(() => window.noonManimCompat.runLive(
        "import noon as m\nclass BoundBackendControl(m.Scene):\n async def construct(self):\n  self.add(m.Circle(radius=.1))\n  await self.wait(.01)\n",
      ));
      resolveQualifiedBackend(backend === "webgpu" ? "WebGPU" : "WebGL2", control.metrics.backend);
      const result = { requested: backend, actualBackend: control.metrics.backend, cases: [], summaries: [] };
      report.backends.push(result);
      for (const [index, expected] of fixture.cases.entries()) {
        const source = sourceFor(expected.spec);
        await writeFile(path.join(output, `xyz-queries-${backend}-${index}-source.py`), source, { flag: "wx" });
        let exported;
        try {
          await page.evaluate(source => window.noonManimCompat.runLive(source), source);
        } catch (error) {
          exported = String(error);
        }
        await writeFile(path.join(output, `xyz-queries-${backend}-${index}-export.log`),
          exported ?? "No exported observation", { flag: "wx" });
        assert.ok(exported?.includes(marker), exported);
        const observed = JSON.parse(exported.split(marker)[1].split("\n")[0]);
        assert.deepEqual(observed.spec, expected.spec);
        result.cases.push({ sourceSha256: sha(source), observed });
        for (const phase of expected.phases) {
          const actual = observed.phases.find(row => row.phase === phase.phase);
          if (actual) {
            assert.deepEqual(actual.value.spec, expected.spec);
            assert.equal(actual.value.observations.length, 38);
          }
          for (const [queryIndex, query] of phase.value.observations.entries()) {
            const actualQuery = actual?.value.observations[queryIndex] ?? { phaseFailure: observed.failure };
            result.summaries.push({ spec: expected.spec, phase: phase.phase, queryIndex,
              method: query.method, matches: equivalent(actualQuery, query), observed: actualQuery, expected: query });
          }
        }
        await writeFile(reportPath, JSON.stringify(report, null, 2) + "\n");
      }
      assert.equal(result.summaries.length, 6080);
      assert.deepEqual(pageErrors, []);
      const mismatches = result.summaries.filter(row => !row.matches);
      console.log(JSON.stringify({ backend, observations: 6080,
        matches: result.summaries.length - mismatches.length, mismatches: mismatches.length }));
      assert.equal(mismatches.length, 0, JSON.stringify(mismatches.slice(0, 2)));
      await context.close();
    } finally {
      await browser.close();
    }
  }
  report.status = "All 12,160 authored/bound/edit/remove observations match the pinned reference";
  report.pyodideResourceCache = cache.stats();
  await writeFile(reportPath, JSON.stringify(report, null, 2) + "\n");
} finally {
  await server.close();
}
