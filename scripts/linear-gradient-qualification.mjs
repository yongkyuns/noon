// Real browser pixels through both direct Rust/WASM and the Python worker.
import assert from "node:assert/strict";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_GRADIENT_ARTIFACTS ?? "ci-artifacts/linear-gradient",
  cases: [false, true].flatMap(glow =>
    [0.25, 1, 1.5, 2.25, 2.75, 3.5, ...(glow ? [4.2, 4.5] : [4.25])].map(sampleTime => ({
    id: `linear-gradient-${glow ? "glow-" : ""}${sampleTime}`,
    file: glow ? "gradient_glow.py" : "linear_gradient.py", glow,
    factory: glow ? "createDirectGradientGlowSmokeRenderer" : "createDirectLinearGradientSmokeRenderer", objectCount: 1,
    duration: glow ? 4.5 : 4.25, sampleTime, playback: "live",
    // Quiet waits retain their last picture and timer deadline in the direct host.
    directHeldSampleTime: sampleTime === 0.25 ? 0 : undefined,
    boundaries: [0.5, 1.5, 1.75, 2.75, 3, 4, ...(glow ? [4.25] : [])],
    stopAtSourceCompletion: sampleTime === (glow ? 4.5 : 4.25),
  }))),
  prepareContext: disableAuthoringJspi,
  qualifyPixels({fixture, rust, python}) {
    const time = fixture.sampleTime;
    let checks = 0;
    for (const image of [rust, python]) {
      for (const x of [400, 440, 480, 520, 560]) for (const y of [240, 270, 300]) {
        const scale = image.height / 8;
        const u = ((x + 0.5 - image.width / 2) / scale + 2) / 4;
        const v = ((image.height / 2 - y - 0.5) / scale + 1) / 2;
        let rgb, opacity;
        if (time <= 1.5) {
          const a = Math.max(0, time - 0.5);
          const t = Math.max(0, Math.min(1,
            ((u - a / 2) * (1 - a) + (v - (1 - a) / 2) * a) / ((1 - a) ** 2 + a ** 2)));
          rgb = [(1 - t) * (1 - a), (1 - t) * a, t]; opacity = 1 - a / 2;
        } else if (time < 3) {
          const a = Math.min(1, Math.max(0, time - 1.75));
          rgb = [a, (1 - a) * (1 - v), (1 - a) * v]; opacity = 0.5 + a / 2;
        } else {
          const a = Math.min(1, time - 3);
          rgb = [1 - a * u, 0, a * u]; opacity = 1;
        }
        if (fixture.glow) opacity = 1;
        const expected = [...rgb.map(c => Math.floor(Math.floor(c * opacity * 65535 + 0.5) / 256)), 255];
        const offset = (y * image.width + x) * 4;
        const actual = image.data.subarray(offset, offset + 4);
        assert.ok(expected.every((v, i) => Math.abs(v - actual[i]) <= 1),
          `${fixture.id} (${x},${y}): ${[...actual]} != ${expected}`);
        checks++;
      }
      if (fixture.glow) {
        const x = Math.ceil(image.width / 2 + 2 * image.height / 8) + 3;
        const y = Math.floor(image.height / 2);
        const at = (y * image.width + x) * 4;
        const halo = image.data.subarray(at, at + 3);
        if (time === 4.5) assert.deepEqual([...halo], [0, 0, 0], "removed glow left a halo");
        else assert.ok([...halo].some(c => c >= 3), "painted source lost its halo");
        checks++;
      }
    }
    return { interiorPixelComparisons: checks, maximumByteError: 1,
      reference: "independent temporal/local-box formula; Cairo LDR quantization on black; glow presence and exact removal" };
  },
});
