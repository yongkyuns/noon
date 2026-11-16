# Comment on a playing lesson

In the playground, run a scene and select **Comment** above the preview.
The viewer pauses through the existing playback controls, presents the same
scene time, and opens a frozen copy of that picture. Click the part you mean and
type one comment. The field receives focus immediately. Clicking another point
moves the pin; Enter on the picture provides a keyboard-accessible center pin.

**Download feedback** creates one file containing the comment, clicked point,
original picture, exact authored time, observed publication/build identities,
and the source of the displayed Run. It does not substitute unsaved editor text.
The file includes source code, so share it only with an intended reviewer/agent.
Nothing is sent to a service automatically. Closing leaves playback paused.

The agent imports the download from the trusted checkout:

```bash
node tools/noon-mcp/bin/noon-feedback.mjs import-live \
  --input noon-feedback-example.json --output review
node tools/noon-mcp/bin/noon-feedback.mjs check \
  --feedback review/feedback.json --source scene.py
```

Import reuses the existing frame-feedback validator. It verifies source/image
hashes, dimensions, bounds, and receipt consistency before writing `feedback.json`,
`frame.png`, `source.txt`, and `capture.json` into a new directory. Changed source
is stale, not silently retargeted. The existing `compare` command accepts a fresh
candidate capture after the edit. A record stays pending until a person reviews
the correction; a successful render alone does not resolve it.

## Current boundaries

- The viewer route admits prepared playback after the initial source attachment.
  It may capture while preparation is still progressing. Observed/non-prepared
  playback, startup, recovery, and unavailable publication receipts fail visibly.
- The initial version records one point/comment per download. Reopen Comment to
  capture another moment. It does not edit an object, identify a semantic target,
  keep a persistent comment board, or automatically ask an AI to change source.
- The capture freezes the exact accepted Run and publication. The modal prevents
  review clicks from triggering scene interactions and isolates Run shortcuts.
  Concurrent seek, replacement, cancellation, or an inconsistent receipt rejects
  the capture instead of guessing its time. There is no alternate clock.
- Current feedback format limits remain: 0–600 seconds, source up to 1,000,000
  UTF-8 bytes, 4,000-byte comment, PNG up to 8 MiB / 16,777,216 pixels / 8192 per axis.
- This is optional browser-host capture/persistence code. Shared Rust continues
  to own source execution, scene state, authored time, playback, and narration.
  File metadata establishes consistency, not authenticity of an untrusted upload.

## Verification

`node --test web/playground-feedback-capture.test.mjs` checks existing-player
coordination, publication races, cancellation, bounds, coordinates, and export.
The tooling package includes producer-to-import and tampering regression tests
in `test/live-feedback.test.mjs`, run by its normal `npm test`.

`node tools/noon-mcp/test/live-feedback-browser.mjs` exercises the real browser
form with an explicitly synthetic canvas/endpoint. It does not execute Noon or
qualify engine playback. Its desktop/mobile images and downloaded note are host
UI evidence only. A real built-playground test is still required for the complete
Rust/WASM playback-to-capture path.
