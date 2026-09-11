# Verification

Treat code execution, semantic correctness, rendered appearance, and mathematical
correctness as separate checks. A scene can run successfully while displaying the
wrong starting object, dropping a label, or using incorrect timing.

Inspect frames at activation/completion boundaries and inside each significant
transition. Check object count/membership, source-versus-target identity, endpoints,
layout, clipping, glyphs, and explanatory pacing. Include an initial frame and the
intended final state, but do not reject a deliberately empty frame after removal.

Record the actual source, Noon revision, execution host/backend, requested and
published scene times, and retained artifact/build identities when the tooling exposes
them. Null or unavailable metadata is not a license to guess. Compare raster evidence
only within the qualified backend and tolerance policy; do not require universal
cross-GPU bit identity or widen thresholds to make a failing scene pass.

## Newly authored source

When `NOON_PREVIEW_RUNTIME_CONFIG` has been prepared, prefer the shared isolated CLI
or MCP path for source-specific evidence. For a file:

```bash
node tools/noon-mcp/bin/noon-preview.mjs \
  --source scene.py --output noon-preview-output \
  --time 1 --time 1.5 --time 3
```

Verify `manifest.json` and inspect the corresponding PNGs. The manifest binds each
retained frame to the submitted source hash, observed runtime build identity,
requested/published time, actual backend, PNG hash, byte length, and opaque session.
The CLI and configured MCP tools delegate to the same `AgentPreviewService` and runner.

For MCP, use `noon_open_scene`, forward-only `noon_sample_frames`, optional
`noon_inspect`, and `noon_close_scene`. Do not reuse a handle after close or
cancellation. A tool result containing an image is evidence only for that returned
artifact and provenance; it is not proof for an unrendered revision of the source.

The local preview boundary has qualified Docker containment for the supported Linux
path: network disabled, read-only root/mounts, private namespaces, explicit seccomp,
all outer capabilities dropped, no-new-privileges, bounded CPU/PID/memory/tmpfs,
process-level cancellation, and cleanup. Pyodide is pinned into the preview image so
the runtime does not require network access. This is **not** a hardened remote service
or a sandbox for a malicious checkout: the configured Noon checkout, runtime setup,
and local Docker daemon remain trusted inputs.

## Repository changes and corpus checks

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

This is a corpus test, not proof that a different source file was rendered. CI
declarations and manifest labels are not current test results. Report the exact tests,
backend, source revision, artifact hashes, and images actually inspected.

## Reproducible source package

A checkout-bound authoring package can be built and verified without installing npm
dependencies or running lifecycle hooks:

```bash
node scripts/package-noon-agent.mjs --output /new/path/noon-agent-bundle
node scripts/package-noon-agent.mjs --verify /new/path/noon-agent-bundle
```

The bundle contains the maintained skill and canonical source capability inventory,
plus hashes for the runner/MCP source it delegates to. Those source hashes are not a
substitute for the actual loaded worker/WASM identity: preview artifacts obtain that
identity from the running browser worker and retain it with each frame.

Do not execute untrusted scene code against unrestricted host files, credentials, or
networking. Capability discovery and bundle verification execute no scenes. If the
qualified local preview boundary is unavailable, report that limitation rather than
silently falling back to unrestricted source execution.
