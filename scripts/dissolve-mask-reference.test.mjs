import assert from "node:assert/strict";
import test from "node:test";
import { dissolveCoverage } from "./dissolve-mask-reference.mjs";

test("seeded field matches independently calculated integer-hash vectors", () => {
  // Python unbounded-integer hashing and scalar arithmetic generated these
  // fixed lattice and interpolated vectors independently of JS and WGSL.
  for (const [seed,x,y,softness,progress,expected] of [
    [7,0,0,0.8,0.5,0.6484599611514605],
    [7,0.083,-0.031,0.4,0.375,1],
    [4294967295,-0.125,0.3125,0.8,0.5,0.9822489803471367],
    [0,0.333,0.777,0.8,0.5,0.9257363574863432],
  ]) assert.ok(Math.abs(dissolveCoverage(x,y,{seed,softness,progress})-expected)<1e-14);
});
test("empty/full endpoints and hard-edge membership are exact", () => {
  for (const seed of [0,7,4294967295]) for (const [x,y] of [[0,0],[-0.125,0.3125],[0.333,0.777]]) {
    for (const softness of [0,0.2,1]) {
      assert.equal(dissolveCoverage(x,y,{seed,softness,progress:0}),1);
      assert.equal(dissolveCoverage(x,y,{seed,softness,progress:1}),0);
    }
    assert.ok([0,1].includes(dissolveCoverage(x,y,{seed,softness:0,progress:0.5})));
  }
});
