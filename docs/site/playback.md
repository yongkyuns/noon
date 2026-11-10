# Playback and interaction

The website, full editor, and native viewer are hosts of the same engine, but not
every source supports the same playback operations.

## Run, replay, and restart

**Run** executes a source program. Replay uses a qualified prepared scene or its
retained history; it must not silently rerun source. Restart is available only
when the viewer has a replayable owner. For a callback-dependent or input-driven
source, the UI may truthfully report that seeking or replay is unavailable.

Use the full playground's status and diagnostics when a capability is gated.
The documentation preview reuses those controls rather than inventing its own
clock, seeking logic, or runtime state.

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

Select **Open interactive preview**, then **Run**. The compact frame puts the
canvas before any optional diagnostic detail. The full editor remains one link
away. Opening a second preview closes the first; closing the preview or hiding
the page removes its browsing context and requires a new explicit start later.
This intentionally favors bounded resource use over preserving an embedded draft.

The website theme affects interface controls. It does not recolor the authored
scene. The full editor remains a separate application with its own presentation.

## Audio and live input

Browser audio requires an appropriate user gesture. Preparation should not
produce sound; picture and audio share the engine's authored time. Supported
finite audio declarations, callback restrictions and export combinations are
listed in [browser integration](web-runtime.html#background-audio-and-scene-navigation).
Native Python does not automatically inherit every browser audio capability.
