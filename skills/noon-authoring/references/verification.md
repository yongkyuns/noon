# Verification

Treat code execution, semantic correctness, rendered appearance, and mathematical
correctness as separate checks. A scene can run successfully while displaying the
wrong starting object, dropping a label, or using incorrect timing.

Inspect frames at activation/completion boundaries and inside each significant
transition. Check object count/membership, source-versus-target identity, endpoints,
layout, clipping, glyphs, and explanatory pacing. Include an initial frame and the
intended final state, but do not reject a deliberately empty frame after removal.

Record the actual source, Noon revision, execution host/backend, and requested and
observed scene times when the available tooling exposes them. Null or unavailable
metadata is not a license to guess. Compare raster evidence only within the
qualified backend and tolerance policy; do not require universal cross-GPU bit
identity or widen thresholds to make a failing scene pass.

For repository changes, the existing validation entrypoints are:

```bash
bash scripts/check.sh fast
bash scripts/check.sh full
```

The fast gate is not a substitute for browser/parity checks. With the existing
browser build and its dependencies available, the maintained tutorial corpus can
be exercised with:

```bash
node scripts/manim-tutorial-smoke.mjs
```

This is a corpus test, not a general source-to-video CLI or a security sandbox.
Do not claim that invoking it verifies a newly authored source file unless that
file was actually included in the run. CI declarations and manifest labels are
not current test results. Report the exact tests and images actually inspected.

Do not execute untrusted scene code against unrestricted host files, credentials,
or networking. AST parsing, a browser worker, local MCP transport, and a timeout
are not by themselves sufficient isolation. The bounded agent runner is separate
work; do not present today's repository harness as a hardened remote service.
