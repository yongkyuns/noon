// #1933: fail-closed, read-only inspection of pre-existing identical-source A/A evidence.
// NOT a scorer for a candidate PR; never revises or replaces original acceptance.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const WORKLOADS = Object.freeze([
  "parity-square-and-circle", "showcase-camera-follows-path", "showcase-first-scene",
  "showcase-raster-images", "showcase-bezier-paths",
]);
const MODES = Object.freeze(["async", "portable", "jspi"]);
const HOST_WORKLOADS = Object.freeze(["deterministic", "segments", "callbacks"]);
const COSTS = Object.freeze(["creation_ms", "local_ms", "execution_ms"]);
const T95 = new Map([[3, 4.302652729696142], [7, 2.446911848791681]]);
const frozen = Object.freeze({ fpsLower95: 0.97, costPoint: 1.03, costUpper95: 1.05,
  productPairs: [7, 3], hostPairs: 7 });

function assertNonQualification(value, source) {
  assert.equal(value?.diagnosticOnly, true, `${source}: not explicitly diagnostic-only`);
  for (const k of ["qualification", "performanceAcceptance", "mergeApproval"])
    assert.equal(value[k], false, `${source}: false ${k} required`);
}

function closeNumber(found, expected, where) {
  assert.ok(Number.isFinite(found) && Number.isFinite(expected) &&
    Math.abs(found - expected) <= 2e-7 * Math.max(1, Math.abs(expected)),
  `${where}: frozen score does not match original observations (${found}, ${expected})`);
}

// Independent read-only integrity check of the *frozen* Student-t estimators.
// This does NOT produce scored candidate-vs-baseline acceptance evidence.
export function originalInterval(before, after) {
  assert.ok(Array.isArray(before) && Array.isArray(after), "both ordered pairs required");
  assert.equal(before.length, after.length, "missing paired side");
  assert.ok(T95.has(before.length), "only original 3 and 7 paired cohorts accepted");
  assert.ok([...before, ...after].every(x => Number.isFinite(x) && x > 0),
    "all original paired samples must be finite, positive");
  const logs = before.map((x, i) => Math.log(after[i] / x));
  const mean = logs.reduce((a, b) => a + b, 0) / logs.length;
  const variance = logs.reduce((a, b) => a + (b - mean) ** 2, 0) / (logs.length - 1);
  const sd = Math.sqrt(variance);
  const half = T95.get(logs.length) * sd / Math.sqrt(logs.length);
  return { ratio: Math.exp(mean), lower: Math.exp(mean - half),
    upper: Math.exp(mean + half), logSd: sd };
}

export function compareOriginalMetric(value, n, kind, where) {
  assert.equal(value?.before?.length, n, `${where}: baseline pair count changed`);
  assert.equal(value?.after?.length, n, `${where}: candidate pair count changed`);
  const score = originalInterval(value.before, value.after);
  for (const field of ["ratio", "lower", "upper"])
    closeNumber(value[field], score[field], `${where}.${field}`);
  const status = kind === "fps"
    ? (score.upper < frozen.fpsLower95 ? "regression"
      : score.lower >= frozen.fpsLower95 ? "pass" : "inconclusive")
    : kind === "renderer"
      ? (score.lower > frozen.costPoint ? "regression"
        : score.ratio <= frozen.costPoint && score.upper <= frozen.costUpper95
          ? "pass" : "inconclusive")
      : (score.ratio > frozen.costPoint ? "regression"
        : score.upper > frozen.costUpper95 ? "inconclusive" : "pass");
  assert.equal(value.status, status, `${where}: frozen status was changed`);
  return { status, ratio: score.ratio, lower: score.lower,
    upper: score.upper, logSd: score.logSd };
}

function tally(values) {
  return { pass: values.filter(x => x === "pass").length,
    inconclusive: values.filter(x => x === "inconclusive").length,
    regression: values.filter(x => x === "regression").length };
}
function median(values) {
  assert.ok(values.length > 0 && values.every(Number.isFinite), "invalid sample spread");
  const ordered = [...values].sort((a,b) => a-b), i = Math.floor(ordered.length/2);
  return ordered.length % 2 ? ordered[i] : (ordered[i-1]+ordered[i])/2;
}
function staticHostProtocol(p) {
  assert.equal(p?.pairs, frozen.hostPairs, "host fixed seven-pair policy changed");
  assert.equal(p?.maxPointRatio, frozen.costPoint, "host point limit changed");
  assert.equal(p?.maxUpperRatio, frozen.costUpper95, "host upper95 limit changed");
  assert.equal(p.warmups, 2, "global host warmup contract changed");
  assert.equal(p.scoredWorkerWarmups, 1, "host worker warmup contract changed");
  assert.equal(p.objects, 600, "host geometry count changed");
  assert.equal(p.localEdits, 2048, "host local edit count changed");
  assert.equal(p.segments, 32, "host segment count changed");
  assert.equal(p.callbacks, 4, "host callback count changed");
  assert.equal(p.samples, 60, "host sample count changed");
  assert.equal(p.sampleHz, 48, "host sample rate changed");
  assert.equal(p.workerLifetime, "fresh-per-pair-warmed", "host worker history changed");
  assert.deepEqual(p.modes, MODES, "host mode set changed");
  assert.deepEqual(p.workloads, HOST_WORKLOADS, "host workload set changed");
}
function validatePlan(plan, rawPlan, product) {
  assert.equal(plan?.schema, 1, "product A/A plan schema changed");
  assert.equal(plan?.kind, "diagnostic-only", "not an A/A study");
  assert.equal(plan?.qualification, false, "product A/A must not qualify production");
  assert.equal(plan?.retries, 0, "no retries permitted");
  assert.equal(plan?.stopOnTrialError, true, "partial/failed A/A must fail closed");
  assert.deepEqual(plan.cohorts, [{name:"seven-pair", pairs:7}, {name:"three-pair",pairs:3}]);
  assert.deepEqual(plan.workloads, WORKLOADS);
  assert.deepEqual(plan.builds?.map(b=>b.name), ["baseline","candidate"]);
  assert.equal(plan.plannedPairCommands, 100, "full A/A product pairs required");
  assert.equal(plan.plannedObservations, 200, "full A/A product observations required");
  for (const b of plan.builds) {
    assert.match(b.source, /^[a-f0-9]{40}$/, "unidentified producer source");
    assert.ok(Number.isSafeInteger(b.artifactId) && b.artifactId > 0,
      "build must have pinned producer artifact ID");
    assert.match(b.zipSha256, /^[a-f0-9]{64}$/, "build ZIP digest missing");
  }
  const sha = createHash("sha256").update(rawPlan).digest("hex");
  assert.equal(product.planSha256, sha, "recorded frozen product plan digest changed");
  assert.equal(product.studyId, plan.studyId, "product source plan mismatched");
  return sha;
}

export function analyzeProductAA(plan, rawPlan, product) {
  assertNonQualification(product, "product summary");
  const planSha256 = validatePlan(plan, rawPlan, product);
  assert.equal(product.acquisitionComplete, true, "product A/A not fully acquired");
  assert.equal(product.status, "diagnostic-complete", "product diagnostic is incomplete");
  assert.equal(product.plannedPairCommands, 100);
  assert.equal(product.completedPairCommands, 100);
  assert.deepEqual(product.errors, [], "recorded product errors cannot be ignored");
  assert.equal(product.cohorts?.length, 20, "all 20 original A/A cohorts required");
  const full = [], fps = [], render = [], latencies = [], spreads = [];
  const seen = new Set();
  for (const c of product.cohorts) {
    assert.ok(["seven-pair","three-pair"].includes(c.cohort), "wrong cohort scope");
    assert.ok(["baseline","candidate"].includes(c.build), "wrong A/A source participant");
    assert.ok(WORKLOADS.includes(c.workload), "unexpected A/A product workload");
    const key = `${c.cohort}/${c.build}/${c.workload}`;
    assert.ok(!seen.has(key), `${key}: duplicate cohort`); seen.add(key);
    const n = c.cohort === "seven-pair" ? 7 : 3;
    assert.equal(c.pairs, n, `${key}: unregistered pair count`);
    assert.equal(c.comparator?.code, 0, `${key}: original comparator failed`);
    assert.equal(c.comparator?.timedOut, false, `${key}: incomplete comparator`);
    assert.deepEqual(c.behaviorFailures, [], `${key}: behavior/visual failure`);
    assert.ok(Array.isArray(c.strictLatencyFailures), `${key}: missing strict latency evidence`);
    const f = compareOriginalMetric(c.metrics?.fps, n, "fps", `${key}/fps`);
    fps.push(f.status); spreads.push({key:`${key}/fps`,logSd:f.logSd});
    const applicable = c.workload !== "parity-square-and-circle";
    assert.equal(c.metrics?.render === null, !applicable,
      `${key}: renderer cost applicability changed`);
    const rc = applicable ? compareOriginalMetric(c.metrics.render, n, "renderer", `${key}/render`) : null;
    if (rc) { render.push(rc.status); spreads.push({key:`${key}/render`,logSd:rc.logSd}); }
    const statuses = [f.status, ...(rc ? [rc.status] : [])];
    const computed = statuses.includes("regression") ? "regression"
      : statuses.includes("inconclusive") ? "inconclusive" : "pass";
    assert.equal(c.metrics.status, computed, `${key}: full metric status was changed`);
    const pass = computed === "pass" && c.strictLatencyFailures.length === 0;
    assert.equal(c.diagnosticClassification,
      pass ? "within-frozen-budgets" : "blocked-on-identical-build",
      `${key}: full strict A/A classification was changed`);
    full.push(pass ? "pass" : "blocked");
    for (const value of c.strictLatencyFailures) latencies.push({key, original:value});
  }
  for (const cohort of ["seven-pair", "three-pair"])
    for (const build of ["baseline", "candidate"])
      for (const workload of WORKLOADS)
        assert.ok(seen.has(`${cohort}/${build}/${workload}`),
          `missing fixed A/A case ${cohort}/${build}/${workload}`);
  return { studyId: product.studyId, planSha256,
    completedPairCommands: 100, cohorts: 20,
    fullCohorts: {pass:full.filter(s=>s==="pass").length,
      blocked:full.filter(s=>s==="blocked").length},
    fps: tally(fps), rendererCost: tally(render), strictLatencyFailures:latencies.length,
    medianLogSd: {fps:median(spreads.filter(v=>v.key.endsWith("/fps")).map(x=>x.logSd)),
      renderer:median(spreads.filter(v=>v.key.endsWith("/render")).map(x=>x.logSd))},
    allStrictCohortsPass: full.every(x=>x==="pass") };
}

export function analyzeHostAA(host) {
  assert.equal(host?.schema, 1, "host comparison schema changed");
  assertNonQualification(host, "host comparison");
  assert.equal(host.identities?.length, 2, "missing host participant identities");
  assert.deepEqual(host.identities[0],host.identities[1],
    "A/A host source, compiler and package manifest identities differ");
  assert.match(host.identities[0].source, /^[a-f0-9]{40}$/, "missing host source commit");
  staticHostProtocol(host.protocol);
  assert.equal(host.rows?.length, 9, "fixed 3×3 host matrix required");
  assert.equal(host.warmups?.length, 126, "each of 126 participants must warm once");
  let warmIndex = 0;
  for(const work of HOST_WORKLOADS)for(const mode of MODES)
    for(let pair=1;pair<=7;pair++) {
      const order = pair % 2 ? [0,1] : [1,0];
      for(const side of order) {
        const observed = host.warmups[warmIndex++];
        assert.deepEqual({side:observed?.side,workload:observed?.workload,
          mode:observed?.mode,pair:observed?.pair,warmup:observed?.warmup,
          workerLifetime:observed?.workerLifetime},
        {side,workload:work,mode,pair,warmup:1,workerLifetime:"fresh-per-pair-warmed"},
        `host worker warmup schedule changed at event ${warmIndex}`);
        assert.equal(observed.result?.mode,mode,"warmup mode changed");
        assert.equal(observed.result?.workload,work,"warmup workload changed");
      }
    }
  assert.ok(Array.isArray(host.failures), "missing original scored failure list");
  const result = [], spreads = {creation_ms:[],local_ms:[],execution_ms:[]}, failing = [];
  const seen = new Set();
  for (const row of host.rows) {
    assert.ok(MODES.includes(row.mode) && HOST_WORKLOADS.includes(row.workload),
      "unrecognized host workload/mode");
    const key = `${row.workload}/${row.mode}`;
    assert.ok(!seen.has(key), `${key}: duplicated host row`);seen.add(key);
    assert.match(row.sourceSha, /^[a-f0-9]{64}$/, `${key}: source hash missing`);
    assert.equal(row.pairs?.length,7,`${key}: fixed host pairs missing`);
    for(const [i,pair] of row.pairs.entries()){
      assert.equal(pair?.length,2,`${key}[${i}] incomplete participant pair`);
      for(const side of pair){
        assert.equal(side.mode,row.mode,`${key}[${i}] changed mode`);
        assert.equal(side.workload,row.workload,`${key}[${i}] changed workload`);
        assert.equal(side.metrics?.backend,"WebGL2",`${key}[${i}]: wrong renderer`);
      }
      assert.equal(pair[0].callback_calls,pair[1].callback_calls,
        `${key}[${i}] unequal callback work`);
      assert.equal(pair[0].metrics?.presentedFrames,pair[1].metrics?.presentedFrames,
        `${key}[${i}] unequal frame work`);
    }
    assert.deepEqual(Object.keys(row.costs).sort(),[...COSTS].sort(),
      `${key}: altered original cost metrics`);
    for(const metric of COSTS){
      const data=row.costs[metric];
      const before=row.pairs.map(p=>p[0][metric]),after=row.pairs.map(p=>p[1][metric]);
      assert.deepEqual(data.before,before,`${key}/${metric}: paired baseline values changed`);
      assert.deepEqual(data.after,after,`${key}/${metric}: paired candidate values changed`);
      const scored=compareOriginalMetric(data,7,"host",`${key}/${metric}`);
      result.push(scored.status);spreads[metric].push(scored.logSd);
      if(scored.status!=="pass")failing.push(`${key}/${metric}/${scored.status}`);
    }
  }
  for(const work of HOST_WORKLOADS)for(const mode of MODES)
    assert.ok(seen.has(`${work}/${mode}`),`missing host matrix case ${work}/${mode}`);
  assert.equal(host.failures.length,failing.length,
    "original host failure count differs from retained metric values");
  const observedFailures=new Set(host.failures.map(f=>`${f.workload}/${f.mode}/${f.key}/${f.status}`));
  assert.equal(observedFailures.size,failing.length,"host failure list contains duplicates");
  for (const key of failing) assert.ok(observedFailures.has(key),`missing declared host failure ${key}`);
  return {studyId:host.studyId,sourceSha:host.identities[0].source,
    scoredPairs:63,scoredMetrics:27,statuses:tally(result),
    medianLogSd: {all:median(Object.values(spreads).flat()),
      creation:median(spreads.creation_ms),local:median(spreads.local_ms),
      execution:median(spreads.execution_ms)},
    idealUnitRatioLogSdCeiling: Math.log(frozen.costUpper95) * Math.sqrt(7) / T95.get(7),
    allStrictHostMetricsPass:result.every(x=>x==="pass") };
}


export function analyzeProductJournal(plan, journal, productSummary) {
  assert.ok(Array.isArray(journal) && journal.length === 200,
    "all 100 started + 100 recorded original A/A product observations required");
  const expected = [];
  for (const cohort of plan.cohorts) {
    for (const workload of plan.workloads) {
      for (let pair = 1; pair <= cohort.pairs; pair++) {
        const order = pair % 2 ? ["baseline", "candidate"] : ["candidate", "baseline"];
        for (const build of order) expected.push({
          ordinal: expected.length + 1, cohort: cohort.name, pairs: cohort.pairs,
          workload, pair, build, logicalOrder: order,
        });
      }
    }
  }
  assert.equal(expected.length, 100);
  const gpu = new Set(), load = [], overcapacity = [], cpuModels = new Set();
  for (const [idx, evt] of expected.entries()) {
    const started = journal[idx * 2], recorded = journal[idx * 2 + 1];
    for (const [kind, record] of [["started", started], ["recorded", recorded]]) {
      assert.equal(record?.status, kind, `${idx + 1}: missing journal ${kind} event`);
      for (const key of ["ordinal", "cohort", "pairs", "workload", "pair", "build"])
        assert.equal(record[key], evt[key], `${idx + 1}: journal ${kind} order changed (${key})`);
      assert.deepEqual(record.logicalOrder, evt.logicalOrder,
        `${idx + 1}: journal ${kind} participant order changed`);
      assert.ok(Number.isFinite(record.before?.load?.[0]) && record.before.load[0] >= 0,
        `${idx + 1}: missing pre-measurement one-minute load`);
      assert.ok(Array.isArray(record.before.cpu) && record.before.cpu.length > 0,
        `${idx + 1}: missing observed CPU capacity`);
      assert.ok(record.before.cpu.every(c => typeof c.model === "string" && c.model.length > 0),
        `${idx + 1}: missing CPU model`);
    }
    assert.equal(recorded.command?.code, 0, `${idx + 1}: product command failed`);
    assert.equal(recorded.command?.timedOut, false, `${idx + 1}: product command timed out`);
    assert.ok(recorded.command.elapsedMs > 0 && Number.isFinite(recorded.command.elapsedMs),
      `${idx + 1}: measured command elapsed time missing`);
    const proof = recorded.rendererProof;
    assertNonQualification(proof, `${idx + 1}: renderer identity proof`);
    assert.equal(proof.backend, "WebGL2", `${idx + 1}: renderer backend changed`);
    assert.equal(proof.shaderLinked, true, `${idx + 1}: GLSL proof missing`);
    assert.equal(proof.contextLost, false, `${idx + 1}: GL context lost`);
    assert.equal(proof.glError, 0, `${idx + 1}: GL error present`);
    assert.equal(proof.browserVersion, plan.environment.browserVersion,
      `${idx + 1}: Chromium browser version changed`);
    assert.equal(proof.pairIndex, evt.pair, `${idx + 1}: renderer proof is from another pair`);
    assert.equal(proof.exampleId, evt.workload, `${idx + 1}: renderer proof is from another workload`);
    assert.ok(proof.unmaskedRenderer && typeof proof.unmaskedRenderer === "string",
      `${idx + 1}: missing unmasked renderer identity`);
    assert.doesNotMatch(proof.unmaskedRenderer,/swiftshader|llvmpipe|software (?:rasterizer|renderer)/i,
      `${idx + 1}: software renderer did not match declared hardware-like mode`);
    if (plan.environment.expectedUnmaskedRenderer) {
      assert.ok(proof.unmaskedRenderer.includes(plan.environment.expectedUnmaskedRenderer),
        `${idx + 1}: changed preregistered device identity`);
    }
    for (const key of ["clearPixel", "trianglePixel"]) {
      assert.equal(proof[key]?.length, 4, `${idx + 1}: missing actual GL ${key}`);
      assert.ok(proof[key].every(x=>Number.isInteger(x) && x>=0 && x<=255),
        `${idx + 1}: invalid GL ${key}`);
    }
    gpu.add(proof.unmaskedRenderer);
    const { before } = recorded;
    load.push(before.load[0]);
    overcapacity.push(before.load[0] > before.cpu.length);
    for (const core of before.cpu) cpuModels.add(core.model);
  }
  assert.equal(gpu.size, 1, "A/A product renderer changed within one study");
  assert.equal(productSummary.completedPairCommands, expected.length);
  return { journalCommands: expected.length, journalEvents: journal.length,
    alternatingOrderValidated: true, realWebglProofs: expected.length,
    observedRenderer: [...gpu][0], cpuModels: [...cpuModels],
    preRunOneMinuteLoad:{median: median(load),
      maximum:Math.max(...load),
      samplesAboveObservedCpuCount:overcapacity.filter(Boolean).length,
      samples:load.length},
    sourceProofScope:"renderer string + shader/pixel proof does not independently establish physical GPU provenance" };
}

export function auditAA({productPlan,rawProductPlan,productSummary,productJournal,hostComparison}) {
  const product=analyzeProductAA(productPlan,rawProductPlan,productSummary);
  const journal=analyzeProductJournal(productPlan,productJournal,productSummary);
  const host=analyzeHostAA(hostComparison);
  const ok=product.allStrictCohortsPass&&host.allStrictHostMetricsPass;
  return {schema:1,diagnosticOnly:true,qualification:false,
    performanceAcceptance:false,mergeApproval:false,
    purpose:"same-source-A/A-metrology-precision-screen-only",
    status:ok?"original-aa-scores-consistent":"original-aa-scores-blocked",
    sourceProofScope:"archive-stated identities; external release ZIP hashes and physical device provenance require independent attestation",
    originalThresholds:frozen,product,journal,host,
    blockers:[...(product.allStrictCohortsPass?[]:["identical-build product strict cohorts did not all pass"]),
      ...(host.allStrictHostMetricsPass?[]:["identical-build Python host strict metrics did not all pass"])],
  };
}

async function main(argv) {
  if(argv.length===1 && argv[0]==="--help"){
    console.log("Read-only fail-closed A/A precision screen, never release qualification.");
    return;
  }
  assert.equal(argv.length,5,
    "usage: node perf-aa-precision-audit.mjs PLAN.json PRODUCT-SUMMARY.json PRODUCT-JOURNAL.jsonl HOST-COMPARISON.json REPORT.json");
  const [planPath,productPath,journalPath,hostPath,outputPath]=argv;
  let report;
  try {
    const rawProductPlan=await readFile(planPath);
    const journal=(await readFile(journalPath,"utf8")).trimEnd().split("\n").map(JSON.parse);
    report=auditAA({productPlan:JSON.parse(rawProductPlan),rawProductPlan,
      productSummary:JSON.parse(await readFile(productPath)),productJournal:journal,
      hostComparison:JSON.parse(await readFile(hostPath))});
  }catch(error){
    report={schema:1,diagnosticOnly:true,qualification:false,performanceAcceptance:false,
      mergeApproval:false,status:"invalid-or-incomplete",errors:[String(error?.message??error)]};
  }
  await mkdir(path.dirname(outputPath),{recursive:true});
  await writeFile(outputPath,JSON.stringify(report,null,2)+"\n",{flag:"wx"});
  console.log(JSON.stringify({status:report.status,blockers:report.blockers??report.errors}));
  if(report.status!=="original-aa-scores-consistent")process.exitCode=1;
}

if(process.argv[1] && path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  await main(process.argv.slice(2));
}
