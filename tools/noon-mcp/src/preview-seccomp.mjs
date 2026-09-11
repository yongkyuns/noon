const CHROOT = "chroot";
const CAP_SYS_CHROOT = "CAP_SYS_CHROOT";

function plainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function emptySelector(value) {
  return plainObject(value) && Object.keys(value).length === 0;
}

/**
 * Derive Noon's Docker seccomp profile from the pinned Playwright profile.
 *
 * Playwright's upstream profile conditionally admits chroot only when Docker
 * grants CAP_SYS_CHROOT to the container. Noon intentionally starts the outer
 * container with --cap-drop=ALL, so Docker omits that rule before Chromium can
 * enter its own user namespace. Chromium's nested sandbox then has a
 * namespace-scoped CAP_SYS_CHROOT, but seccomp still rejects the syscall.
 *
 * Admitting chroot at the syscall filter does not grant CAP_SYS_CHROOT. The
 * outer process remains capability-free; the kernel permission check therefore
 * still rejects its chroot attempts. The derived rule only lets Chromium use
 * the capability it acquires inside the nested user namespace.
 */
export function deriveNoonPreviewSeccompProfile(upstream) {
  if (!plainObject(upstream) || !Array.isArray(upstream.syscalls)) {
    throw new TypeError("Playwright seccomp profile must contain syscall rules");
  }

  let conditionalChrootRules = 0;
  const syscalls = [];
  for (const rule of upstream.syscalls) {
    if (!plainObject(rule) || !Array.isArray(rule.names)) {
      throw new TypeError("Playwright seccomp syscall rule must contain names");
    }
    if (!rule.names.includes(CHROOT)) {
      syscalls.push(structuredClone(rule));
      continue;
    }

    const includes = rule.includes;
    const caps = plainObject(includes) && Array.isArray(includes.caps) ? includes.caps : [];
    if (rule.action !== "SCMP_ACT_ALLOW" || !caps.includes(CAP_SYS_CHROOT)) {
      if (rule.action === "SCMP_ACT_ALLOW" && emptySelector(includes)) {
        throw new Error("Playwright seccomp profile already admits chroot unconditionally");
      }
      throw new Error("unexpected Playwright chroot seccomp rule");
    }
    conditionalChrootRules += 1;

    const remainingNames = rule.names.filter((name) => name !== CHROOT);
    if (remainingNames.length > 0) syscalls.push({ ...structuredClone(rule), names: remainingNames });
  }

  if (conditionalChrootRules !== 1) {
    throw new Error(`expected exactly one capability-gated Playwright chroot rule, found ${conditionalChrootRules}`);
  }

  syscalls.unshift({
    names: [CHROOT],
    action: "SCMP_ACT_ALLOW",
    args: [],
    comment: "Allow Chromium nested user-namespace sandbox chroot; outer container retains zero capabilities",
    includes: {},
    excludes: {},
  });

  return { ...structuredClone(upstream), syscalls };
}
