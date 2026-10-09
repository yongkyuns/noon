// #1933: Derive a diagnostic-only A/A driver from the pinned #1875 host harness.
// The scored source, work, warmups, pair order and pairedCost estimator remain frozen.
import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const STUDY_ID = '1933-mesa-host-aa-20261009-02';

function replaceOnce(source, from, to) {
  assert.ok(source.includes(from), `missing frozen source marker: ${from.slice(0, 80)}`);
  assert.equal(source.split(from).length, 2, `ambiguous frozen source marker: ${from.slice(0, 80)}`);
  return source.replace(from, to);
}

const MESA_HEADLESS_TO_HEADED_AND_PROOF = [
  '  // MESA_AA_BACKEND_PROOF: pre-scored GL identity/readback, never a timing sample.',
  '  assert.equal(process.env.LIBGL_ALWAYS_SOFTWARE, "true", "Mesa software selector changed");',
  '  assert.equal(process.env.GALLIUM_DRIVER, "llvmpipe", "Mesa driver selector changed");',
  '  assert.equal(process.env.LP_NUM_THREADS, "2", "Mesa thread cap changed");',
  '  assert.ok(process.env.DISPLAY, "headed Mesa test needs an active Xvfb display");',
  '  browser = await playwright.chromium.launch({ headless: false, args: browserArgs("webgl") });',
  '  assert.equal(browser.version(), "151.0.7922.34", "browser version changed");',
  '  const mesaContext = await browser.newContext({ viewport: { width: 64, height: 64 } });',
  '  let mesaProof;',
  '  try {',
  '    const p = await mesaContext.newPage();',
  '    await p.goto("about:blank");',
  '    mesaProof = await p.evaluate(() => {',
  '      const canvas = document.createElement("canvas"); canvas.width = canvas.height = 64;',
  '      const gl = canvas.getContext("webgl2", { preserveDrawingBuffer: true, antialias: false });',
  '      if (!gl) return { backend: "missing", unmaskedRenderer: "", readbackValid: false };',
  '      const dbg = gl.getExtension("WEBGL_debug_renderer_info");',
  '      const unmaskedRenderer = dbg ? String(gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL)) : "";',
  '      gl.clearColor(0.2, 0.4, 0.6, 1); gl.clear(gl.COLOR_BUFFER_BIT);',
  '      const pixel = new Uint8Array(4); gl.readPixels(32, 32, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);',
  '      const target = [51, 102, 153, 255];',
  '      const readbackValid = [...pixel].every((v, i) => Math.abs(v - target[i]) <= 3)',
  '        && gl.getError() === gl.NO_ERROR && gl.isContextLost() === false;',
  '      return { backend: "WebGL2", unmaskedRenderer, pixel: [...pixel], readbackValid };',
  '    });',
  '  } finally { await mesaContext.close(); }',
  '  await writeFile(path.join(output, "mesa-browser-proof.json"), stringifyEvidence({',
  '    ...mesaProof, browserVersion: browser.version(), configuredThreads: process.env.LP_NUM_THREADS,',
  '    galliumDriver: process.env.GALLIUM_DRIVER, diagnosticOnly: true,',
  '    qualification: false, performanceAcceptance: false, mergeApproval: false,',
  '  }) + "\\n", { flag: "wx" });',
  '  assert.equal(mesaProof.backend, "WebGL2", "no actual WebGL2 surface");',
  '  assert.match(mesaProof.unmaskedRenderer, /llvmpipe/i, "no verified Mesa llvmpipe renderer");',
  '  assert.doesNotMatch(mesaProof.unmaskedRenderer, /swiftshader/i, "unexpected SwiftShader fallback");',
  '  assert.equal(mesaProof.readbackValid, true, "actual Mesa readPixels failed");',
].join("\n");

export function deriveAAHostDriver(original) {
  assert.equal(typeof original, 'string');
  let source = replaceOnce(original,
    'const role = side === 0 ? "baseline" : "candidate";',
    'const role = "baseline"; // Both roles are the SAME authenticated original baseline.');
  const begin = '  // Every manifest workload receives the same strict seven-pair qualification.';
  const end = '\n} catch (error) {';
  assert.equal(source.split(begin).length, 2, 'missing or duplicated host-only cutoff');
  const startIndex = source.indexOf(begin);
  const endIndex = source.indexOf(end, startIndex + begin.length);
  assert.ok(endIndex > startIndex, 'missing host acquisition cleanup boundary');
  // Only the already-frozen nine-workload/7-pair host acquisition is used here.
  // Original product comparison, intrusive profiling and retained-worker diagnostics
  // are intentionally excluded from this separate A/A precision investigation.
  const metadata = `  await writeFile(path.join(output, "study.json"), stringifyEvidence({\n` +
    `    schema: 1, studyId: "${STUDY_ID}", diagnosticOnly: true,\n` +
    '    comparison: "authenticated-baseline-versus-itself",\n' +
    '    acquisitionComplete: rows.length === protocol.workloads.length * protocol.modes.length &&\n' +
    '      rows.every(row => row.pairs.length === protocol.pairs),\n' +
    '    rows: rows.map(({ workload, mode, sourceSha, costs, pairs }) => ({\n' +
    '      workload, mode, sourceSha, costs, pairCount: pairs.length,\n' +
    '    })), identities,\n' +
    '    qualification: false, performanceAcceptance: false, mergeApproval: false,\n' +
    '  }) + "\\n");';
  source = source.slice(0, startIndex) + metadata + source.slice(endIndex);
  source = replaceOnce(source,
    'stringifyEvidence({ schema: 1, protocol, identities, changedBuildInputs,',
    `stringifyEvidence({ schema: 1, studyId: "${STUDY_ID}", diagnosticOnly: true, qualification: false, performanceAcceptance: false, mergeApproval: false, protocol, identities, changedBuildInputs,`);
  source = replaceOnce(source,
    'assert.deepEqual(failures, [], "performance regression or inconclusive qualification; all samples retained");',
    'assert.equal(rows.length, protocol.workloads.length * protocol.modes.length, "missing fixed host A/A rows");\n' +
    'for (const row of rows) assert.equal(row.pairs.length, protocol.pairs, "incomplete fixed host A/A pairs");\n' +
    'assert.deepEqual(failures.filter(failure => !(failure.workload && failure.mode && failure.key)), [],\n' +
    '  "host A/A acquisition invalid; retained evidence must not be used for precision claims");');
  // Only browser construction and a pre-scored read-only GPU identity proof
  // differ from the frozen, uninstrumented original host A/A acquisition.
  source = replaceOnce(source,
    '  browser = await playwright.chromium.launch({ headless: true, args: browserArgs("webgl") });',
    MESA_HEADLESS_TO_HEADED_AND_PROOF);
  assert.ok(source.includes('MESA_AA_BACKEND_PROOF'), 'actual Mesa backend guard was omitted');
  assert.ok(source.includes('scoredPairSchedule(pair + 1)'), 'original alternating schedule lost');
  assert.ok(source.includes('pairedCost(pairs.map(pair => pair[0][key])'), 'original scorer lost');
  assert.ok(source.includes('workerLifetime: "fresh-per-pair-warmed"'), 'original worker lifetime lost');
  assert.ok(!source.includes('qualifyProductCohorts(async directory =>'), 'product comparison leaked into A/A');
  return source;
}

async function main() {
  const [srcPath, destPath] = process.argv.slice(2);
  assert.ok(srcPath && destPath, 'usage: node host-cost-aa-derive.mjs <frozen-harness-file> <derived-driver-file>');
  const original = await readFile(srcPath, 'utf8');
  const derived = deriveAAHostDriver(original);
  await writeFile(destPath, derived);
  console.log(`Prepared diagnostic-only ${STUDY_ID}: ${path.resolve(destPath)}`);
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error); process.exitCode = 1; });
}
