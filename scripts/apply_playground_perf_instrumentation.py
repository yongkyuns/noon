from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    if new in text:
        return
    if old not in text:
        raise SystemExit(f"expected source fragment not found in {path}:\n{old[:320]}")
    file.write_text(text.replace(old, new, 1))


# Browser timing requires the high-resolution Performance clock.
replace_once(
    "crates/noon-web/Cargo.toml",
    'web-sys = { version = "0.3", features = ["HtmlCanvasElement"] }\n',
    'web-sys = { version = "0.3", features = ["HtmlCanvasElement", "Performance", "Window"] }\n',
)

# Expose CPU phase timings alongside the existing asynchronous GPU timestamp profiler.
replace_once(
    "crates/noon-web/src/lib.rs",
    "        last_bytes_uploaded: usize,\n        last_geometry_cache_misses: usize,\n        gpu_profiler: Option<GpuFrameProfiler>,\n",
    "        last_bytes_uploaded: usize,\n        last_geometry_cache_misses: usize,\n        last_cpu_frame_ms: f64,\n        last_runtime_evaluation_ms: f64,\n        last_frame_prepare_ms: f64,\n        last_upload_ms: f64,\n        last_encode_submit_ms: f64,\n        gpu_profiler: Option<GpuFrameProfiler>,\n",
)
replace_once(
    "crates/noon-web/src/lib.rs",
    "                last_bytes_uploaded: 0,\n                last_geometry_cache_misses: 0,\n                gpu_profiler,\n",
    "                last_bytes_uploaded: 0,\n                last_geometry_cache_misses: 0,\n                last_cpu_frame_ms: f64::NAN,\n                last_runtime_evaluation_ms: f64::NAN,\n                last_frame_prepare_ms: f64::NAN,\n                last_upload_ms: f64::NAN,\n                last_encode_submit_ms: f64::NAN,\n                gpu_profiler,\n",
)
replace_once(
    "crates/noon-web/src/lib.rs",
    "        pub fn render_frame(&mut self, timestamp_ms: f64) -> Result<bool, JsValue> {\n            let scene_time = self.clock.scene_time(timestamp_ms).map_err(js_error)?;\n            self.player.advance_to(scene_time).map_err(js_error)?;\n            self.render_current_frame()\n        }\n",
    "        pub fn render_frame(&mut self, timestamp_ms: f64) -> Result<bool, JsValue> {\n            let frame_started_ms = performance_now_ms();\n            let scene_time = self.clock.scene_time(timestamp_ms).map_err(js_error)?;\n            let runtime_started_ms = performance_now_ms();\n            self.player.advance_to(scene_time).map_err(js_error)?;\n            self.last_runtime_evaluation_ms = elapsed_ms(runtime_started_ms);\n            let presented = self.render_current_frame()?;\n            self.last_cpu_frame_ms = elapsed_ms(frame_started_ms);\n            Ok(presented)\n        }\n",
)
replace_once(
    "crates/noon-web/src/lib.rs",
    "        #[wasm_bindgen(js_name = lastGeometryCacheMisses)]\n        pub fn last_geometry_cache_misses(&self) -> usize {\n            self.last_geometry_cache_misses\n        }\n\n        #[wasm_bindgen(js_name = gpuProfilingSupported)]\n",
    "        #[wasm_bindgen(js_name = lastGeometryCacheMisses)]\n        pub fn last_geometry_cache_misses(&self) -> usize {\n            self.last_geometry_cache_misses\n        }\n\n        #[wasm_bindgen(js_name = lastCpuFrameMs)]\n        pub fn last_cpu_frame_ms(&self) -> f64 {\n            self.last_cpu_frame_ms\n        }\n\n        #[wasm_bindgen(js_name = lastRuntimeEvaluationMs)]\n        pub fn last_runtime_evaluation_ms(&self) -> f64 {\n            self.last_runtime_evaluation_ms\n        }\n\n        #[wasm_bindgen(js_name = lastFramePrepareMs)]\n        pub fn last_frame_prepare_ms(&self) -> f64 {\n            self.last_frame_prepare_ms\n        }\n\n        #[wasm_bindgen(js_name = lastUploadMs)]\n        pub fn last_upload_ms(&self) -> f64 {\n            self.last_upload_ms\n        }\n\n        #[wasm_bindgen(js_name = lastEncodeSubmitMs)]\n        pub fn last_encode_submit_ms(&self) -> f64 {\n            self.last_encode_submit_ms\n        }\n\n        #[wasm_bindgen(js_name = gpuProfilingSupported)]\n",
)
replace_once(
    "crates/noon-web/src/lib.rs",
    "            let changes = self.player.take_frame_changes();\n            let prepared = self\n                .preparer\n                .prepare_incremental(self.player.frame(), &changes);\n            self.last_geometry_cache_misses = prepared.stats.geometry_cache_misses;\n            let upload = self.renderer.upload(&self.device, &self.queue, &prepared);\n            self.last_bytes_uploaded = upload.bytes_uploaded;\n",
    "            let prepare_started_ms = performance_now_ms();\n            let changes = self.player.take_frame_changes();\n            let prepared = self\n                .preparer\n                .prepare_incremental(self.player.frame(), &changes);\n            self.last_frame_prepare_ms = elapsed_ms(prepare_started_ms);\n            self.last_geometry_cache_misses = prepared.stats.geometry_cache_misses;\n            let upload_started_ms = performance_now_ms();\n            let upload = self.renderer.upload(&self.device, &self.queue, &prepared);\n            self.last_upload_ms = elapsed_ms(upload_started_ms);\n            self.last_bytes_uploaded = upload.bytes_uploaded;\n",
)
replace_once(
    "crates/noon-web/src/lib.rs",
    "            let view = surface_texture\n                .texture\n                .create_view(&wgpu::TextureViewDescriptor::default());\n            let mut encoder = self\n",
    "            let view = surface_texture\n                .texture\n                .create_view(&wgpu::TextureViewDescriptor::default());\n            let encode_started_ms = performance_now_ms();\n            let mut encoder = self\n",
)
replace_once(
    "crates/noon-web/src/lib.rs",
    "            self.queue.submit(Some(encoder.finish()));\n            surface_texture.present();\n\n            self.last_draw_calls = draw.draw_calls;\n",
    "            self.queue.submit(Some(encoder.finish()));\n            self.last_encode_submit_ms = elapsed_ms(encode_started_ms);\n            surface_texture.present();\n\n            self.last_draw_calls = draw.draw_calls;\n",
)
replace_once(
    "crates/noon-web/src/lib.rs",
    "    fn create_surface(\n        instance: &wgpu::Instance,\n",
    "    fn performance_now_ms() -> f64 {\n        web_sys::window()\n            .and_then(|window| window.performance())\n            .map_or(f64::NAN, |performance| performance.now())\n    }\n\n    fn elapsed_ms(start_ms: f64) -> f64 {\n        let end_ms = performance_now_ms();\n        if start_ms.is_finite() && end_ms.is_finite() {\n            (end_ms - start_ms).max(0.0)\n        } else {\n            f64::NAN\n        }\n    }\n\n    fn create_surface(\n        instance: &wgpu::Instance,\n",
)

# Reusable bounded sample windows keep the playground metrics stable and testable.
replace_once(
    "web/frame-metrics.js",
    "export function summarizeSamples(samples) {\n",
    "export class SampleWindow {\n  #capacity;\n  #samples = [];\n\n  constructor(capacity = 180) {\n    if (!Number.isSafeInteger(capacity) || capacity <= 0) {\n      throw new RangeError(\"sample window capacity must be a positive integer\");\n    }\n    this.#capacity = capacity;\n  }\n\n  record(value) {\n    if (!Number.isFinite(value)) {\n      throw new TypeError(\"sample window requires finite values\");\n    }\n    this.#samples.push(value);\n    if (this.#samples.length > this.#capacity) {\n      this.#samples.splice(0, this.#samples.length - this.#capacity);\n    }\n  }\n\n  reset() {\n    this.#samples.length = 0;\n  }\n\n  summary() {\n    return summarizeSamples(this.#samples);\n  }\n\n  get size() {\n    return this.#samples.length;\n  }\n}\n\nexport function summarizeSamples(samples) {\n",
)
replace_once(
    "web/frame-metrics.test.mjs",
    'import { FrameMetrics, summarizeSamples } from "./frame-metrics.js";\n',
    'import { FrameMetrics, SampleWindow, summarizeSamples } from "./frame-metrics.js";\n',
)
replace_once(
    "web/frame-metrics.test.mjs",
    'test("rejects non-finite measurements", () => {\n',
    'test("bounded sample windows retain recent measurements", () => {\n  const samples = new SampleWindow(3);\n  samples.record(1);\n  samples.record(2);\n  samples.record(3);\n  samples.record(10);\n\n  assert.equal(samples.size, 3);\n  assert.deepEqual(samples.summary(), {\n    p50: 3,\n    p95: 10,\n    max: 10,\n    mean: 5,\n  });\n  samples.reset();\n  assert.equal(samples.size, 0);\n  assert.equal(samples.summary(), null);\n  assert.throws(() => new SampleWindow(0), /positive integer/);\n  assert.throws(() => samples.record(Number.NaN), /finite values/);\n});\n\ntest("rejects non-finite measurements", () => {\n',
)

# The morph stress source now takes an authoring context so one implementation
# backs several repeatable object-count presets.
stress = Path("web/python/examples/morph_stress_test.py")
stress.write_text('''import math\n\nfrom noon import Color, Scene, Transform, VectorPath\n\nscene = Scene()\n\n# A deliberately dense morph scene that exercises the production path. The\n# playground passes object_count through the worker context so the same source\n# can benchmark several scales without changing the animation semantics.\nrequested_count = context.get("object_count", 600) if isinstance(context, dict) else 600\nif isinstance(requested_count, bool) or not isinstance(requested_count, int):\n    raise TypeError("object_count must be an integer")\nif requested_count <= 0 or requested_count > 10_000:\n    raise ValueError("object_count must be between 1 and 10000")\n\nobject_count = requested_count\nvariant_count = 12\naspect = 1.5\ncolumns = math.ceil(math.sqrt(object_count * aspect))\nrows = math.ceil(object_count / columns)\nspacing_x = 5.8 / max(columns - 1, 1)\nspacing_y = 3.8 / max(rows - 1, 1)\nsource_radius = min(spacing_x, spacing_y) * 0.37\nstroke_width = max(source_radius * 0.24, 0.0025)\n\n\ndef rounded_source(radius: float) -> VectorPath:\n    # Four cubic Beziers approximating a circle. Using the same source shape for\n    # every variant lets target shape diversity, rather than source complexity,\n    # determine the number of cached morph meshes.\n    k = radius * 0.58\n    return (\n        VectorPath()\n        .move_to((0.0, radius))\n        .cubic_to((k, radius), (radius, k), (radius, 0.0))\n        .cubic_to((radius, -k), (k, -radius), (0.0, -radius))\n        .cubic_to((-k, -radius), (-radius, -k), (-radius, 0.0))\n        .cubic_to((-radius, k), (-k, radius), (0.0, radius))\n        .close()\n    )\n\n\ndef star_target(variant: int) -> VectorPath:\n    # Twelve subtly different targets create twelve reusable geometry-cache\n    # entries. Every object therefore updates only its compact instance record\n    # during steady-state morph playback.\n    phase = (variant / variant_count) * math.pi * 0.36\n    outer = source_radius * (1.18 + 0.08 * math.sin(variant * 1.7))\n    inner = outer * (0.42 + 0.05 * math.cos(variant * 0.9))\n    points = []\n    for point_index in range(10):\n        angle = phase + math.pi / 2.0 + point_index * math.pi / 5.0\n        radius = outer if point_index % 2 == 0 else inner\n        points.append((math.cos(angle) * radius, math.sin(angle) * radius))\n\n    target = VectorPath().move_to(points[0])\n    for point in points[1:]:\n        target.line_to(point)\n    return target.close()\n\n\nsource = rounded_source(source_radius)\ntargets = [star_target(variant) for variant in range(variant_count)]\n\nfor index in range(object_count):\n    row = index // columns\n    column = index % columns\n    variant = index % variant_count\n    x = (column - (columns - 1) / 2.0) * spacing_x\n    y = ((rows - 1) / 2.0 - row) * spacing_y\n    phase = (row * 0.19 + column * 0.13) % (2.0 * math.pi)\n\n    color_t = variant / (variant_count - 1)\n    color = Color(\n        0.34 + 0.58 * color_t,\n        0.80 - 0.34 * color_t,\n        0.98 - 0.18 * math.sin(variant * 0.7) ** 2,\n        0.92,\n    )\n    shape = scene.path(\n        source,\n        position=(x, y),\n        rotation=phase * 0.18,\n        fill=None,\n        stroke=color,\n        stroke_width=stroke_width,\n        key=f"stress.{index}",\n    )\n    scene.play(\n        Transform(shape, targets[variant], key=f"stress.{index}.morph"),\n        duration=4.0,\n        easing="ease_in_out_cubic",\n    )\n    scene.animate_rotation(\n        shape,\n        phase * 0.18,\n        phase * 0.18 + (0.75 if (row + column) % 2 == 0 else -0.75),\n        duration=4.0,\n        easing="ease_in_out_cubic",\n        key=f"stress.{index}.rotation",\n    )\n\nresult = scene\n''')

# Playground wiring: stress presets, rolling CPU phase percentiles, and GPU p50/p95.
replace_once(
    "web/main.js",
    'import { diffSceneDocuments, SceneIdentityMap } from "./scene-identity.js";\n',
    'import { diffSceneDocuments, SceneIdentityMap } from "./scene-identity.js";\nimport { SampleWindow } from "./frame-metrics.js";\n',
)
replace_once(
    "web/main.js",
    'const metricTime = document.querySelector("#metric-time");\n',
    'const metricTime = document.querySelector("#metric-time");\nconst metricCpuFrame = document.querySelector("#metric-cpu-frame");\nconst metricRuntime = document.querySelector("#metric-runtime");\nconst metricPrepare = document.querySelector("#metric-prepare");\nconst metricUploadMs = document.querySelector("#metric-upload-ms");\nconst metricEncode = document.querySelector("#metric-encode");\nconst metricGpu = document.querySelector("#metric-gpu");\n',
)
old_stress = '''  {\n    name: "Morph stress test",\n    path: "./python/examples/morph_stress_test.py",\n    summary:\n      "Six hundred simultaneous path morphs reuse twelve cached meshes to stress runtime evaluation, batching and dirty uploads.",\n    features: "600 morphs · 12 meshes · batching · dirty uploads",\n  },\n'''
new_stress = '''  {\n    name: "Morph stress · 600",\n    path: "./python/examples/morph_stress_test.py",\n    context: { object_count: 600 },\n    summary:\n      "Six hundred simultaneous path morphs reuse twelve cached meshes with live CPU/GPU percentile profiling.",\n    features: "600 morphs · 12 meshes · CPU/GPU profile",\n  },\n  {\n    name: "Morph stress · 1,000",\n    path: "./python/examples/morph_stress_test.py",\n    context: { object_count: 1000 },\n    summary:\n      "One thousand simultaneous path morphs reuse twelve cached meshes with live CPU/GPU percentile profiling.",\n    features: "1,000 morphs · 12 meshes · CPU/GPU profile",\n  },\n  {\n    name: "Morph stress · 3,000",\n    path: "./python/examples/morph_stress_test.py",\n    context: { object_count: 3000 },\n    summary:\n      "Three thousand simultaneous path morphs reuse twelve cached meshes to expose CPU preparation and GPU limits.",\n    features: "3,000 morphs · 12 meshes · CPU/GPU profile",\n  },\n'''
replace_once("web/main.js", old_stress, new_stress)
replace_once(
    "web/main.js",
    "  const player = await NoonCanvasPlayer.create(canvas, demoSceneJson(), 4.0);\n",
    "  const player = await NoonCanvasPlayer.create(canvas, demoSceneJson(), 4.0);\n  const gpuProfilingSupported = player.gpuProfilingSupported();\n  player.setGpuProfilingEnabled(gpuProfilingSupported);\n",
)
replace_once(
    "web/main.js",
    "  let authoredScene = null;\n\n  async function loadExample",
    '''  let authoredScene = null;\n  const PERF_SAMPLE_CAPACITY = 180;\n  const PERF_WARMUP_FRAMES = 30;\n  const perfSamples = {\n    cpuFrame: new SampleWindow(PERF_SAMPLE_CAPACITY),\n    runtime: new SampleWindow(PERF_SAMPLE_CAPACITY),\n    prepare: new SampleWindow(PERF_SAMPLE_CAPACITY),\n    upload: new SampleWindow(PERF_SAMPLE_CAPACITY),\n    encode: new SampleWindow(PERF_SAMPLE_CAPACITY),\n  };\n  let perfWarmupRemaining = PERF_WARMUP_FRAMES;\n\n  function resetPerformanceProfile() {\n    Object.values(perfSamples).forEach((samples) => samples.reset());\n    perfWarmupRemaining = PERF_WARMUP_FRAMES;\n    player.resetGpuProfiling();\n  }\n\n  function recordPerformanceFrame() {\n    if (perfWarmupRemaining > 0) {\n      perfWarmupRemaining -= 1;\n      if (perfWarmupRemaining === 0) {\n        player.resetGpuProfiling();\n      }\n      return;\n    }\n    perfSamples.cpuFrame.record(player.lastCpuFrameMs());\n    perfSamples.runtime.record(player.lastRuntimeEvaluationMs());\n    perfSamples.prepare.record(player.lastFramePrepareMs());\n    perfSamples.upload.record(player.lastUploadMs());\n    perfSamples.encode.record(player.lastEncodeSubmitMs());\n  }\n\n  function formatPerfSummary(summary) {\n    if (summary === null) {\n      return "—";\n    }\n    return `${summary.p50.toFixed(2)} / ${summary.p95.toFixed(2)} ms`;\n  }\n\n  function formatGpuSummary() {\n    if (!gpuProfilingSupported) {\n      return "unsupported";\n    }\n    const p50 = player.gpuRenderP50Ms();\n    const p95 = player.gpuRenderP95Ms();\n    if (!Number.isFinite(p50) || !Number.isFinite(p95)) {\n      return "sampling…";\n    }\n    return `${p50.toFixed(2)} / ${p95.toFixed(2)} ms`;\n  }\n\n  function updatePerformanceMetrics() {\n    if (perfWarmupRemaining > 0) {\n      const warmup = `warming ${PERF_WARMUP_FRAMES - perfWarmupRemaining}/${PERF_WARMUP_FRAMES}`;\n      metricCpuFrame.value = warmup;\n      metricRuntime.value = warmup;\n      metricPrepare.value = warmup;\n      metricUploadMs.value = warmup;\n      metricEncode.value = warmup;\n      metricGpu.value = gpuProfilingSupported ? warmup : "unsupported";\n      return;\n    }\n\n    metricCpuFrame.value = formatPerfSummary(perfSamples.cpuFrame.summary());\n    metricRuntime.value = formatPerfSummary(perfSamples.runtime.summary());\n    metricPrepare.value = formatPerfSummary(perfSamples.prepare.summary());\n    metricUploadMs.value = formatPerfSummary(perfSamples.upload.summary());\n    metricEncode.value = formatPerfSummary(perfSamples.encode.summary());\n    metricGpu.value = formatGpuSummary();\n\n    status.dataset.profileSamples = String(perfSamples.cpuFrame.size);\n    status.dataset.cpuFrameP95Ms = String(perfSamples.cpuFrame.summary()?.p95 ?? "");\n    status.dataset.runtimeP95Ms = String(perfSamples.runtime.summary()?.p95 ?? "");\n    status.dataset.prepareP95Ms = String(perfSamples.prepare.summary()?.p95 ?? "");\n    status.dataset.uploadP95Ms = String(perfSamples.upload.summary()?.p95 ?? "");\n    status.dataset.encodeP95Ms = String(perfSamples.encode.summary()?.p95 ?? "");\n    status.dataset.gpuTimestampSupported = String(gpuProfilingSupported);\n    status.dataset.gpuP95Ms = String(\n      gpuProfilingSupported && Number.isFinite(player.gpuRenderP95Ms())\n        ? player.gpuRenderP95Ms()\n        : "",\n    );\n    status.dataset.gpuDroppedSamples = String(player.gpuDroppedSampleCount());\n    status.dataset.gpuFailedSamples = String(player.gpuFailedSampleCount());\n  }\n\n  async function loadExample''',
)
replace_once(
    "web/main.js",
    "      const result = await authoringClient.run(sceneSourceEditor.value);\n",
    '      const result = await authoringClient.run(\n        sceneSourceEditor.value,\n        currentExample("scene").context ?? {},\n      );\n',
)
replace_once(
    "web/main.js",
    "      authoredScene = stableDocument;\n\n      const preservedPlayhead = player.time();\n",
    "      authoredScene = stableDocument;\n      resetPerformanceProfile();\n\n      const preservedPlayhead = player.time();\n",
)
replace_once(
    "web/main.js",
    "      const presented = player.renderFrame(timestamp);\n      if (presented && timestamp - lastStatusUpdate > 200) {\n",
    "      const presented = player.renderFrame(timestamp);\n      if (presented) {\n        recordPerformanceFrame();\n      }\n      if (presented && timestamp - lastStatusUpdate > 200) {\n",
)
replace_once(
    "web/main.js",
    "        metricTime.value = `${playhead.toFixed(2)} s`;\n\n        status.dataset.instances",
    "        metricTime.value = `${playhead.toFixed(2)} s`;\n        updatePerformanceMetrics();\n\n        status.dataset.instances",
)

# Add a dedicated performance strip so the original compact runtime metrics stay readable.
replace_once(
    "web/index.html",
    '''      .metrics {\n        display: grid;\n        grid-template-columns: repeat(4, minmax(0, 1fr));\n        border-top: 1px solid var(--border);\n        background: rgb(10 13 21 / 86%);\n      }\n''',
    '''      .metrics,\n      .perf-metrics {\n        display: grid;\n        border-top: 1px solid var(--border);\n        background: rgb(10 13 21 / 86%);\n      }\n\n      .metrics {\n        grid-template-columns: repeat(4, minmax(0, 1fr));\n      }\n\n      .perf-metrics {\n        grid-template-columns: repeat(3, minmax(0, 1fr));\n        background: rgb(8 11 18 / 92%);\n      }\n''',
)
replace_once(
    "web/index.html",
    '''        .metrics {\n          grid-template-columns: repeat(2, 1fr);\n        }\n\n        .metric:nth-child(2) {\n''',
    '''        .metrics,\n        .perf-metrics {\n          grid-template-columns: repeat(2, 1fr);\n        }\n\n        .metric:nth-child(2) {\n''',
)
replace_once(
    "web/index.html",
    '''          <div class="metrics" aria-label="Runtime metrics">\n            <div class="metric">\n              <span class="metric-label">Objects</span>\n              <output id="metric-objects" class="metric-value">—</output>\n            </div>\n            <div class="metric">\n              <span class="metric-label">Draw calls</span>\n              <output id="metric-draws" class="metric-value">—</output>\n            </div>\n            <div class="metric">\n              <span class="metric-label">GPU upload</span>\n              <output id="metric-upload" class="metric-value">—</output>\n            </div>\n            <div class="metric">\n              <span class="metric-label">Playhead</span>\n              <output id="metric-time" class="metric-value">—</output>\n            </div>\n          </div>\n''',
    '''          <div class="metrics" aria-label="Runtime metrics">\n            <div class="metric">\n              <span class="metric-label">Objects</span>\n              <output id="metric-objects" class="metric-value">—</output>\n            </div>\n            <div class="metric">\n              <span class="metric-label">Draw calls</span>\n              <output id="metric-draws" class="metric-value">—</output>\n            </div>\n            <div class="metric">\n              <span class="metric-label">GPU upload</span>\n              <output id="metric-upload" class="metric-value">—</output>\n            </div>\n            <div class="metric">\n              <span class="metric-label">Playhead</span>\n              <output id="metric-time" class="metric-value">—</output>\n            </div>\n          </div>\n          <div class="perf-metrics" aria-label="Frame performance p50 and p95">\n            <div class="metric">\n              <span class="metric-label">CPU frame · p50 / p95</span>\n              <output id="metric-cpu-frame" class="metric-value">—</output>\n            </div>\n            <div class="metric">\n              <span class="metric-label">Runtime eval · p50 / p95</span>\n              <output id="metric-runtime" class="metric-value">—</output>\n            </div>\n            <div class="metric">\n              <span class="metric-label">Frame prepare · p50 / p95</span>\n              <output id="metric-prepare" class="metric-value">—</output>\n            </div>\n            <div class="metric">\n              <span class="metric-label">GPU upload CPU · p50 / p95</span>\n              <output id="metric-upload-ms" class="metric-value">—</output>\n            </div>\n            <div class="metric">\n              <span class="metric-label">Encode + submit · p50 / p95</span>\n              <output id="metric-encode" class="metric-value">—</output>\n            </div>\n            <div class="metric">\n              <span class="metric-label">GPU render · p50 / p95</span>\n              <output id="metric-gpu" class="metric-value">—</output>\n            </div>\n          </div>\n''',
)

# Ensure every shipped Python example at least passes the parser in CI.
replace_once(
    "scripts/build-web-demo.sh",
    "node --test web/frame-metrics.test.mjs\nPYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s web/python -p 'test_*.py'\n",
    "node --test web/frame-metrics.test.mjs\nPYTHONDONTWRITEBYTECODE=1 python3 -m compileall -q web/python/examples\nPYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s web/python -p 'test_*.py'\n",
)
