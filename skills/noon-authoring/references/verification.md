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

## Faithful scene ports

Identify the upstream scene and exact source revision or release. Use the
original footage or frames as the visual reference when available. Credit the
original authors in the source and any gallery entry, and follow the source license.

Preserve the scene's mathematical meaning, text and formulas, composition,
styling, camera motion, animation order, timing, holds, and final state. Keep
authored randomness, audio, and interactions where they affect the result.
Use equivalent Noon operations that preserve the visible behavior.

Compare the Noon render with the reference at matching scene times and framing.
Inspect the opening, transition interiors, and final state, then watch the full
sequence at playback speed. Resolve material differences before calling the
port complete. If a required Noon capability or reference is unavailable, report
the exact gap and any differences you observed.

## First-play seeking

Open the exact scene in a fresh session of its normal target player. On the
initial run, seek forward and backward between two buffered times as soon as
the first playable interval is available. For incremental preparation, do this
while later history is still being prepared. Check that the range grows as
history arrives and that the player presents the expected state at the selected
time. For a fully prepared timeline, test once the controls become available.

For checked-in gallery scenes that prepare incrementally, use
`requireInitialSeek` in `scripts/playground-profile-expectations.mjs` to guard
initial admission. Exercise the actual browser scrubber on that same run and
record the selected and presented times. Use the preview CLI for forward-only
frame inspection.

If first-play seeking is impossible for a faithful scene, report the exact
blocker and when seeking becomes available, if ever. If you could not exercise
the player, report that first-play seeking is unverified.

## Repository validation

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
