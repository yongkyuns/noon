import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { interpretation, validatePlan, STUDY_ID } from "./macos-http-server-preflight.mjs";
const plan = JSON.parse(await readFile(new URL("../benchmarks/macos-server-preflight.json", import.meta.url)));

test("frozen preflight is a one-shot diagnosis of the historical zero-sample failure", () => {
  assert.equal(validatePlan(plan), plan);
  assert.equal(plan.studyId, STUDY_ID);
  assert.equal(plan.priorAttempt.run, 37977203253);
  assert.equal(plan.priorAttempt.completedPairs, 0);
  assert.deepEqual(plan.windows.map(x=>x.durationSeconds),[10,5]);
  assert.equal(plan.oneShot, true);
  assert.equal(plan.retries, 0);
  for (const edit of [
    p=>p.runner="macos-15-xlarge",
    p=>p.priorAttempt.completedPairs=1,
    p=>p.windows[0].durationSeconds=2,
    p=>p.performanceAcceptance=true,
    p=>p.oneShot=false,
  ]) { const p=structuredClone(plan);edit(p);assert.throws(()=>validatePlan(p)); }
});

test("exact own-server HTTP response qualifies even with zero stdout startup banner", () => {
  const x = { childAliveBeforeHTTP: true, bannerSeen: false, pid: 13,
    response: { status:200,bodyMatch:true }, owner: {ownerMatched:true} };
  const a=interpretation(x);
  assert.equal(a.status,"operational");
  assert.equal(a.bannerSeen,false);
  assert.equal(a.bannerRequired,false);
  assert.equal(a.qualification,false);
  assert.equal(a.performanceAcceptance,false);
  assert.equal(a.mergeApproval,false);
});

test("stale server, no HTTP response, or dead child fail closed", () => {
  const healthy= { childAliveBeforeHTTP: true, bannerSeen:true,
    response: {status:200,bodyMatch:true},owner:{ownerMatched:true} };
  for(const edit of [
    x=>x.childAliveBeforeHTTP=false,
    x=>x.response.bodyMatch=false,
    x=>x.response.status=404,
    x=>x.owner.ownerMatched=false,
    x=>x.response=null,
  ]) {const x=structuredClone(healthy);edit(x);assert.equal(interpretation(x).status,"not-operational");}
  assert.equal(interpretation(null).status,"not-operational");
});
