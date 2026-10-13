import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { diagnosticSource, distribution, summarizeProfile } from "./showcase-triangle-light-profile.mjs";
const source = await readFile(new URL("../web/python/examples/showcase_triangle_light.py",import.meta.url),"utf8");
const timed = text => text.split("\n").filter(line=>/self\.(play|wait)|run_time=|FLIGHT_SECONDS|BOUNCES =/.test(line));

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
