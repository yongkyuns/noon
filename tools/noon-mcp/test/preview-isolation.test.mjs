import assert from "node:assert/strict";
import test from "node:test";

import { buildDockerCreateArgs, normalizePreviewLimits, validateDockerInspection } from "../src/preview-isolation.mjs";

const limits = normalizePreviewLimits();
const config = {
  imageId: `sha256:${"a".repeat(64)}`,
  seccompProfile: "/home/user/.cache/noon/seccomp.json",
  webRoot: "/repo/web",
  toolingRoot: "/repo/tools/noon-mcp",
  limits,
};

test("Docker create contract is fail-closed and exposes only bounded read-only assets", () => {
  const args = buildDockerCreateArgs(config, ["node", "/noon/tools/noon-mcp/test/preview-isolation-probe.mjs"]);
  const joined = args.join(" ");
  for (const required of [
    "--read-only", "--network=none", "--ipc=private", "--pid=private", "--user=pwuser",
    "--cap-drop=ALL", "--security-opt=no-new-privileges=true", "--pids-limit=128",
    "dst=/noon/web,readonly", "dst=/noon/tools/noon-mcp,readonly",
  ]) assert.ok(joined.includes(required), `missing ${required}`);
  assert.equal(joined.includes("src=/repo,dst=/noon"), false, "whole checkout must not be mounted");
  assert.equal(args.at(-3), config.imageId);
});

test("untrusted paths and unbounded resource requests are rejected", () => {
  assert.throws(() => buildDockerCreateArgs({ ...config, webRoot: "/repo,evil/web" }, ["node"]), /without NUL or comma/);
  assert.throws(() => normalizePreviewLimits({ memoryBytes: 1 }), /memoryBytes/);
  assert.throws(() => normalizePreviewLimits({ pids: 10000 }), /pids/);
  assert.throws(() => normalizePreviewLimits({ cpuCount: Infinity }), /cpuCount/);
  assert.throws(() => buildDockerCreateArgs({ ...config, imageId: "latest" }, ["node"]), /content-addressed/);
  assert.throws(() => buildDockerCreateArgs(config, []), /command/);
});

test("post-create inspection rejects isolation downgrades before workload start", () => {
  const secure = {
    Config: { User: "pwuser" },
    HostConfig: {
      NetworkMode: "none", ReadonlyRootfs: true, Privileged: false,
      Memory: limits.memoryBytes, MemorySwap: limits.memoryBytes,
      NanoCpus: limits.cpuCount * 1e9, PidsLimit: limits.pids, ShmSize: limits.shmBytes,
      CapDrop: ["ALL"], SecurityOpt: ["no-new-privileges=true", `seccomp=${config.seccompProfile}`],
    },
    Mounts: [
      { Destination: "/noon/web", Type: "bind", RW: false },
      { Destination: "/noon/tools/noon-mcp", Type: "bind", RW: false },
    ],
  };
  assert.equal(validateDockerInspection(secure, config), true);
  assert.throws(() => validateDockerInspection({ ...secure, HostConfig: { ...secure.HostConfig, NetworkMode: "default" } }, config), /network must be none/);
  assert.throws(() => validateDockerInspection({ ...secure, HostConfig: { ...secure.HostConfig, PidsLimit: 0 } }, config), /PID limit mismatch/);
  assert.throws(() => validateDockerInspection({ ...secure, Mounts: [{ Destination: "/noon/web", Type: "bind", RW: true }] }, config), /read-only bind mount/);
});
