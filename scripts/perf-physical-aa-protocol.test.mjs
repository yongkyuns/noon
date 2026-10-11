import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile, mkdtemp, rm } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  validatePlan, studySchedule, pairEnvironment, authoredSourceFingerprint,
  validatePhysicalProof, strictLatencyFailures, runRecorded, exclusiveDirectory,
} from "./perf-physical-aa-product.mjs";
import { PHYSICAL_HARNESS, gitBlobSha, physicalPairSource } from "./perf-physical-aa-overlay.mjs";
import { deriveAAHostDriver, STUDY_ID } from "./perf-physical-aa-host-derive.mjs";

const plan=JSON.parse(await readFile(new URL("../benchmarks/perf-physical-aa-study.json",import.meta.url)));
const clone=x=>structuredClone(x);
const gpu="ANGLE (Apple, ANGLE Metal Renderer: Apple M4 Pro, Unspecified Version)";
function readyProof(){return {pairIndex:1,exampleId:plan.workloads[0],backend:"WebGL2",
  unmaskedVendor:"Google Inc. (Apple)",unmaskedRenderer:gpu,
  browserVersion:"151.0.7922.34",platform:"darwin",arch:"arm64",
  clearPixel:[51,102,153,255],trianglePixel:[153,51,102,255],
  shaderLinked:true,glError:0,contextLost:false,
  diagnosticOnly:true,qualification:false,performanceAcceptance:false,mergeApproval:false};}

test("the frozen physical study is complete, signed to original two release identities and never approves a merge",()=>{
  validatePlan(plan);
  assert.equal(plan.studyId,"1933-physical-aa-20261010-01");
  assert.equal(plan.producerRun,38078042785);
  assert.equal(plan.builds.length,2);
  assert.equal(plan.harnessOverlay.physicalPairBlob,PHYSICAL_HARNESS.physicalPairBlob);
  assert.equal(plan.qualification,false);
  assert.equal(plan.mergeApproval,false);
  assert.equal(plan.execution.event,"workflow_dispatch");
});

for(const [label,edit] of [
  ["candidate source",p=>p.builds[1].source="a".repeat(40)],
  ["candidate release ZIP",p=>p.builds[1].zipSha256="a".repeat(64)],
  ["baseline release source",p=>p.sourcePair.baseline="c".repeat(40)],
  ["candidate binary ID",p=>p.builds[1].buildId="0".repeat(64)],
  ["original reference sample",p=>p.authoredSourceHashes[plan.workloads[2]]="0".repeat(64)],
  ["sample window count",p=>p.cohorts[0].pairs=6],
  ["extra attempt",p=>p.retries=1],
  ["software browser",p=>p.environment.browserMode="software-WebGL"],
  ["host different scorer",p=>p.host.originalDriverBlob="0".repeat(40)],
  ["producer bypass",p=>p.harnessOverlay.physicalPairBlob="0".repeat(40)],
  ["modified manual-only event",p=>p.execution.event="pull_request"],
  ["changed one-shot policy",p=>p.execution.allowedRunAttempt=2],
  ["acceptance mislabeled",p=>p.performanceAcceptance=true],
]){
  test(`rejects ${label}`,()=>{const p=clone(plan);edit(p);assert.throws(()=>validatePlan(p));});
}

test("100 preregistered product commands contain 200 real immutable package trials in alternating order",()=>{
  const schedule=studySchedule(plan);
  assert.equal(schedule.length,100);
  assert.equal(new Set(schedule.map(x=>[x.cohort,x.build,x.workload,x.pair].join("/"))).size,100);
  assert.deepEqual(schedule.map(x=>x.ordinal),Array.from({length:100},(_,i)=>i+1));
  for(const run of schedule) {
    assert.deepEqual(run.logicalOrder,run.pair%2?["baseline","candidate"]:["candidate","baseline"]);
    const root="/original/"+run.build;
    const env=pairEnvironment(plan,run,root,"/evidence",{
      PATH:"/usr/bin",NOON_PRODUCT_MIN_FPS_RATIO:"0.1",
      NOON_PRODUCT_REFERENCE_ROOT:"/forged",NOON_PERF_RERUN:"1",
    });
    assert.equal(env.NOON_PRODUCT_REFERENCE_ROOT,root);
    assert.equal(env.NOON_PRODUCT_CANDIDATE_ROOT,root,"A/A MUST compare one original package against itself");
    assert.equal(env.NOON_PRODUCT_REFERENCE_ARTIFACT_ROLE,"baseline");
    assert.equal(env.NOON_PRODUCT_STRICT_MIN_FPS_RATIO,"0.97");
    assert.equal(env.NOON_PRODUCT_MIN_FPS_RATIO,"0.80");
    assert.equal(env.NOON_PERF_RERUN,undefined);
  }
});

test("original square/circle source modification is frozen and not inferred from changed inputs",()=>{
  const raw="self.play(Create(circle), Create(square))";
  const window={windowEndSeconds:4,endpointHoldSeconds:0.5};
  const expected=raw.replace(raw,`self.play(Create(circle), Create(square), run_time=${window.windowEndSeconds})\n        self.wait(${window.endpointHoldSeconds})`);
  assert.equal(authoredSourceFingerprint("parity-square-and-circle",window,raw),
    createHash("sha256").update(expected).digest("hex"));
  assert.throws(()=>authoredSourceFingerprint("parity-square-and-circle",window,"self.wait(1)"));
});

test("physical proof demands actual matching Apple Metal hardware and correct GLSL/readback",()=>{
  const old=process.env.NOON_PHYSICAL_AA_EXPECTED_RENDERER;
  process.env.NOON_PHYSICAL_AA_EXPECTED_RENDERER=gpu;
  try {
    const run={pair:1,workload:plan.workloads[0]},ok=readyProof();
    assert.deepEqual(validatePhysicalProof(plan,run,ok),ok);
    for(const change of [
      x=>x.unmaskedRenderer="ANGLE (Apple, ANGLE Metal Renderer: Apple Paravirtual device)",
      x=>x.unmaskedRenderer="SwiftShader",x=>x.unmaskedVendor="software",
      x=>x.trianglePixel=[0,0,0,0],x=>x.clearPixel=[0,0,0,0],
      x=>x.glError=1280,x=>x.contextLost=true,x=>x.shaderLinked=false,
      x=>x.exampleId=plan.workloads[1],x=>x.pairIndex=2,
      x=>x.browserVersion="0",x=>x.backend="WebGPU",x=>x.mergeApproval=true,
    ]) {
      const changed=clone(ok);change(changed);
      assert.throws(()=>validatePhysicalProof(plan,run,changed));
    }
  }finally{
    if(old===undefined)delete process.env.NOON_PHYSICAL_AA_EXPECTED_RENDERER;
    else process.env.NOON_PHYSICAL_AA_EXPECTED_RENDERER=old;
  }
});

test("strict original 1.03× + 20ms latency adjunct cannot be relaxed",()=>{
  const names=["shell ready","cold Run → applied","warm Run → applied","edit → applied"];
  const latency=Object.fromEntries(names.map(n=>[n,{baselineMs:100,candidateMs:123}]));
  assert.deepEqual(strictLatencyFailures({latency}),[]);
  latency[names[1]].candidateMs=123.001;
  assert.equal(strictLatencyFailures({latency}).length,1);
});

test("only the exact original headed pair script is allowed to change its GPU identity proof",async()=>{
  const original=await readFile("frozen-harness/scripts/playground-product-pair.mjs","utf8");
  assert.equal(gitBlobSha(original),PHYSICAL_HARNESS.originalPairBlob);
  const adapted=physicalPairSource(original);
  assert.equal(gitBlobSha(adapted),PHYSICAL_HARNESS.physicalPairBlob);
  assert.match(adapted,/NOON_PHYSICAL_AA_EXPECTED_RENDERER/);
  assert.match(adapted,/paravirtual\|virtual device/);
  assert.match(adapted,/headless: false/);
  assert.match(adapted,/--disable-features=WebGPU,WebGPUService,WebGPUBlobCache/);
  assert.throws(()=>physicalPairSource(original+"\n"));
});

test("derived host acquisition retains unchanged 63 warmed original pairs and the fixed scorer",async()=>{
  const original=await readFile("frozen-harness/scripts/python-host-perf.mjs","utf8");
  assert.equal(gitBlobSha(original),"30d766b4f9c0b87d9994adb707c47d5dbb84fe4c");
  const driver=deriveAAHostDriver(original);
  assert.equal(STUDY_ID,"1933-physical-host-aa-20261010-01");
  assert.match(driver,/PHYSICAL_AA_BACKEND_PROOF/);
  assert.match(driver,/NOON_PHYSICAL_AA_EXPECTED_RENDERER/);
  assert.match(driver,/paravirtual\|virtual device/);
  assert.match(driver,/const role = "baseline"/);
  assert.match(driver,/scoredPairSchedule\(pair \+ 1\)/);
  assert.match(driver,/pairedCost\(pairs.map/);
  assert.match(driver,/fresh-per-pair-warmed/);
  assert.match(driver,/qualification: false/);
  assert.doesNotMatch(driver,/await qualifyProductCohorts\(/);
  const check=execFileSync(process.execPath,["--input-type=module","--check"],{input:driver,encoding:"utf8"});
  assert.equal(check,"");
});

test("manual physical workflow cannot schedule an A/A from a pull request or hosted runner",async()=>{
  const s=await readFile(new URL("../.github/workflows/perf-physical-aa.yml",import.meta.url),"utf8");
  assert.match(s,/workflow_dispatch:/);
  assert.match(s,/github.event_name == 'workflow_dispatch'/);
  assert.match(s,/github.ref == 'refs\/heads\/master'/);
  assert.match(s,/github.run_attempt == 1/);
  assert.match(s,/RUN_PHYSICAL_AA_ONCE/);
  assert.match(s,/runs-on: \[self-hosted, macOS, ARM64, noon-physical\]/);
  assert.match(s,/if: always\(\)/);
  assert.match(s,/node control\/scripts\/perf-aa-precision-audit.mjs/);
  assert.match(s,/node control\/scripts\/perf-physical-aa-product.mjs/);
  assert.match(s,/baseline-peer\/browser-smoke-artifacts\/product-gate\/host-cost\/comparison.json/);
  assert.doesNotMatch(s,/cargo build|wasm-pack build|rerun-failed-jobs|cancel-in-progress: true/);
});

test("exclusive original evidence cannot be overwritten and no pair is silently retried",async t=>{
  const root=await mkdtemp(path.join(os.tmpdir(),"physical-aa-protocol-"));
  t.after(()=>rm(root,{recursive:true,force:true}));
  await exclusiveDirectory(path.join(root,"evidence"));
  await assert.rejects(()=>exclusiveDirectory(path.join(root,"evidence")),{code:"EEXIST"});
  const log=path.join(root,"failure.log");
  await assert.rejects(()=>runRecorded(process.execPath,["-e","process.exit(3)"],
    {cwd:root,env:process.env,log}),e=>e.result.code===3);
  assert.equal((await readFile(log,"utf8")).length,0);
});
