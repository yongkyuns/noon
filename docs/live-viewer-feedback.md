# Mark up a paused lesson

In the playground, select **Comment** above a scene with prepared playback.
Noon pauses at the displayed frame. Drag a rectangle around the relevant area,
click to drop a pin, or press Enter on the picture to mark the center. Write one
comment and select **Download feedback**. Closing the panel leaves playback paused.

Attach the download to your agent chat and identify the scene you want to
revise. The file contains the original PNG, the selection, the comment, the
exact scene time, and the source that produced the displayed Run. The bounds
are fractions of the image, measured from its top-left corner.

## Agent handoff

From the Noon checkout with the locked `tools/noon-mcp` dependencies installed:

```bash
node tools/noon-mcp/bin/noon-feedback.mjs import-live \
  --input viewer-feedback.json --output review
node tools/noon-mcp/bin/noon-feedback.mjs check \
  --feedback review/feedback.json --source scene.py
```

Import validates the source and image digests, dimensions, region, and playback
receipt. Inspect `review/frame.png` in context and use the region from
`review/feedback.json` to zoom in. Read `review/source.txt` for the source that
produced the image. If `check` reports stale source, inspect the intervening
edits and locate the corresponding moment before changing the scene.

Render the revised source in a fresh `noon-preview` run at the relevant time.
Use the resulting schema-2 manifest to record the comparison. Supply the sample
index if the relevant frame is beyond the initial frame.

```bash
node tools/noon-mcp/bin/noon-feedback.mjs compare \
  --feedback review/feedback.json --candidate revised-preview/manifest.json \
  --sample 1 --source scene.py --output comparison
```

Show the author `comparison/before.png` and `comparison/after.png`, and explain
the focused edit. The comparison stays pending for the author to accept. If
timing changed, choose the corresponding moment from the new run explicitly.

The viewer checks that the same Run, time, and presentation receipt bracket
the canvas copy. Capture is available once prepared playback has a matching
receipt. The browser smoke exercises rectangle capture, an unapplied source
edit, import, and a before-and-after revision in **One Extra Column**.
