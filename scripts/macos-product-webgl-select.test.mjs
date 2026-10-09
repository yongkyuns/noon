import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { STUDY_ID, FIXED_CASES, validatePlan, flagsFor, classifyCase } from "./macos-product-webgl-select.mjs";
const plan=JSON.parse(await readFile(new URL("../benchmarks/macos-product-webgl-select.json", import.meta.url)));

test("one immutable four-case cold-playback study, no score or benchmark", () => {
  assert.equal(validatePlan(plan),plan);
  assert.equal(plan.studyId,STUDY_ID);
  assert.deepEqual(plan.cases,FIXED_CASES);
  assert.equal(plan.noScoring,true);
  assert.equal(plan.source.baselineArtifact,11523896652);
  assert.equal(plan.priorFailedProduct.completedPairs,0);
  for(const edit of [
    p=>p.cases.reverse(),p=>p.cases.pop(),
    p=>p.node="23.0",p=>p.chromium="152.0",
    p=>p.source.baselineSource="f".repeat(40),
    p=>p.qualification=true,p=>p.performanceAcceptance=true,
    p=>p.noScoring=false,p=>p.runner="macos-15-xlarge",
  ]) {const mutated=structuredClone(plan);edit(mutated);assert.throws(()=>validatePlan(mutated));}
});

test("every fixed browser feature case changes exactly the planned WebGPU feature switch", () => {
  const original=["--disable-features=WebGPU","--use-gpu-in-tests","--ignore-gpu-blocklist"];
  for(const c of FIXED_CASES) {
    const actual=flagsFor(c,original);
    assert.equal(actual[0],"--disable-features="+c.disableFeatures);
    assert.deepEqual(actual.slice(1,3),original.slice(1,3));
    assert.deepEqual(actual.slice(3),c.addArgs);
  }
  assert.throws(()=>flagsFor(FIXED_CASES[0],["--disable-gpu"]));
  assert.throws(()=>flagsFor(FIXED_CASES[0],["--disable-features=WebGPU","--disable-features=WebGPU"]));
});

test("real original Noon playback WebGL2 with paravirtual Apple readback is diagnostic only", () => {
  const correct=()=>({browserVersion:"151.0.7922.34",status:{patch:"applied",backend:"WebGL2"},
    gpu:{unmaskedRenderer:"ANGLE (Apple, ANGLE Metal Renderer: Apple Paravirtual device)",
      pixel:[51,102,153,255],glError:0,contextLost:false},pageErrors:[]});
  assert.equal(classifyCase(correct()).status,"eligible-for-fresh-aa-only");
  for(const edit of [
    p=>p.status.backend="WebGPU",
    p=>p.status.patch="error",
    p=>p.gpu.unmaskedRenderer="ANGLE (Google SwiftShader)",
    p=>p.gpu.unmaskedRenderer="ANGLE (Mesa llvmpipe)",
    p=>p.gpu.pixel=[0,0,0,0],
    p=>p.gpu.contextLost=true,
    p=>p.gpu.glError=1280,
    p=>p.pageErrors.push("Uncaught"),
    p=>p.browserVersion="150",
  ]) {const x=correct();edit(x);assert.equal(classifyCase(x).status,"not-confirmed");}
});

test("workflow is a single initial PR-opened event with exact four-case real playback and no build retries", async () => {
  const w=await readFile(new URL("../.github/workflows/macos-product-webgl-select.yml",import.meta.url),"utf8");
  assert.match(w,/types: \[opened\]/);
  assert.match(w,/github\.run_attempt == 1/);
  assert.match(w,/head\.ref == 'research\/1933-macos-product-webgl-select-20261009-01'/);
  assert.match(w,/runs-on: macos-15/);
  assert.match(w,/ref: \$\{\{ github\.event\.pull_request\.head\.sha \}\}/);
  assert.doesNotMatch(w,/macos-15-xlarge|xvfb-run|cargo build|wasm-pack build|workflow_dispatch/);
  assert.match(w,/node scripts\/macos-product-webgl-select\.mjs/);
});
