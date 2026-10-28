import assert from "node:assert/strict";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";
import { filteredHatch, hatchFootprint, localFromPixel } from "./hatch-paint-reference.mjs";

const identity = { scale: [1, 1], angle: 0, translation: [0, 0] };
const first = { spacing: 0.6, width: 0.09, angle: 0.2, phase: [-0.173, 0.231], cross: true };
const second = { spacing: 0.35, width: 0.12, angle: 0.85, phase: [0.531, -0.719], cross: true };
const profiles = [
  { ...first, cross: false }, first,
  { ...first, spacing: 0.035, width: 0.011, angle: 0.7 },
  { ...first, spacing: 0.0001, width: 0.000035, angle: 0.7 },
  { ...first, width: 0, angle: 0.7, phase: [0, 0] },
  { ...first, width: 0.6, angle: 0.7, phase: [0, 0] },
  { ...first, phase: [4294967296.173, -4294967296.231] },
  { ...first, phase: [-1e-20, 0] },
  { ...second, pose: { scale: [-0.8, 1.2], angle: 0.3, translation: [0.2, -0.1] } },
];

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_HATCH_ARTIFACTS ?? "browser-smoke-artifacts/hatch-paint-authoring",
  cases: [...profiles.map((profile, index) => ({
    id: `hatch-profile-${index}`, profile, factory: "createDirectHatchPaintProfileSmokeRenderer", factoryArgs: [index],
    objectCount: 1, duration: 0, sampleTime: 0, canvasSize: [960, 540],
    source: `from noon import Color, HatchPaint, Rectangle, Scene
class HatchedSurface(Scene):
    def construct(self):
        panel = Rectangle(width=4, height=2, stroke=None)
        panel.set_fill(HatchPaint(Color(1,0,0,0.5), Color(0,0,1), spacing=${profile.spacing}, width=${profile.width}, angle=${profile.angle}, phase=(${profile.phase}), cross=${profile.cross ? "True" : "False"}), opacity=0.7)
        panel.set_object_opacity(0.6)
${profile.pose ? "        panel.scale((-0.8,1.2,1)).rotate(0.3).shift((0.2,-0.1,0))\n" : ""}        self.add(panel)
`,
  })), ...[0.25, 0.75, 1, 1.25, 1.5, 2.25, 2.75, 3.5, 4.25].map(sampleTime => ({
    id: `hatch-paint-${sampleTime}`, file: "hatch_paint.py", factory: "createDirectHatchPaintSmokeRenderer",
    objectCount: 1, duration: 4.25, sampleTime, playback: "live", canvasSize: [960, 540],
    directHeldSampleTime: sampleTime === 0.25 ? 0 : undefined,
    boundaries: [0.5, 1.5, 1.75, 2.75, 3, 4], stopAtSourceCompletion: sampleTime === 4.25,
  }))],
  prepareContext: disableAuthoringJspi,
  qualifyPixels({ fixture, rust, python }) {
    let options, foreground, background, opacity;
    const time = fixture.sampleTime, profile = fixture.profile, pose = profile?.pose ?? identity;
    if (profile) {
      options = profile; foreground = [0.5, 0, 0]; background = [0, 0, 1]; opacity = 0.42;
    } else if (time <= 1.5) {
      const a = Math.max(0, time - 0.5);
      options = { spacing: 0.6 - 0.25 * a, width: 0.09 + 0.03 * a,
        angle: 0.2 + 0.65 * a, phase: [-0.173 + 0.704 * a, 0.231 - 0.95 * a], cross: true };
      foreground = [0.5 * (1 - a), a, 0]; background = [0, 0, 1 - a]; opacity = 1 - a / 2;
    } else if (time < 3) {
      const a = Math.max(0, Math.min(1, time - 1.75)); options = second;
      foreground = [a, 1 - a, 0]; background = [a, 0, 0]; opacity = 0.5 + a / 2;
    } else {
      const a = Math.min(1, time - 3); options = first;
      foreground = [1 - a / 2, 0, 0]; background = [1 - a, 0, a]; opacity = 1;
    }
    let comparisons = 0, maximumByteError = 0, filteredEdges = 0;
    for (const image of [rust, python]) {
      const pixelScale = image.height / 8, footprint = hatchFootprint(options, pose, pixelScale);
      for (let x = Math.floor(image.width / 2 - pixelScale * 2); x <= image.width / 2 + pixelScale * 2; x += 11) {
        for (let y = Math.floor(image.height / 2 - pixelScale * 1.4); y <= image.height / 2 + pixelScale * 1.4; y += 9) {
          const local = localFromPixel(x, y, image.width, image.height, pose);
          if (Math.abs(local[0]) > 1.8 || Math.abs(local[1]) > 0.8) continue;
          const coverage = filteredHatch(local, options, footprint);
          const expected = [...background.map((c, i) => Math.floor(Math.floor(
            (c * (1 - coverage) + foreground[i] * coverage) * opacity * 65535 + 0.5,
          ) / 256)), 255];
          const offset = (y * image.width + x) * 4, actual = image.data.subarray(offset, offset + 4);
          const error = Math.max(...expected.map((v, i) => Math.abs(v - actual[i])));
          maximumByteError = Math.max(maximumByteError, error);
          assert.ok(error <= 1, `${fixture.id} (${x},${y}) ${[...actual]} != ${expected}; coverage=${coverage}; footprint=${footprint}`);
          if (coverage > 0.001 && coverage < 0.999) filteredEdges++;
          comparisons++;
        }
      }
    }
    assert.ok(comparisons > 100, `${fixture.id} must exercise a substantial independent interior grid`);
    return { interiorPixelComparisons: comparisons, maximumByteError, filteredEdges,
      reference: "independent periodic interval enumeration; physical-pixel hatch-space AABB; premultiplied encoded LDR" };
  },
});
