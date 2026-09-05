import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const host = await readFile(
  new URL("../crates/noon-web/src/execution_canvas.rs", import.meta.url),
  "utf8",
);
const timestamps = await readFile(
  new URL("../crates/noon-web/src/gpu_timestamps.rs", import.meta.url),
  "utf8",
);
const profile = await readFile(new URL("./perf-profile.js", import.meta.url), "utf8");
const runner = await readFile(new URL("../scripts/perf-profile.mjs", import.meta.url), "utf8");

for (const required of [
  "adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY)",
  "wgpu::Features::TIMESTAMP_QUERY",
  "enableGpuTimestampProfiling",
  "resetGpuTimestampMetrics",
  "takeGpuTimestampJson",
]) {
  assert.ok(host.includes(required), `browser timestamp host must contain ${required}`);
}
for (const required of [
  "GpuTimestampProfiler",
  "READBACK_SLOTS: usize = 4",
  "encoder.resolve_query_set(",
  "encoder.copy_buffer_to_buffer(",
  "readback_buffer.map_async(wgpu::MapMode::Read",
  "QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC",
  "MAP_READ | wgpu::BufferUsages::COPY_DST",
]) {
  assert.ok(timestamps.includes(required), `timestamp diagnostics must contain ${required}`);
}
assert.ok(
  host.includes("self.renderer.encode_profiled("),
  "browser host must encode through the reusable profiled renderer path",
);
assert.equal(
  timestamps.includes("device.poll("),
  false,
  "browser timestamp collection must not synchronously poll normal presentation frames",
);

for (const required of [
  "takeGpuTimestampMetrics",
  "settleGpuTimestampMetrics",
  "resetGpuTimestampMetrics",
  "renderPassMs: windows.gpuRenderPassMs.summary()",
]) {
  assert.ok(profile.includes(required), `performance report must contain ${required}`);
}
assert.ok(
  runner.includes("--enable-dawn-features=allow_unsafe_apis"),
  "required timestamp qualification must enable Chromium's optional timestamp API",
);
