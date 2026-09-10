import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { chmod, mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { PassThrough } from "node:stream";
import test from "node:test";

import {
  buildDockerCreateArgs,
  DockerIsolatedProcess,
  normalizePreviewLimits,
  validateDockerInspection,
} from "../src/preview-isolation.mjs";

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
    "--read-only", "--network=none", "--ipc=private", "--user=pwuser",
    "--cap-drop=ALL", "--security-opt=no-new-privileges=true", "--pids-limit=128",
    "dst=/noon/web,readonly", "dst=/noon/tools/noon-mcp,readonly",
  ]) assert.ok(joined.includes(required), `missing ${required}`);
  assert.equal(args.some((arg) => arg.startsWith("--cap-add=")), false,
    "Chromium sandbox compatibility must not grant capabilities to the outer container");
  assert.equal(args.some((arg) => arg.startsWith("--pid=")), false,
    "Docker default PID namespace must remain private");
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
  assert.throws(() => buildDockerCreateArgs(config, ["node"], { containerName: "other-container" }), /owned Noon preview/);
});

test("post-create inspection rejects isolation downgrades before workload start", () => {
  const secure = {
    Image: config.imageId,
    Config: { User: "pwuser" },
    HostConfig: {
      NetworkMode: "none", IpcMode: "private", PidMode: "", ReadonlyRootfs: true, Privileged: false,
      Memory: limits.memoryBytes, MemorySwap: limits.memoryBytes,
      NanoCpus: limits.cpuCount * 1e9, PidsLimit: limits.pids, ShmSize: limits.shmBytes,
      CapDrop: ["ALL"], SecurityOpt: ["no-new-privileges=true", `seccomp=${config.seccompProfile}`],
      Tmpfs: {
        "/work": `rw,nosuid,nodev,mode=1777,size=${limits.workBytes}`,
        "/tmp": `rw,nosuid,nodev,mode=1777,size=${limits.tmpBytes}`,
      },
    },
    Mounts: [
      { Source: config.webRoot, Destination: "/noon/web", Type: "bind", RW: false },
      { Source: config.toolingRoot, Destination: "/noon/tools/noon-mcp", Type: "bind", RW: false },
    ],
  };
  assert.equal(validateDockerInspection(secure, config), true);
  assert.throws(() => validateDockerInspection({ ...secure, Image: `sha256:${"b".repeat(64)}` }, config), /image identity/);
  assert.throws(() => validateDockerInspection({ ...secure, HostConfig: { ...secure.HostConfig, NetworkMode: "default" } }, config), /network must be none/);
  assert.throws(() => validateDockerInspection({ ...secure, HostConfig: { ...secure.HostConfig, PidMode: "host" } }, config), /PID namespace/);
  assert.throws(() => validateDockerInspection({ ...secure, HostConfig: { ...secure.HostConfig, PidMode: "container:other" } }, config), /PID namespace/);
  assert.throws(() => validateDockerInspection({ ...secure, HostConfig: { ...secure.HostConfig, PidsLimit: 0 } }, config), /PID limit mismatch/);
  assert.throws(() => validateDockerInspection({ ...secure, HostConfig: { ...secure.HostConfig, Tmpfs: {} } }, config), /tmpfs/);
  assert.throws(() => validateDockerInspection({ ...secure, Mounts: [{ Source: config.webRoot, Destination: "/noon/web", Type: "bind", RW: true }] }, config), /read-only bind mount/);
  assert.throws(() => validateDockerInspection({
    ...secure,
    Mounts: secure.Mounts.map((entry) => entry.Destination === "/noon/web" ? { ...entry, Source: "/other/web" } : entry),
  }, config), /expected read-only bind mount/);
});

test("concurrent close calls await the same authoritative container cleanup", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "noon-preview-docker-test-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const docker = path.join(root, "docker");
  await writeFile(docker, [
    "#!/usr/bin/env node",
    "setTimeout(() => process.exit(0), 150);",
    "",
  ].join("\n"));
  await chmod(docker, 0o755);

  const attached = new EventEmitter();
  attached.stdin = new PassThrough();
  attached.stdout = new PassThrough();
  attached.stderr = new PassThrough();
  attached.exitCode = null;
  attached.signalCode = null;
  attached.kill = (signal) => {
    attached.signalCode = signal;
    queueMicrotask(() => attached.emit("close", null, signal));
    return true;
  };
  attached.stdin.once("finish", () => setTimeout(() => {
    if (attached.exitCode === null && attached.signalCode === null) {
      attached.exitCode = 0;
      attached.emit("close", 0, null);
    }
  }, 20));

  const process = new DockerIsolatedProcess(
    { ...config, dockerExecutable: docker },
    "a".repeat(64),
    attached,
  );
  const first = process.close("explicit close");
  const second = process.close("concurrent close");
  assert.equal(second, first, "all close callers must retain the same cleanup promise");
  let settled = false;
  second.finally(() => { settled = true; }).catch(() => {});
  await new Promise((resolve) => setTimeout(resolve, 30));
  assert.equal(settled, false, "close must not settle before Docker removal completes");
  const result = await first;
  assert.equal(result.closed, true);
  assert.equal(result.cleanup.removed, true);
  assert.equal(await process.close("later close"), result);
});
