import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { diagnosticForegroundPixels, rendererProfileHarness, diagnosticSource, distribution, summarizeProfile } from "./showcase-triangle-light-profile.mjs";
const source = await readFile(new URL("../web/python/examples/showcase_triangle_light.py",import.meta.url),"utf8");
const timed = text => text.split("\n").filter(line=>/self\.(play|wait)|run_time=|FLIGHT_SECONDS|BOUNCES =/.test(line));

test("a bright border cannot qualify a blank scene, but an interior dot can", () => {
  const image = { width: 8, height: 8, data: new Uint8Array(8 * 8 * 4) };
  for (let y = 0; y < 8; y++) for (let x = 0; x < 8; x++) {
    const offset = (y * 8 + x) * 4;
    const edge = x < 2 || y < 2 || x >= 6 || y >= 6;
    image.data.set(edge ? [255, 255, 255, 255] : [13, 18, 32, 255], offset);
  }
  assert.equal(diagnosticForegroundPixels(image), 0);
  image.data.set([255, 255, 255, 255], (3 * 8 + 3) * 4);
  assert.equal(diagnosticForegroundPixels(image), 1);
  assert.throws(() => diagnosticForegroundPixels({ ...image, data: image.data.subarray(4) }));
});

test("baseline and repeat use the exact source, not an optimized replacement",()=>{
  assert.equal(diagnosticSource(source),source);
  assert.equal(diagnosticSource(source,{glow:true,laboratory:true}),source);
});
test("glow ablation leaves geometry, clock and opacity animations unchanged",()=>{
  const variant=diagnosticSource(source,{glow:false});
  assert.equal((variant.match(/diagnostic: disable only Gaussian glow/g)??[]).length,4);
  assert.doesNotMatch(variant,/(photon|halo|arc)\.set_glow/);
  assert.deepEqual(timed(variant),timed(source));
  for (const name of ["Create(trail)","Create(halo)","Create(core)","photon.animate.move_to"])
    assert.ok(variant.includes(name));
});
test("laboratory ablation preserves source construction and later pose targets",()=>{
  const variant=diagnosticSource(source,{laboratory:false});
  assert.ok(variant.includes("room, arms, mouth, clock_hands = laboratory_artwork()"));
  assert.ok(variant.includes("*arms, *clock_hands]"));
  assert.deepEqual(timed(variant),timed(source));
});
test("unknown source layouts cannot silently produce the wrong experiment",()=>{
  assert.throws(()=>diagnosticSource(source.replace("photon.set_glow(color=color)","photon.remove_glow()"),{glow:false}));
  assert.throws(()=>diagnosticSource(source.replace("room, arms, mouth, clock_hands = laboratory_artwork()","room = []"),{laboratory:false}));
});
test("unavailable stage data is not reported as zero",()=>{
  assert.deepEqual(distribution([3,1,2]),{samples:3,median:2,p95:3,max:3,mean:2});
  for(const values of [[],[undefined],[NaN],[-1]]) assert.throws(()=>distribution(values));
  assert.throws(()=>summarizeProfile({schemaVersion:2}));
});

test("renderer measurement preserves each engine's verified ready adapter", () => {
  const current = 'import { PythonAuthoringClient } from "./authoring-client.js";';
  const legacy = 'import { ProvenancedPythonAuthoringClient } from "./provenanced-authoring-client.js";';
  const timing = '\nconst frames = 90; const client = new PythonAuthoringClient();';
  assert.equal(rendererProfileHarness(current + timing, current), current + timing);
  assert.equal(rendererProfileHarness(current + timing, legacy),
    'import { ProvenancedPythonAuthoringClient as PythonAuthoringClient } from "./provenanced-authoring-client.js";' + timing);
  assert.throws(() => rendererProfileHarness(current + timing, 'unknown'));
});
