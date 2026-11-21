# Mark up a paused lesson

In the playground, select **Comment** above a scene with prepared playback.
Noon pauses the preview so you can drag a rectangle right on the scene, click
to drop a pin, or press Enter to mark the center. Write your note under the
preview and select **Add to source**. The note is appended to the current Python
draft as ordinary comments, including the captured time, image-relative bounds,
and hash of the source that produced the frame. Existing edits stay in the draft
and playback stays paused.

Use **Save .py** to save the annotated scene. An agent working on that source can
find `Noon feedback (pending)` at the end of the file. Use the recorded time and
region to find the visual, render a revision, and keep the note pending until
the author accepts it. The bounds start at the top-left of the captured image.

## Download the original evidence

Choose **Download evidence** if the reviewer needs the original frame. The
bundle contains the PNG, selection, comment, scene time, and displayed Run source.

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

The viewer checks the Run, time, and shared Rust presentation receipt around
the canvas copy. The browser writes the Python source comment. The browser smoke
covers adding and saving a comment, preserving an unapplied edit, and revising
**One Extra Column** at the captured frame.
