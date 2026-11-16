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

## Captured frame feedback

Use `noon_annotate_frame` with the owned `session`, retained `artifactId`,
`interval: {start,end}`, `observation`, `desiredChange`, optional `preserve`, and
optional `region: {x,y,width,height}`. Region coordinates are fractions of the
captured image, from its top-left corner. The interval must contain the published
capture time. Preserve the returned pending feedback record and PNG before the
live artifact expires or the session closes. Do not fabricate provenance or
substitute a screenshot from a different run.

For persisted schema-2 preview captures, use the offline CLI from the checkout:

```bash
node tools/noon-mcp/bin/noon-feedback.mjs annotate \
  --preview preview-output/manifest.json --sample 1 \
  --source scene.py --input note.json --output review
node tools/noon-mcp/bin/noon-feedback.mjs check \
  --feedback review/feedback.json --source scene.py
node tools/noon-mcp/bin/noon-feedback.mjs compare \
  --feedback review/feedback.json --candidate revised-preview/manifest.json \
  --sample 1 --source scene.py --output comparison
```

Put the same interval/region/observation/desiredChange/preserve fields in
`note.json`; omit session and artifactId for the offline route. Indices start at
zero. Each output directory must be new. `annotate` saves the original PNG and
source with the record. `check` verifies that evidence and reports source changes
as stale (exit 2); current source exits 0, invalid evidence exits 1. Run it before
editing, and inspect the original source snapshot if the association is stale.

After the focused correction, capture a fresh run before using `compare`. Choose
the candidate frame explicitly, since narration or timing edits may move the
relevant moment. The comparison retains separate before/after source/build/run/
time identities and source snapshots. It remains pending review with an unverified
interval association; it does not retarget the original region or resolve feedback.
Inspect both frames and review actual playback when the comment concerns time or
sound. Report remaining uncertainty, not an automatic success from rendering.

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
