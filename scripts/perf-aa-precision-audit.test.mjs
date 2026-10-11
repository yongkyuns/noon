import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";
import { auditAA, originalInterval, compareOriginalMetric } from "./perf-aa-precision-audit.mjs";

const workloads = ["parity-square-and-circle", "showcase-camera-follows-path",
  "showcase-first-scene", "showcase-raster-images", "showcase-bezier-paths"];
const hostWorkloads = ["deterministic", "segments", "callbacks"];
const modes = ["async", "portable", "jspi"];
const metrics = ["creation_ms", "local_ms", "execution_ms"];
function scored(before, after, kind) {
  const s = originalInterval(before, after);
  const status = kind === "fps"
    ? s.upper < 0.97 ? "regression" : s.lower >= 0.97 ? "pass" : "inconclusive"
    : kind === "renderer"
      ? s.lower > 1.03 ? "regression"
        : s.ratio <= 1.03 && s.upper <= 1.05 ? "pass" : "inconclusive"
      : s.ratio > 1.03 ? "regression" : s.upper > 1.05 ? "inconclusive" : "pass";
  return {ratio:s.ratio,lower:s.lower,upper:s.upper,before,after,status};
}
const source = "a".repeat(40), binaryHash = "b".repeat(64);
function modelFixture() {
  const plan = {
    schema:1,studyId:"fixture-aa-v1",kind:"diagnostic-only",qualification:false,
    retries:0,stopOnTrialError:true,cohorts:[{name:"seven-pair",pairs:7},{name:"three-pair",pairs:3}],
    workloads,builds:[{name:"baseline",source,artifactId:1,zipSha256:binaryHash},
      {name:"candidate",source:"c".repeat(40),artifactId:2,zipSha256:binaryHash}],
    plannedPairCommands:100,plannedObservations:200,
    environment:{browserVersion:"151.0.7922.34",expectedUnmaskedRenderer:"Fixture GPU"},
  };
  const rawProductPlan=Buffer.from(JSON.stringify(plan)+"\n");
  const productSummary = {
    studyId:plan.studyId,diagnosticOnly:true,qualification:false,
    performanceAcceptance:false,mergeApproval:false,
    planSha256:createHash("sha256").update(rawProductPlan).digest("hex"),
    acquisitionComplete:true,status:"diagnostic-complete",plannedPairCommands:100,
    completedPairCommands:100,errors:[],cohorts:[],
  };
  const productJournal=[];
  for(const c of plan.cohorts)for(const workload of workloads){
    for(const build of ["baseline","candidate"]){
      const n=c.pairs;
      const fps=scored(Array(n).fill(60),Array(n).fill(60),"fps");
      const render=workload==="parity-square-and-circle" ? null :
        scored(Array(n).fill(0.5),Array(n).fill(0.5),"renderer");
      productSummary.cohorts.push({cohort:c.name,build,workload,pairs:n,
        comparator:{code:0,timedOut:false},metrics:{fps,render,status:"pass"},
        strictLatencyFailures:[],behaviorFailures:[],diagnosticClassification:"within-frozen-budgets"});
    }
    for(let pair=1;pair<=c.pairs;pair++){
      const order=pair%2?["baseline","candidate"]:["candidate","baseline"];
      for(const build of order){
        const event={ordinal:productJournal.length/2+1,cohort:c.name,pairs:c.pairs,
          workload,pair,build,logicalOrder:order,before:{load:[0.1],
            cpu:[{model:"Fixture CPU"},{model:"Fixture CPU"}]} };
        productJournal.push({...event,status:"started"});
        productJournal.push({...event,status:"recorded",command:{code:0,timedOut:false,elapsedMs:100},
          rendererProof:{backend:"WebGL2",diagnosticOnly:true,qualification:false,
            performanceAcceptance:false,mergeApproval:false,shaderLinked:true,
            contextLost:false,glError:0,browserVersion:plan.environment.browserVersion,
            pairIndex:pair,exampleId:workload,unmaskedRenderer:"Fixture GPU Renderer",
            clearPixel:[51,102,153,255],trianglePixel:[153,51,102,255]}});
      }
    }
  }
  const protocol={pairs:7,warmups:2,scoredWorkerWarmups:1,workerLifetime:"fresh-per-pair-warmed",
    objects:600,localEdits:2048,segments:32,callbacks:4,samples:60,sampleHz:48,
    modes,workloads:hostWorkloads,maxPointRatio:1.03,maxUpperRatio:1.05};
  const rowSet=[],warmups=[];
  for(const workload of hostWorkloads)for(const mode of modes){
    const pairs=[];
    for(let pair=1;pair<=7;pair++){
      for(const side of (pair%2?[0,1]:[1,0]))warmups.push({side,workload,mode,pair,
        warmup:1,workerLifetime:"fresh-per-pair-warmed",result:{mode,workload}});
      pairs.push(Array.from({length:2},()=>({mode,workload,callback_calls:workload==="callbacks"?8:0,
        center:[0,0,0],creation_ms:100,local_ms:100,execution_ms:100,
        metrics:{backend:"WebGL2",presentedFrames:10}})));
    }
    rowSet.push({mode,workload,sourceSha:"f".repeat(64),pairs,costs:Object.fromEntries(
      metrics.map(k=>[k,scored(Array(7).fill(100),Array(7).fill(100),"host")]))});
  }
  const identities=Array.from({length:2},()=>({schema:1,source,build:{profile:"release"},
    compiler:"fixed rustc",runtime:{wasmSha:binaryHash}}));
  const hostComparison={schema:1,studyId:"host-aa-v1",diagnosticOnly:true,qualification:false,
    performanceAcceptance:false,mergeApproval:false,identities,protocol,
    rows:rowSet,warmups,failures:[]};
  return {productPlan:plan,rawProductPlan,productSummary,productJournal,hostComparison};
}
function mutate(fn){const fixture=modelFixture();fn(fixture);return fixture;}

test("ideal, complete same-byte 100-product + 63-host A/A screens consistent but never grants merge approval",()=>{
  const result=auditAA(modelFixture());
  assert.equal(result.status,"original-aa-scores-consistent");
  assert.equal(result.qualification,false);
  assert.equal(result.mergeApproval,false);
  assert.equal(result.performanceAcceptance,false);
  assert.equal(result.journal.journalCommands,100);
  assert.equal(result.journal.journalEvents,200);
  assert.equal(result.product.fullCohorts.pass,20);
  assert.equal(result.host.statuses.pass,27);
  assert.ok(result.host.idealUnitRatioLogSdCeiling > 0.052 && result.host.idealUnitRatioLogSdCeiling < 0.054);
});
test("real original seven-pair host ceiling classifies point exceedances without changing data",()=>{
  const before=Array(7).fill(100),after=Array(7).fill(105);
  assert.equal(scored(before,after,"host").status,"regression");
  assert.equal(scored(before,before,"host").status,"pass");
  assert.equal(scored(before,after,"renderer").status,"regression");
  assert.equal(scored(before,Array(7).fill(99),"fps").status,"pass");
});
for(const [label,corrupt,expected] of [
  ["partial product",f=>f.productSummary.cohorts.pop(),/20 original A\/A cohorts/],
  ["partial journal",f=>f.productJournal.pop(),/100 started/],
  ["reordered participant journal",f=>{f.productJournal[0].ordinal=2},/journal started order/],
  ["changed renderer",f=>{f.productJournal[1].rendererProof.unmaskedRenderer="SwiftShader"},/software renderer/],
  ["modified original product observation",f=>{f.productSummary.cohorts[0].metrics.fps.after[0]=45},/does not match original/],
  ["faked product score status",f=>{f.productSummary.cohorts[0].metrics.fps.status="inconclusive"},/status was changed/],
  ["moved plan hash",f=>{f.productSummary.planSha256="0".repeat(64)},/plan digest changed/],
  ["extra product cohorts",f=>{f.productSummary.cohorts.push(f.productSummary.cohorts[0])},/20 original A\/A cohorts/],
  ["incomplete host",f=>{f.hostComparison.rows.pop()},/fixed 3×3 host matrix/],
  ["altered host source",f=>{f.hostComparison.identities[1].source="c".repeat(40)},/host source, compiler/],
  ["altered host protocol",f=>{f.hostComparison.protocol.maxUpperRatio=1.5},/host upper95 limit/],
  ["missing worker warmup",f=>{f.hostComparison.warmups.shift()},/each of 126/],
  ["wrong warmup history",f=>{f.hostComparison.warmups[0].side=1},/warmup schedule changed/],
  ["short host pair",f=>{f.hostComparison.rows[0].pairs.pop()},/fixed host pairs missing/],
  ["altered host timing",f=>{f.hostComparison.rows[0].pairs[0][1].creation_ms=50},/paired candidate values changed/],
  ["faked host pass",f=>{f.hostComparison.rows[0].costs.local_ms.status="inconclusive"},/frozen status was changed/],
]){
  test(`${label} fails closed`,()=>assert.throws(()=>auditAA(mutate(corrupt)),expected));
}
test("realistic noisy cost cannot yield a false pass, even if product is clean",()=>{
  const f=modelFixture(),r=f.hostComparison.rows[0];
  r.pairs.forEach((pair,i)=>pair[1].creation_ms = [180,60,130,75,105,85,120][i]);
  r.costs.creation_ms=scored(r.pairs.map(p=>p[0].creation_ms),
    r.pairs.map(p=>p[1].creation_ms),"host");
  assert.notEqual(r.costs.creation_ms.status,"pass");
  f.hostComparison.failures.push({workload:r.workload,mode:r.mode,
    key:"creation_ms",status:r.costs.creation_ms.status});
  const out=auditAA(f);
  assert.equal(out.status,"original-aa-scores-blocked");
  assert.equal(out.host.statuses.pass,26);
  assert.equal(out.performanceAcceptance,false);
});
