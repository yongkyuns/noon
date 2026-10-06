"""Temporary #1653 experiment. Do not merge; no production implementation."""
import hashlib
import json
import pathlib
import subprocess
import sys

root = pathlib.Path(sys.argv[1])
site = pathlib.Path(sys.argv[2])
evidence = pathlib.Path(sys.argv[3])
mode = sys.argv[4]
assert mode in {"control", "normal", "small", "timer"}
evidence.mkdir(parents=True, exist_ok=True)

def sha(data):
    return hashlib.sha256(data).hexdigest()

def replace(text, old, new):
    assert text.count(old) == 1, (old, text.count(old))
    return text.replace(old, new)

identity = json.loads((site / "web/runtime-build-identity.json").read_text())
assert identity["sourceRevision"] == "886ede3e2243d621cda5823b30a7911f4e382417"
for item in identity["files"].values():
    assert sha((site / "web" / item["path"]).read_bytes()) == item["sha256"], item
profile = mode != "control"
overrides = []
prefix = '''
// Temporary #1653 diagnostic overlay. Not production code or qualification.
const __camMark = (name, detail) => performance.mark('cam.' + name, {detail});
const __camSync = (name, call) => { const start = performance.now(); try { return call(); }
  finally { performance.measure('cam.' + name, {start, end: performance.now()}); } };
const __camAsync = async (name, call) => { const start = performance.now(); try { return await call(); }
  finally { performance.measure('cam.' + name, {start, end: performance.now()}); } };
'''

def wrap(text, name, args, asynchronous=False):
    kind = "async function" if asynchronous else "function"
    old = f"{kind} {name}({args}) {{"
    helper = "__camAsync" if asynchronous else "__camSync"
    new = (f'{kind} {name}({args}) {{ return {helper}("{name}", () => __camOriginal_{name}({args})); }}\n'
           f"{kind} __camOriginal_{name}({args}) {{")
    return replace(text, old, new)

def save(name, old, text):
    if old == text:
        return
    (site / "web" / name).write_text(text)
    subprocess.run(["node", "--check", str(site / "web" / name)], check=True)
    overrides.append({"path": name, "originalSha256": sha(old.encode()),
                      "diagnosticSha256": sha(text.encode())})

for name in ["authoring-render-controller.js", "semantic-engine-endpoint.js", "authoring-render-worker.js"]:
    text = old = (site / "web" / name).read_text()
    assert (root / "web" / name).read_bytes() == old.encode(), name
    if profile and name == "authoring-render-controller.js":
        text = prefix + text
        for function, args in [("tryPresent", ""), ("applyRendererDelta", "json"), ("drainGpuDiagnostics", "")]:
            text = wrap(text, function, args)
        text = wrap(text, "flushGpuDiagnostics", "", True)
        text = replace(text, 'function handleEngineMessage(message) {', 'function handleEngineMessage(message) { __camMark("render.message." + message?.type, {sequence:message?.sequence});')
        text = replace(text, 'function consumeDelta(json, publication = null) {', 'function consumeDelta(json, publication = null) { __camMark("render.consume", publication);')
        text = replace(text, 'if (!renderer.render()) {', 'if (!__camSync("renderer.call", () => renderer.render())) {')
        text = replace(text, 'renderPort?.postMessage({ type: "tick", timestamp });', '__camMark("render.tick.sent", {timestamp}); renderPort?.postMessage({ type: "tick", timestamp });')
        text = replace(text, 'handle: host.requestAnimationFrame((timestamp) => frame(timestamp, generation, ticket)),', 'handle: host.requestAnimationFrame((timestamp) => { __camMark("raf.delivered", {ticket, timestamp}); frame(timestamp, generation, ticket); }),')
        text = replace(text, 'if (needsAnimationFrame && typeof host?.requestAnimationFrame === "function") {', 'if (needsAnimationFrame && typeof host?.requestAnimationFrame === "function") { __camMark("raf.requested", {ticket});')
        text = replace(text, 'handle: setTimeout(() => frame(performance.now(), generation, ticket), delay),', 'handle: setTimeout(() => { __camMark("timer.delivered", {ticket}); frame(performance.now(), generation, ticket); }, delay),')
    if profile and name == "semantic-engine-endpoint.js":
        text = prefix + text
        text = wrap(text, "driveContinuation", "wallTime", True)
        text = replace(text, 'const drive = player.driveLiveSegmentFromWallTime(wallTime);', 'const drive = __camSync("rust.drive", () => player.driveLiveSegmentFromWallTime(wallTime));')
        text = replace(text, 'const batchJson = await runRequiredCallbackPhase(phase, player);', 'const batchJson = await __camAsync("python.callback", () => runRequiredCallbackPhase(phase, player));')
        text = replace(text, 'const nextRegionJson = player.commitCallbackPhaseJson(batchJson);', 'const nextRegionJson = __camSync("callback.commit", () => player.commitCallbackPhaseJson(batchJson));')
        text = replace(text, 'await completeRequiredCallbackPhase?.(phase);', 'await __camAsync("callback.complete", () => completeRequiredCallbackPhase?.(phase));')
        text = text.replace('player.drainDeltaJson()', '__camSync("delta.drain", () => player.drainDeltaJson())')
        text = replace(text, 'renderPort.addEventListener("message", ({ data: message }) => {', 'renderPort.addEventListener("message", ({ data: message }) => { __camMark("engine.message." + message?.type, {sequence:message?.sequence});')
        text = replace(text, 'executionWakeCadence = cadence;', 'executionWakeCadence = cadence; __camMark("engine.wake.sent", {cadence, timerAfterMilliseconds});')
        text = replace(text, 'lastSentPublication = publication;', 'lastSentPublication = publication; __camMark("engine.delta.sent", publication);')
    if mode == "timer" and name == "authoring-render-worker.js":
        text = replace(text, 'typeof self.requestAnimationFrame === "function"', 'false')
        text = replace(text, 'typeof self.cancelAnimationFrame === "function"', 'false')
    save(name, old, text)

(evidence / "overlay.json").write_text(json.dumps({"diagnosticOnly": True, "mode": mode,
    "runtimeIdentity": identity, "overrides": overrides}, indent=2))
harness = (root / "scripts/playground-product-e2e.mjs").read_text()
harness = replace(harness, 'const cold = await runAndMeasure(page);', '''const cdp = await browser.newBrowserCDPSession();
  await writeFile(path.join(artifactDir, 'gpu-info.json'), JSON.stringify(await cdp.send('SystemInfo.getInfo'), null, 2));
  if (process.env.NOON_CAMERA_MODE === 'small') await page.addStyleTag({content: '.canvas-frame {width:311px !important; height:175px !important;} #scene {width:311px !important; height:175px !important;}'});
  const cold = await runAndMeasure(page);''')
harness = replace(harness, 'const warm = await runAndMeasure(page, { captureFrames: true });', '''const tracing = process.env.NOON_CAMERA_MODE !== 'control';
  if (tracing) await cdp.send('Tracing.start', {
    categories: 'blink.user_timing,devtools.timeline,v8,cc,gpu,disabled-by-default-devtools.timeline,disabled-by-default-v8.cpu_profiler',
    options: 'record-as-much-as-possible', transferMode: 'ReturnAsStream'});
  let warm;
  try { warm = await runAndMeasure(page, {captureFrames:true}); }
  finally {
    if (tracing) {
      const finished = new Promise(resolve => cdp.once('Tracing.tracingComplete', resolve));
      await cdp.send('Tracing.end');
      const {stream} = await finished;
      const chunks = [];
      for (;;) {
        const part = await cdp.send('IO.read', {handle:stream});
        chunks.push(Buffer.from(part.data, part.base64Encoded ? 'base64' : 'utf8'));
        if (part.eof) break;
      }
      await cdp.send('IO.close', {handle:stream});
      const {gzipSync} = await import('node:zlib');
      await writeFile(path.join(artifactDir, 'pipeline.trace.json.gz'), gzipSync(Buffer.concat(chunks)));
    }
  }''')
harness = replace(harness, 'packageSizes: await packageSizes(siteRoot),', 'packageSizes: null, // Diagnostic archive is a Pages site, not a product package.')
harness = replace(harness, 'const report = {', 'const report = {\n    diagnosticOnly:true, cameraDiagnosticMode:process.env.NOON_CAMERA_MODE,')
(root / "scripts/camera-diagnostic-run.mjs").write_text(harness)
subprocess.run(["node", "--check", str(root / "scripts/camera-diagnostic-run.mjs")], check=True)
