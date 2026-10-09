// #1933: Derive a diagnostic-only A/A driver from the pinned #1875 host harness.
// The scored source, work, warmups, pair order and pairedCost estimator remain frozen.
import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const STUDY_ID = '1933-apple-host-aa-20261009-02';

function replaceOnce(source, from, to) {
  assert.ok(source.includes(from), `missing frozen source marker: ${from.slice(0, 80)}`);
  assert.equal(source.split(from).length, 2, `ambiguous frozen source marker: ${from.slice(0, 80)}`);
  return source.replace(from, to);
}

const APPLE_HEADLESS_TO_HEADED_AND_PROOF = [
  "  // APPLE_AA_BACKEND_PROOF: performed BEFORE all nine scored original workloads.",
  "  assert.equal(process.platform, \"darwin\", \"standard macOS host required\");",
  "  assert.equal(process.arch, \"arm64\", \"standard Apple Silicon required\");",
  "  assert.equal(process.env.RUNNER_OS, \"macOS\", \"GitHub macOS runner changed\");",
  "  assert.equal(process.env.RUNNER_ARCH, \"ARM64\", \"GitHub hosted Apple Silicon changed\");",
  "  browser = await playwright.chromium.launch({ headless: false,",
  "    args: browserArgs(\"webgl\", { gpuMode: \"hardware\" }) });",
  "  assert.equal(browser.version(), \"151.0.7922.34\", \"pinned Chromium changed\");",
  "  const appleContext = await browser.newContext({ viewport: { width: 64, height: 64 } });",
  "  let appleProof = null;",
  "  try {",
  "    const probe = await appleContext.newPage();",
  "    await probe.goto(\"about:blank\");",
  "    appleProof = await probe.evaluate(() => {",
  "      const canvas = document.createElement(\"canvas\");",
  "      canvas.width = canvas.height = 64;",
  "      const gl = canvas.getContext(\"webgl2\", {",
  "        preserveDrawingBuffer: true, antialias: false, powerPreference: \"high-performance\",",
  "      });",
  "      if (!gl) return { backend: \"missing\", unmaskedRenderer: \"\", unmaskedVendor: \"\" };",
  "      const dbg = gl.getExtension(\"WEBGL_debug_renderer_info\");",
  "      const unmaskedRenderer = dbg ? String(gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL)) : \"\";",
  "      const unmaskedVendor = dbg ? String(gl.getParameter(dbg.UNMASKED_VENDOR_WEBGL)) : \"\";",
  "      gl.viewport(0, 0, 64, 64); gl.clearColor(0.2, 0.4, 0.6, 1);",
  "      gl.clear(gl.COLOR_BUFFER_BIT);",
  "      const pixel = new Uint8Array(4);",
  "      gl.readPixels(32, 32, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);",
  "      const clearPixel = [...pixel];",
  "      const makeShader = (kind, code) => {",
  "        const shader = gl.createShader(kind);",
  "        gl.shaderSource(shader, code); gl.compileShader(shader);",
  "        return { shader, ok: Boolean(gl.getShaderParameter(shader, gl.COMPILE_STATUS)) };",
  "      };",
  "      const vs = makeShader(gl.VERTEX_SHADER, \"#version 300 es\\nvoid main(){vec2 p[3]=vec2[3](vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.));gl_Position=vec4(p[gl_VertexID],0.,1.);}\");",
  "      const fs = makeShader(gl.FRAGMENT_SHADER, \"#version 300 es\\nprecision highp float;\\nout vec4 color;\\nvoid main(){color=vec4(0.6,0.2,0.4,1.);}\");",
  "      let shaderLinked = false, trianglePixel = null;",
  "      if (vs.ok && fs.ok) {",
  "        const program = gl.createProgram();",
  "        gl.attachShader(program, vs.shader); gl.attachShader(program, fs.shader);",
  "        gl.linkProgram(program);",
  "        shaderLinked = Boolean(gl.getProgramParameter(program, gl.LINK_STATUS));",
  "        if (shaderLinked) {",
  "          gl.useProgram(program);",
  "          gl.drawArrays(gl.TRIANGLES, 0, 3);",
  "          gl.readPixels(32, 32, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);",
  "          trianglePixel = [...pixel];",
  "        }",
  "      }",
  "      return { backend: \"WebGL2\", unmaskedRenderer, unmaskedVendor, clearPixel,",
  "        trianglePixel, shaderLinked, glError: gl.getError(), contextLost: gl.isContextLost() };",
  "    });",
  "  } finally { await appleContext.close(); }",
  "  await writeFile(path.join(output, \"apple-browser-proof.json\"),",
  "    stringifyEvidence({ ...appleProof, browserVersion: browser.version(),",
  "      diagnosticOnly: true, qualification: false,",
  "      performanceAcceptance: false, mergeApproval: false }) + \"\\n\", { flag: \"wx\" });",
  "  assert.equal(appleProof.backend, \"WebGL2\", \"real WebGL2 unavailable\");",
  "  assert.match(appleProof.unmaskedRenderer, /Apple.*Metal Renderer.*Apple Paravirtual/i,",
  "    \"actual Apple Metal paravirtual renderer not observed\");",
  "  assert.doesNotMatch(appleProof.unmaskedRenderer, /swiftshader|llvmpipe|software/i,",
  "    \"silent software renderer fallback\");",
  "  assert.match(appleProof.unmaskedVendor, /Apple/i, \"unexpected vendor\");",
  "  const closePixels = (a, b) => Array.isArray(a) && a.length === 4",
  "    && b.every((v, i) => Math.abs(v - a[i]) <= 3);",
  "  assert.ok(closePixels(appleProof.clearPixel, [51, 102, 153, 255]),",
  "    \"WebGL2 clear/readPixels invalid\");",
  "  assert.ok(closePixels(appleProof.trianglePixel, [153, 51, 102, 255]),",
  "    \"WebGL2 GLSL triangle/readPixels invalid\");",
  "  assert.equal(appleProof.shaderLinked, true, \"GLSL compile/link failed\");",
  "  assert.equal(appleProof.glError, 0, \"unexpected GL error\");",
  "  assert.equal(appleProof.contextLost, false, \"unexpected GL context loss\");",
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
    APPLE_HEADLESS_TO_HEADED_AND_PROOF);
  assert.ok(source.includes('APPLE_AA_BACKEND_PROOF'), 'actual Mesa backend guard was omitted');
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
