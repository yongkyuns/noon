import assert from "node:assert/strict";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";

await qualifyPairedAuthoring({
  cases: [...[0, 1.375, 3.125, 4.875, 6.5].map(sampleTime => ({
    id: `group-animation-${sampleTime}`, file: "showcase_group_animation.py",
    factory: "createDirectGroupAnimationSmokeRenderer", objectCount: sampleTime < 0.75 ? 6 : 16,
    duration: 6.5, sampleTime, playback: "live",
    boundaries: [0.75, 2, 2.5, 3.75, 4.25, 5.5],
    stopAtSourceCompletion: sampleTime === 6.5,
  })), ...[0, 1, 1.75, 2.75, 4].map(sampleTime => ({
    id: `mounted-callout-${sampleTime}`, file: "showcase_mounted_callout.py",
    factory: "createDirectMountedCalloutSmokeRenderer", objectCount: sampleTime < 0.5 ? 5 : 7,
    duration: 4, sampleTime, playback: "live",
    boundaries: [0.5, 1.5, 2, 3],
    directHeldSampleTime: sampleTime === 1.75 ? 1.5 : undefined,
    stopAtSourceCompletion: sampleTime === 4,
  }))],
  artifactDirectory: "browser-smoke-artifacts/group-animation",
  prepareContext: disableAuthoringJspi,
  qualifyPixels({ fixture, python: { data, width } }) {
    if (fixture.file !== "showcase_group_animation.py" || fixture.sampleTime === 0) return;
    let right = -1, top = Infinity, bottom = -1;
    for (let i = 0; i < data.length; i += 4) {
      if (data[i] < 230 || data[i + 1] < 190 || data[i + 2] >= 145) continue;
      const x = (i / 4) % width, y = Math.floor(i / 4 / width);
      right = Math.max(right, x);
      top = Math.min(top, y);
      bottom = Math.max(bottom, y);
    }
    assert.ok(right >= 0 && right + 7 < width, "expected the yellow tip of the moving underline");
    // The warm halo must travel outside the solid yellow tip at each sampled angle.
    const halo = (Math.round((top + bottom) / 2) * width + right + 7) * 4;
    const [red, green, blue] = data.subarray(halo, halo + 3);
    assert.ok(red >= 24 && red > green && green > blue, `${fixture.id}: glow must follow the tip`);
    return { haloRgb: [red, green, blue] };
  },
});
