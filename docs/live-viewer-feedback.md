# Mark up a paused lesson

In the playground, select **Comment** above a scene with prepared playback or
a loaded finite soundtrack scene such as **Double the input**.
Noon pauses the preview so you can drag a rectangle right on the scene, click
to drop a pin, or press Enter to mark the center. Write your note under the
preview and select **Add to source**. The note is appended to the current Python
draft as ordinary comments, including the captured time, image-relative bounds,
and hash of the source that produced the frame. Existing edits stay in the draft
and playback stays paused.

With prepared playback, releasing a rectangle checks the paused frame for up to four
overlapping 2D objects. The comment includes their types, visible text, and
Python creation or caller lines when available. Dense scenes can yield a partial
list, and the marked region still gives the reviewer a place to look.

Use **Save .py** to save the annotated scene. An agent can use the `Noon feedback`
comments, recorded times, and object hints to address the notes in the Python
source. Coordinates start at the top-left of the captured image. Use **Run** to
play the revised scene.
