// Independent raster assertions for the fixed shared-Rust signal-train fixture.
// These are test observations, never scene construction or animation evaluation.
import assert from "node:assert/strict";

export const signalTrainTimes = Object.freeze([0, 0.65, 0.9, 1.5, 2, 3.55, 4.15, 4.2]);
const expectedSignals = [0, 1, 2, 5, 8, 1, 0, 0];

export function inspectSignalSample(sample, snapshot, index, backend) {
  assert.ok(Number.isInteger(index) && index >= 0 && index < signalTrainTimes.length);
  const time = signalTrainTimes[index];
  assert.equal(sample?.type, "sample_to_authored_time");
  assert.equal(sample.time, time, "acknowledged authored sample time");
  assert.equal(sample.sourceCompleted, index === signalTrainTimes.length - 1, "source completion boundary");
  assert.equal(snapshot.executionMode, "semantic");
  const metrics = snapshot.metrics;
  assert.equal(metrics?.ready, true);
  assert.equal(metrics.retained, true);
  assert.equal(metrics.backend, backend === "webgpu" ? "WebGPU" : "WebGL2");
  assert.ok(metrics.presentedFrames > 0, "sample must have a presented publication");
  const renderTime = Number(metrics.time);
  assert.ok(Number.isFinite(renderTime) && renderTime >= 0 && renderTime <= time,
    "presented time cannot be invalid or newer than the acknowledged sample");
  // The sample response awaits the required publication, including the last
  // one when no new delta exists. Static holds need no redundant GPU redraw.
  // Active pulses still require a freshly presented frame at the exact time.
  if (expectedSignals[index] > 0) assert.equal(renderTime, time, "active pulse presentation time");
  return { time, renderTime, sourceCompleted: sample.sourceCompleted };
}

export function inspectSignalTrain(image, index) {
  assert.ok(Number.isInteger(index) && index >= 0 && index < signalTrainTimes.length);
  assert.equal(image.width, 640);
  assert.equal(image.height, 360);
  assert.equal(image.data.length, image.width * image.height * 4);
  const columns = [new Set(), new Set()];
  const connection = [new Set(), new Set()];
  // Camera height 8, framebuffer height 360: 45 px per scene unit. The two
  // x=[-3,3] rails occupy columns [185,455), centered at y=135 and y=225.
  const centers = [135, 225];
  for (let i = 0; i < image.data.length; i += 4) {
    const [r, g, b, a] = image.data.subarray(i, i + 4);
    assert.equal(a, 255);
    const x = (i / 4) % image.width;
    const y = Math.floor(i / 4 / image.width);
    const pulse = g > 128 && b > 128 && r < 64 ? 0
      : r > 128 && g > 100 && b < 64 ? 1 : -1;
    if (pulse !== -1) {
      assert.ok(Math.abs(y - centers[pulse]) <= 3, `pulse left rail ${pulse}`);
      assert.ok(x >= 185 && x < 455, `pulse left connection at x=${x}`);
      columns[pulse].add(x);
    }
    // Pulse-edge antialiasing blends colored ink with the dim rail; occupancy
    // must accept that blend without counting it as another bright pulse.
    if (Math.max(r, g, b) > 20) {
      for (let rail = 0; rail < centers.length; rail += 1) {
        if (Math.abs(y - centers[rail]) <= 1 && x >= 185 && x < 455) {
          connection[rail].add(x);
        }
      }
    }
  }
  return columns.map((occupied, rail) => {
    // Equality of two black frames is not proof of a preserved connection.
    // Every column must retain rail ink, or a pulse covering that same rail.
    for (let x = 185; x < 455; x += 1) {
      assert.ok(connection[rail].has(x), `missing connection ${rail} at x=${x}, t=${signalTrainTimes[index]}`);
    }
    const xs = [...occupied].sort((a, b) => a - b);
    const count = xs.filter((x, i) => i === 0 || x > xs[i - 1] + 1).length;
    assert.equal(count, expectedSignals[index], `rail ${rail} signal count at t=${signalTrainTimes[index]}`);
    return { count, min: xs.at(0) ?? null, max: xs.at(-1) ?? null };
  });
}
