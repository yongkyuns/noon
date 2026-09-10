import assert from "node:assert/strict";
import { test } from "node:test";

import { deriveNoonPreviewSeccompProfile } from "../src/preview-seccomp.mjs";

function upstreamFixture() {
  return {
    defaultAction: "SCMP_ACT_ERRNO",
    syscalls: [
      {
        names: ["clone", "setns", "unshare"],
        action: "SCMP_ACT_ALLOW",
        args: [],
        includes: {},
        excludes: {},
      },
      {
        names: ["chroot"],
        action: "SCMP_ACT_ALLOW",
        args: [],
        includes: { caps: ["CAP_SYS_CHROOT"] },
        excludes: {},
      },
      {
        names: ["mount"],
        action: "SCMP_ACT_ALLOW",
        args: [],
        includes: { caps: ["CAP_SYS_ADMIN"] },
        excludes: {},
      },
    ],
  };
}

test("derives one unconditional chroot admission without granting a capability", () => {
  const upstream = upstreamFixture();
  const original = structuredClone(upstream);
  const derived = deriveNoonPreviewSeccompProfile(upstream);

  assert.deepEqual(upstream, original, "derivation must not mutate the pinned upstream profile");
  const chrootRules = derived.syscalls.filter((rule) => rule.names.includes("chroot"));
  assert.equal(chrootRules.length, 1);
  assert.equal(chrootRules[0].action, "SCMP_ACT_ALLOW");
  assert.deepEqual(chrootRules[0].includes, {},
    "Docker must not omit chroot merely because the outer container has cap-drop=ALL");
  assert.deepEqual(chrootRules[0].excludes, {});
  assert.deepEqual(
    derived.syscalls.find((rule) => rule.names.includes("mount")),
    original.syscalls.find((rule) => rule.names.includes("mount")),
    "unrelated capability-gated syscalls must remain gated",
  );
});

test("fails closed when the pinned upstream chroot contract changes", () => {
  const missing = upstreamFixture();
  missing.syscalls = missing.syscalls.filter((rule) => !rule.names.includes("chroot"));
  assert.throws(() => deriveNoonPreviewSeccompProfile(missing), /exactly one capability-gated/);

  const alreadyOpen = upstreamFixture();
  alreadyOpen.syscalls.find((rule) => rule.names.includes("chroot")).includes = {};
  assert.throws(() => deriveNoonPreviewSeccompProfile(alreadyOpen), /already admits chroot unconditionally/);

  const wrongCapability = upstreamFixture();
  wrongCapability.syscalls.find((rule) => rule.names.includes("chroot")).includes = { caps: ["CAP_SYS_ADMIN"] };
  assert.throws(() => deriveNoonPreviewSeccompProfile(wrongCapability), /unexpected Playwright chroot/);
});
