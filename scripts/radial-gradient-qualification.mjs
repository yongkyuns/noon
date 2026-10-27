// Real browser field/alpha/time pixels through direct Rust/WASM and Python.
import assert from "node:assert/strict";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_RADIAL_ARTIFACTS ?? "ci-artifacts/radial-gradient",
  cases: [0.25, 1, 1.5, 2.25, 2.75, 3.5, 4.25].map(sampleTime => ({
    id: `radial-gradient-${sampleTime}`, file: "radial_gradient.py",
    factory: "createDirectRadialGradientSmokeRenderer", objectCount: 1,
    duration: 4.25, sampleTime, playback: "live",
    directHeldSampleTime: sampleTime === 0.25 ? 0 : undefined,
    boundaries: [0.5, 1.5, 1.75, 2.75, 3, 4],
    stopAtSourceCompletion: sampleTime === 4.25,
  })),
  prepareContext: disableAuthoringJspi,
  qualifyPixels({ fixture, rust, python }) {
    let checks = 0, maximumByteError = 0;
    const time = fixture.sampleTime;
    for (const image of [rust, python]) {
      for (const x of [400, 440, 480, 520, 560]) for (const y of [240, 270, 300]) {
        const scale = image.height / 8;
        const u = ((x + 0.5 - image.width / 2) / scale + 2) / 4;
        const v = ((image.height / 2 - y - 0.5) / scale + 1) / 2;
        let center, radii, first, last, opacity;
        if (time <= 1.5) {
          const a = Math.max(0, time - 0.5);
          center = [0.25 + 0.4 * a, 0.5 - 0.15 * a];
          radii = [0.5 + 0.25 * a, 0.75 - 0.35 * a];
          first = [0.5 * (1 - a), a, 0]; last = [0, 0, 1 - a];
          opacity = 1 - a / 2;
        } else if (time < 3) {
          const a = Math.min(1, Math.max(0, time - 1.75));
          center = [0.65, 0.35]; radii = [0.75, 0.4];
          first = [a, 1 - a, 0]; last = [a, 0, 0];
          opacity = 0.5 + a / 2;
        } else {
          const a = Math.min(1, time - 3);
          center = [0.25, 0.5]; radii = [0.5, 0.75];
          first = [1 - a / 2, 0, 0]; last = [1 - a, 0, a];
          opacity = 1;
        }
        const q = Math.min(1, Math.hypot((u - center[0]) / radii[0], (v - center[1]) / radii[1]));
        const rgb = first.map((c, i) => c * (1 - q) + last[i] * q);
        const expected = [...rgb.map(c => Math.floor(Math.floor(c * opacity * 65535 + 0.5) / 256)), 255];
        const offset = (y * image.width + x) * 4;
        const actual = image.data.subarray(offset, offset + 4);
        const error = Math.max(...expected.map((v, i) => Math.abs(v - actual[i])));
        maximumByteError = Math.max(maximumByteError, error);
        assert.ok(error <= 1, `${fixture.id} (${x},${y}): ${[...actual]} != ${expected}`);
        checks++;
      }
    }
    return { interiorPixelComparisons: checks, maximumByteError,
      reference: "independent elliptical distance and premultiplied temporal stop vectors; encoded LDR quantization" };
  },
});
