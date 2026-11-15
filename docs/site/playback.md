# Playback and interaction

The website, full editor, and native viewer are hosts of the same engine, but not
every source supports the same playback operations.

## Run, replay, and restart

**Run** executes a source program. Replay uses a qualified prepared scene or its
retained history; it must not silently rerun source. Restart is available only
when the viewer has a replayable owner. For a callback-dependent or input-driven
source, the UI may truthfully report that seeking or replay is unavailable.

Use the full playground's status and diagnostics when a capability is gated.
The embedded view uses the same execution path but hides the full playback
toolbar. Open the playground for explicit playback and inspection controls.

## Lesson sections

Use `self.next_section("Name")` between ordinary Python `play` and `wait` calls
to label the current authored time. Completed replayable lessons can display
section markers and previous/next controls. These markers are navigation cues,
not interpolation keyframes.

## Keyboard controls

Focus the playback panel, not the source editor, before using shortcuts. The
canonical [browser integration guide](web-runtime.html#lesson-key-points)
documents Space for play/pause, Left/Right for seeking, Page Up/Page Down for
section navigation and Home for restart. Focused buttons, sliders and editors
retain their normal keyboard behavior.

## Documentation preview lifecycle

Embedded scenes start when the page loads. **View** shows the live canvas;
**Code** exposes the same Python editor as the gallery. Source edits use the
existing typing debounce, composition handling, run replacement and diagnostics.
No second clock, source buffer or hot-reload implementation is involved.

View changes preserve source and runtime state, and examples do not close one
another. Navigation retires the embedded contexts. Back/forward restoration
retains the editor draft and resumes it using the ordinary gallery Run action.
Code edits execute source again; changing View/Code does not.

The website theme affects the controls, never authored scene colors. The code
editor retains the gallery's syntax theme. Runtime loading and error messages
appear only when necessary; the normal View shows the scene and View/Code switch.

## Audio and live input

Browser audio requires an appropriate user gesture. Preparation should not
produce sound; picture and audio share the engine's authored time. Supported
finite audio declarations, callback restrictions and export combinations are
listed in [browser integration](web-runtime.html#background-audio-and-scene-navigation).
Native Python does not automatically inherit every browser audio capability.

In an embedded **View**, focus the scene and use **Space** to pause or play,
**Home** to restart, and the arrow keys to seek when the scene supports playback.
These shortcuts use the same controller as the full gallery.
