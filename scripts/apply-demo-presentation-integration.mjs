import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";

function replaceOnce(text, before, after, label) {
  const index = text.indexOf(before);
  assert.notEqual(index, -1, `missing ${label}`);
  assert.equal(text.indexOf(before, index + before.length), -1, `duplicate ${label}`);
  return text.slice(0, index) + after + text.slice(index + before.length);
}

function removeBalancedDiv(text, marker, label) {
  const start = text.indexOf(marker);
  assert.notEqual(start, -1, `missing ${label}`);
  const token = /<div\b|<\/div>/g;
  token.lastIndex = start;
  let depth = 0;
  let match;
  while ((match = token.exec(text)) !== null) {
    if (match[0] === "<div") depth += 1;
    else depth -= 1;
    if (depth === 0) {
      let end = token.lastIndex;
      if (text[end] === "\n") end += 1;
      return text.slice(0, start) + text.slice(end);
    }
  }
  throw new Error(`unterminated ${label}`);
}

const htmlPath = "web/index.html";
const mainPath = "web/main.js";
const editorPath = "web/python-editor.js";
const smokePath = "scripts/playground-layout-smoke.mjs";

let html = await readFile(htmlPath, "utf8");
let main = await readFile(mainPath, "utf8");
let editor = await readFile(editorPath, "utf8");
let smoke = await readFile(smokePath, "utf8");

const alreadyApplied =
  main.includes("workspace.before(selectedExampleStrip);") &&
  !main.includes("metricCpuFrame") &&
  !html.includes('class="below"') &&
  !html.includes('class="perf-metrics"') &&
  smoke.includes("fakePercentileLabels");
if (alreadyApplied) {
  console.log("demo presentation integration already applied");
  process.exit(0);
}

html = replaceOnce(
  html,
  "grid-template-columns: minmax(21rem, 0.86fr) minmax(0, 1.34fr);",
  "grid-template-columns: minmax(24rem, 1fr) minmax(0, 1.15fr);",
  "desktop workspace split",
);
const obsoleteStyleStart = html.indexOf("      .below {\n");
const responsiveStart = html.indexOf("      @media (max-width: 68rem) {", obsoleteStyleStart);
assert.ok(obsoleteStyleStart >= 0 && responsiveStart > obsoleteStyleStart, "obsolete presentation CSS bounds");
html = html.slice(0, obsoleteStyleStart) + html.slice(responsiveStart);
html = html.replace(
  "\n        .below {\n          grid-template-columns: 1fr;\n        }\n",
  "",
);
html = html.replace(
  "\n        .pipeline {\n          display: grid;\n          grid-template-columns: 1fr 1fr;\n        }\n\n        .arrow {\n          display: none;\n        }\n",
  "",
);
html = removeBalancedDiv(
  html,
  '          <div class="perf-metrics" aria-label="Frame performance p50 and p95">',
  "fake frame-performance panel",
);
const obsoleteMarkupStart = html.indexOf('      <section class="below" aria-label="Architecture and API notes">');
const mainClose = html.indexOf("    </main>", obsoleteMarkupStart);
assert.ok(obsoleteMarkupStart >= 0 && mainClose > obsoleteMarkupStart, "obsolete presentation markup bounds");
html = html.slice(0, obsoleteMarkupStart) + html.slice(mainClose);

main = replaceOnce(
  main,
  'const metricCpuFrame = document.querySelector("#metric-cpu-frame");\nconst metricRuntime = document.querySelector("#metric-runtime");\nconst metricPrepare = document.querySelector("#metric-prepare");\nconst metricUploadMs = document.querySelector("#metric-upload-ms");\nconst metricEncode = document.querySelector("#metric-encode");\nconst metricGpu = document.querySelector("#metric-gpu");\n',
  "",
  "fake performance metric lookups",
);
main = replaceOnce(
  main,
  '    min-height: 3.8rem;\n    padding: 0.75rem 0.85rem;\n    border-bottom: 1px solid var(--border);\n    background: rgba(11, 15, 24, 0.92);',
  '    min-height: 3.8rem;\n    margin-bottom: 1rem;\n    padding: 0.75rem 0.85rem;\n    border: 1px solid var(--border);\n    border-radius: 0.9rem;\n    background: rgba(11, 15, 24, 0.92);',
  "selected example presentation",
);
main = replaceOnce(
  main,
  'selectedExampleStrip.className = "selected-example";\n',
  'selectedExampleStrip.className = "selected-example";\nselectedExampleStrip.setAttribute("aria-label", "Selected example");\n',
  "selected example accessibility label",
);
main = replaceOnce(
  main,
  'document.querySelector(".editor-stack").before(selectedExampleStrip);',
  "workspace.before(selectedExampleStrip);",
  "selected example placement",
);
main = replaceOnce(
  main,
  '    workspace.scrollIntoView({ behavior: "smooth", block: "start" });',
  '    selectedExampleStrip.scrollIntoView({ behavior: "smooth", block: "start" });',
  "selection scroll target",
);
main = replaceOnce(
  main,
  '      metricCpuFrame.value = "engine worker";\n      metricRuntime.value = host.enabled ? `${host.missedDeadlines} host misses` : "engine worker";\n      metricPrepare.value = "render worker";\n      metricUploadMs.value = "render worker";\n      metricEncode.value = "render worker";\n      metricGpu.value = rendererBackend;\n',
  "",
  "fake performance metric assignments",
);

editor = replaceOnce(
  editor,
  '    @media (min-width: 68.01rem) {\n      .canvas-wrap {\n        padding: 0 !important;\n        place-items: stretch !important;\n      }\n      .canvas-wrap > canvas {\n        width: 100%;\n        height: 100%;\n        max-width: none;\n        aspect-ratio: auto;\n        border: 0;\n        border-radius: 0;\n      }\n    }\n',
  "",
  "editor-owned desktop canvas override",
);

const smokeAnchor = '  assert.match(galleryContract.href, /example=parity-square-and-circle/);\n';
const presentationCoverage = `\n  const presentationContract = await page.evaluate(() => {\n    const workspace = document.querySelector(".workspace");\n    const editor = document.querySelector(".editor-pane");\n    const preview = document.querySelector(".preview-pane");\n    const selected = document.querySelector(".selected-example");\n    const workspaceRect = workspace.getBoundingClientRect();\n    const editorRect = editor.getBoundingClientRect();\n    const previewRect = preview.getBoundingClientRect();\n    return {\n      selectedOutsideWorkspace: selected !== null && !workspace.contains(selected),\n      selectedImmediatelyBeforeWorkspace: selected?.nextElementSibling === workspace,\n      obsoletePanels: document.querySelectorAll(\n        ".below, .info-panel, .pipeline, .api-list, .perf-metrics",\n      ).length,\n      fakePercentileLabels: [...document.querySelectorAll(".metric-label")].filter((label) =>\n        /p50\\s*\\/\\s*p95/i.test(label.textContent ?? ""),\n      ).length,\n      editorTop: editorRect.top,\n      previewTop: previewRect.top,\n      editorShare: editorRect.width / workspaceRect.width,\n    };\n  });\n  assert.equal(\n    presentationContract.selectedOutsideWorkspace,\n    true,\n    "selected-example context must not offset only the editor pane",\n  );\n  assert.equal(\n    presentationContract.selectedImmediatelyBeforeWorkspace,\n    true,\n    "selected-example context must sit directly above the shared source/preview workspace",\n  );\n  assert.equal(\n    presentationContract.obsoletePanels,\n    0,\n    "obsolete architecture/API/performance presentation panes must not be rendered",\n  );\n  assert.equal(\n    presentationContract.fakePercentileLabels,\n    0,\n    "public demo must not claim p50/p95 telemetry it does not measure",\n  );\n  assert.ok(\n    Math.abs(presentationContract.editorTop - presentationContract.previewTop) <= 1,\n    \\`desktop source/preview panes must align at the top (\\${presentationContract.editorTop} vs \\${presentationContract.previewTop})\\`,\n  );\n  assert.ok(\n    presentationContract.editorShare >= 0.42 && presentationContract.editorShare <= 0.5,\n    \\`desktop editor should receive a balanced workspace share, got \\${presentationContract.editorShare}\\`,\n  );\n`;
smoke = replaceOnce(smoke, smokeAnchor, smokeAnchor + presentationCoverage, "presentation smoke anchor");
smoke = smoke.replace(
  "✓ Manim gallery + WebGL2 viewport @ DPR",
  "✓ Manim gallery + aligned WebGL2 viewport @ DPR",
);

await Promise.all([
  writeFile(htmlPath, html, "utf8"),
  writeFile(mainPath, main, "utf8"),
  writeFile(editorPath, editor, "utf8"),
  writeFile(smokePath, smoke, "utf8"),
]);
console.log("applied demo presentation integration");
