import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { acquire, classifyMode, EXPECTED_BROWSER, launchForMode, PINNED_MODES,
  STUDY } from "./perf-hosted-mesa-feas.mjs";

const valid = (renderer) => ({
  browser: EXPECTED_BROWSER, backend: "WebGL2", unmaskedRenderer: renderer,
  clearPixel: [51, 102, 153, 255], trianglePixel: [153, 51, 102, 255],
  linkSuccess: true, glError: 0, contextLost: false,
});
const plan = { studyId: STUDY, scenarios: PINNED_MODES.map(x => ({ id: x.id })) };

test("only observed real Mesa llvmpipe + completed WebGL2 draw/readback qualifies as feasible", () => {
  assert.equal(classifyMode(PINNED_MODES[0], valid("ANGLE SwiftShader Device")).status,
    "usable-software-webgl2-backend");
  assert.equal(classifyMode(PINNED_MODES[1], valid("ANGLE (Mesa llvmpipe LLVM 18)")).status,
    "usable-software-webgl2-backend");
  assert.equal(classifyMode(PINNED_MODES[1], valid("ANGLE SwiftShader Device")).status,
    "not-confirmed");
  assert.equal(classifyMode(PINNED_MODES[2], valid("ANGLE (Google SwiftShader llvmpipe)")).status,
    "not-confirmed");
  assert.equal(classifyMode(PINNED_MODES[2], { ...valid("Mesa llvmpipe"), trianglePixel: [0, 0, 0, 0] }).status,
    "not-confirmed");
  assert.equal(classifyMode(PINNED_MODES[1], { ...valid("Mesa llvmpipe"), browser: "other" }).status,
    "not-confirmed");
  assert.equal(classifyMode(PINNED_MODES[1], { ...valid("Mesa llvmpipe"), unmaskedRenderer: "" }).status,
    "not-confirmed");
});

test("fixed modes expose documented llvmpipe control only for Mesa; no silent renderer switching", () => {
  assert.deepEqual(PINNED_MODES.map(x => x.id),
    ["frozen-swiftshader-control", "mesa-angle-gl", "mesa-angle-gl-egl"]);
  for (const mode of PINNED_MODES) {
    const options = launchForMode(mode);
    assert.equal(options.headless, false);
    if (mode.mesa) {
      assert.equal(options.env.LP_NUM_THREADS, "2");
      assert.equal(options.env.GALLIUM_DRIVER, "llvmpipe");
      assert.equal(options.env.LIBGL_ALWAYS_SOFTWARE, "true");
      assert.ok(options.args.includes("--use-angle=" + mode.angle));
      assert.ok(!options.args.includes("--use-angle=swiftshader"));
    } else assert.ok(options.args.includes("--use-angle=swiftshader"));
  }
  assert.throws(() => launchForMode({ id: "unregistered", angle: "gl", mesa: true }));
});

test("collect exactly three results and fail closed without retry or overwriting any evidence", async () => {
  const dir = await mkdtemp(path.join(os.tmpdir(), "noon-hosted-mesa-"));
  const outfile = path.join(dir, "raw.json");
  let launches = 0;
  try {
    const fakeLaunch = async () => {
      launches++;
      const index = launches - 1;
      return {
        version: () => EXPECTED_BROWSER,
        newContext: async () => ({
          newPage: async () => ({
            goto: async () => {},
            evaluate: async () => ({
              ...valid(index === 0 ? "Google SwiftShader" : index === 1
                ? "Mesa llvmpipe" : "Google SwiftShader"),
            }),
          }),
          close: async () => {},
        }),
        close: async () => {},
      };
    };
    const record = await acquire({ launch: fakeLaunch, outfile, plan });
    assert.equal(launches, 3);
    assert.equal(record.acquisitionComplete, true);
    assert.equal(record.anyMesaEligible, true);
    assert.deepEqual(record.cases.map(x => x.decision.status),
      ["usable-software-webgl2-backend", "usable-software-webgl2-backend", "not-confirmed"]);
    assert.equal(record.qualification, false);
    assert.equal(record.mergeApproval, false);
    assert.equal(record.performanceAcceptance, false);
    const saved = JSON.parse(await readFile(outfile, "utf8"));
    assert.deepEqual(saved.cases.map(x => x.id), PINNED_MODES.map(x => x.id));
    await assert.rejects(() => acquire({ launch: fakeLaunch, outfile, plan }), /EEXIST/);
    assert.equal(saved.cases.length, 3);
  } finally { await rm(dir, { recursive: true, force: true }); }
});

test("tampered or reordered preregistered plan is rejected before any browser is launched", async () => {
  const reversed = { ...plan, scenarios: [...plan.scenarios].reverse() };
  await assert.rejects(() => acquire({
    launch: () => { throw Error("should not launch"); }, outfile: "unused.json", plan: reversed,
  }));
});
