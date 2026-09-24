from pathlib import Path
import shutil

proposal = Path('../proposal')
for name in ['showcase-posters.mjs', 'showcase-posters.test.mjs']:
    shutil.copyfile(proposal / 'scripts' / name, Path('scripts') / name)

def replace(path, old, new):
    p = Path(path)
    s = p.read_text()
    assert s.count(old) == 1, (path, old[:70], s.count(old))
    p.write_text(s.replace(old, new))

def append(path, text):
    p = Path(path)
    p.write_text(p.read_text() + text)

replace('web/main.js', 'object-fit: cover;', 'object-fit: contain;')
replace('web/playground-playback-controls.js', 'const blockCommands = !this.#controllable || this.#externalBusy || this.#commandPending || this.#seekActive;', '''// Capability denial is not an unfinished command. A completed, non-replayable
    // scene stays disabled without displaying an indefinite wait cursor/aria-busy.
    const busy = this.#externalBusy || this.#commandPending || this.#seekActive;
    const blockCommands = !this.#controllable || busy;''')
replace('web/playground-playback-controls.js', 'this.#root.dataset.busy = String(blockCommands);', 'this.#root.dataset.busy = String(busy);')
replace('web/playground-playback-controls.js', 'this.#root.setAttribute("aria-busy", String(blockCommands));', 'this.#root.setAttribute("aria-busy", String(busy));')
append('web/playground-playback-controls.test.mjs', '''

test("completed unavailable replay is disabled, not indefinitely busy", async () => {
  const f = fixture();
  f.controls.setBusy(true);
  f.controls.setControllable(false);
  f.controls.observe({ time: 9.2, playing: false });
  f.controls.setUnavailable("UnsupportedDomain");
  assert.equal(f.controls.element.dataset.busy, "true", "unavailability cannot clear an actual operation");
  f.controls.setBusy(false);
  assert.equal(f.controls.element.dataset.busy, "false");
  assert.equal(f.controls.element.getAttribute("aria-busy"), "false");
  assert.equal(f.controls.element.dataset.controllable, "false");
  assert.equal(f.controls.element.dataset.elapsedSeconds, "9.2");
  assert.equal(f.output.value, "9.20 s · completed");
  assert.match(f.controls.element.title, /UnsupportedDomain/);
  for (const selector of [".playback-toggle", ".playback-restart", ".playback-scrubber"]) {
    const element = f.preview.querySelector(selector);
    assert.equal(element.disabled, true);
    element.dispatchEvent(new Event(selector.includes("scrubber") ? "input" : "click"));
  }
  await flush();
  assert.deepEqual(f.calls, [], "no rejected capability may dispatch a runtime command");
});

test("unavailable capability does not hide an outstanding seek", async () => {
  const f = fixture(); let resolve;
  f.player.seek = () => new Promise(done => { resolve = done; });
  f.range.value = "1";
  f.range.dispatchEvent(new Event("input"));
  f.controls.setUnavailable("UnsupportedDomain");
  assert.equal(f.controls.element.dataset.busy, "true");
  resolve({ time: 1, playing: false });
  await flush();
  assert.equal(f.controls.element.dataset.busy, "false");
  assert.equal(f.range.disabled, true);
  assert.equal(f.controls.element.dataset.controllable, "false");
  f.controls.setControllable(true);
  assert.equal(f.range.disabled, false);
  assert.equal(f.controls.element.title, "");
});
''')
replace('scripts/showcase-live-review.mjs', 'export function assertLiveOutcome', '''// First execution and replay are distinct engine capabilities. Keep both results,
// but do not count a successful first pass as a replacement for failed replay.
export function assertFirstPass(entry, observed, expectedBackend) {
  assert.equal(observed.selectedExampleId, entry.id, "wrong first-pass lesson");
  assert.equal(observed.patchState, "applied", "first-pass source did not finish successfully");
  assert.equal(observed.runInFlight, false, "first-pass source is still running");
  assert.equal(observed.backend, expectedBackend, "wrong first-pass backend");
  assert.ok(Number.isSafeInteger(observed.objectCount) && observed.objectCount > 0,
    "first-pass lesson has no resolved composition");
  if (entry.performance) assert.ok(observed.objectCount >= 600, "dense scene lost its geometry workload");
  const elapsed = Number(observed.controls?.elapsedSeconds);
  const roundoff = 8 * Number.EPSILON * Math.max(1, entry.duration);
  assert.ok(Number.isFinite(elapsed) && Math.abs(elapsed - entry.duration) <= roundoff,
    "first-pass source did not reach its authored endpoint");
}

export function assertReplayAvailable(observed) {
  assert.equal(observed.controls?.controllable, "true",
    observed.replayReason || "completed source did not admit retained replay");
}

export function assertLiveOutcome''')
replace('scripts/showcase-live-review.mjs', 'const result = { id: entry.id, outcome: "fail", stage: "open", pageErrors: [] };', 'const result = { id: entry.id, outcome: "fail", firstPassOutcome: "not-run", replayOutcome: "not-run", stage: "open", pageErrors: [] };')
replace('scripts/showcase-live-review.mjs', 'result.stage = "ordinary first pass";', 'result.stage = "ordinary first pass";\n        result.firstPassOutcome = "fail";')
replace('scripts/showcase-live-review.mjs', '''        result.firstPassPixelSha256 = hash(firstPass.data);
        result.stage = "replay seek to resolved endpoint";''', '''        result.firstPassPixelSha256 = hash(firstPass.data);
        const firstMetrics = await page.evaluate(() => window.__noonExampleGallery.executionMetrics());
        result.firstPass.objectCount = firstMetrics.metrics.objectCount;
        assertFirstPass(entry, result.firstPass, expectedBackend);
        assert.deepEqual(result.pageErrors, [], "ordinary source execution raised browser errors");
        result.firstPassOutcome = "pass";
        result.stage = "replay capability admission";
        result.replayOutcome = "fail";
        assertReplayAvailable(result.firstPass);
        result.stage = "replay seek to resolved endpoint";''')
replace('scripts/showcase-live-review.mjs', '''        result.restartRestoresEndpoint = true;
        result.outcome = "pass";''', '''        result.restartRestoresEndpoint = true;
        result.replayOutcome = "pass";
        result.outcome = "pass";''')
replace('scripts/showcase-live-review.test.mjs', 'assertLiveOutcome, assertLiveEndpoint, assertLivePixels', 'assertFirstPass, assertReplayAvailable, assertLiveOutcome, assertLiveEndpoint, assertLivePixels')
append('scripts/showcase-live-review.test.mjs', '''

test("successful first execution cannot substitute for unavailable replay", () => {
  const state = observed();
  state.controls.controllable = "false";
  state.replayReason = "Replay unavailable: UnsupportedDomain";
  assertFirstPass(entry, state, "WebGPU");
  assert.throws(() => assertReplayAvailable(state), /UnsupportedDomain/);
  assert.throws(() => assertLiveOutcome(entry, state, "WebGPU"));
});

test("first-pass errors and incomplete execution fail before replay is attempted", () => {
  for (const change of [
    { patchState: "error" }, { runInFlight: true }, { selectedExampleId: "wrong" },
    { backend: "WebGL2" }, { objectCount: 0 }, { objectCount: NaN },
    { controls: { elapsedSeconds: "0" } }, { controls: { elapsedSeconds: "7.999" } },
    { controls: { elapsedSeconds: "NaN" } },
  ]) assert.throws(() => assertFirstPass(entry, { ...observed(), ...change }, "WebGPU"));
  assert.throws(() => assertFirstPass({ ...entry, performance: true }, observed(), "WebGPU"));
});
''')
replace('scripts/showcase-capture.mjs', 'import { normalizeShowcaseManifest }', 'import { posterEvidence } from "./showcase-posters.mjs";\nimport { normalizeShowcaseManifest }')
replace('scripts/showcase-capture.mjs', '  // Stage real posters only after every scene passes. They remain review artifacts;', '''  const evidence = json(posterEvidence(manifest, report));
  await writeFile(path.join(output, "capture-evidence.json"), evidence);
  // Stage real posters only after every scene passes. They remain review artifacts;''')
replace('scripts/showcase-capture.mjs', '  const decodePage = await context.newPage();', '''  await writeFile(path.join(root, "web/thumbnails/showcase/capture-evidence.json"), evidence);
  const decodePage = await context.newPage();''')
replace('.github/workflows/playground-showcase-qualification.yml', "      - 'web/showcase-*'", "      - 'web/showcase-*'\n      - 'web/thumbnails/showcase/**'")
replace('.github/workflows/playground-showcase-qualification.yml', 'node --test web/showcase-gallery.test.mjs scripts/showcase-*.test.mjs', 'node --test web/showcase-gallery.test.mjs web/playground-playback-controls.test.mjs scripts/showcase-*.test.mjs')
