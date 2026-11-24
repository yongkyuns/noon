import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const workflowUrl = new URL("../.github/workflows/pages.yml", import.meta.url);
const captureSetup = [
  "npm install --no-save --ignore-scripts playwright@1.62.1 pngjs@7.0.0",
  "npx playwright install --with-deps chromium",
  "sudo apt-get update",
  "sudo apt-get install -y ffmpeg",
];
const siteCommands = [
  "node --test scripts/project-site.test.mjs",
  "node scripts/capture-project-media.mjs",
  "node scripts/build-project-site.mjs",
  "node scripts/project-site-smoke.mjs",
];
const captureName = "Capture actual scenes and validate project website";
const setupName = "Install pinned website capture tools";
const recheckName = "Recheck deployment authorization";

function assertProjectSiteDeployment(workflow) {
  const steps = [...workflow.matchAll(/^      - name: ([^\n]+)\n([\s\S]*?)(?=^      - name: |$(?![\s\S]))/gm)]
    .map(([, name, body]) => ({ name, body }));
  const step = name => {
    const matches = steps.filter(entry => entry.name === name);
    assert.equal(matches.length, 1, `expected one ${name} step`);
    return matches[0];
  };
  const requiredOrder = ["Authorize exact demo revision", "Build and validate playground",
    "Stamp deployed revision", setupName, captureName, recheckName, "Configure Pages",
    "Upload Pages artifact", "Deploy", "Verify deployed revision"];
  let previous = -1;
  for (const name of requiredOrder) {
    const entry = step(name);
    const index = steps.indexOf(entry);
    assert.ok(index > previous, `out-of-order website deployment step: ${name}`);
    previous = index;
    assert.doesNotMatch(entry.body, /^\s*(?:if|continue-on-error):/m,
      `${name} must stop on failure, not bypass or suppress qualification`);
  }
  assert.equal(steps.indexOf(step("Configure Pages")), steps.indexOf(step(recheckName)) + 1,
    "authorization must be rechecked after all site generation and immediately before Pages configuration");
  for (const [name, commands] of [[setupName, captureSetup], [captureName, siteCommands]]) {
    assert.equal(step(name).body.trimEnd(), `        run: |\n${commands.map(command => `          ${command}`).join("\n")}`,
      `${name} must retain the complete pinned setup or qualification sequence`);
  }
  for (const name of ["Authorize exact demo revision", recheckName]) {
    assert.equal(step(name).body.trim(), "run: node .github/ci/pages-authorization.mjs");
  }
  assert.match(step("Upload Pages artifact").body, /^          path: _site$/m,
    "deploy the staged homepage and playground, not the original web directory");
  assert.match(step("Upload Pages artifact").body, /^          include-hidden-files: true$/m);
  assert.doesNotMatch(workflow, /steps\.scope\.outputs\.deploy|ci-qualification-scope/,
    "site integration must not resurrect the retired push-deployment selector");
}

function replaceStep(workflow, name, replacement) {
  const start = workflow.indexOf(`      - name: ${name}\n`);
  assert.ok(start >= 0, `missing mutation input: ${name}`);
  const next = workflow.indexOf("      - name: ", start + 1);
  const end = next < 0 ? workflow.length : next;
  return workflow.slice(0, start) + replacement + workflow.slice(end);
}

test("manual Pages deployment retains the staged website and its complete capture qualification", async () => {
  assertProjectSiteDeployment(await readFile(workflowUrl, "utf8"));
});

test("website integration rejects omitted, reordered, skipped and wrong-directory work", async t => {
  const workflow = await readFile(workflowUrl, "utf8");
  const mutations = [
    ["missing capture step", source => replaceStep(source, captureName, "")],
    ["missing pinned setup", source => replaceStep(source, setupName, "")],
    ...siteCommands.map(command => [`omitted ${command}`, source => source.replace(`          ${command}\n`, "")]),
    ["raw web upload", source => source.replace("          path: _site\n", "          path: web\n")],
    ["skipped capture", source => source.replace(`      - name: ${captureName}\n`, `      - name: ${captureName}\n        if: false\n`)],
    ["ignored capture failure", source => source.replace(`      - name: ${captureName}\n`, `      - name: ${captureName}\n        continue-on-error: true\n`)],
    ["retired scope guard", source => source.replace(`      - name: ${setupName}\n`, `      - name: ${setupName}\n        if: \${{ steps.scope.outputs.deploy != 'false' }}\n`)],
    ["unverified revision", source => replaceStep(source, recheckName, `      - name: ${recheckName}\n        run: echo skipped\n\n`)],
    ["recheck before site generation", source => {
      const without = replaceStep(source, recheckName, "");
      return without.replace(`      - name: ${captureName}\n`,
        `      - name: ${recheckName}\n        run: node .github/ci/pages-authorization.mjs\n\n      - name: ${captureName}\n`);
    }],
    ["staging before capture", source => source.replace(siteCommands[1], "TEMPORARY_SENTINEL")
      .replace(siteCommands[2], siteCommands[1]).replace("TEMPORARY_SENTINEL", siteCommands[2])],
    ["duplicate capture", source => source.replace(`      - name: ${captureName}\n`,
      `      - name: ${captureName}\n        run: echo duplicate\n\n      - name: ${captureName}\n`)],
  ];
  for (const [label, mutate] of mutations) {
    await t.test(label, () => {
      const changed = mutate(workflow);
      assert.notEqual(changed, workflow, `mutation must change the input: ${label}`);
      assert.throws(() => assertProjectSiteDeployment(changed), `undetected site regression: ${label}`);
    });
  }
});
