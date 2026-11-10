# Troubleshooting

Begin with an unchanged example on the exact host you intend to use. Record the
source revision, operating system, browser or interpreter, and the full diagnostic
when reporting a problem.

## The preview is blank

Opening the preview loads its interface; **Run** starts source execution. Check
the status and expand diagnostics. The first execution needs network access for
Pyodide resources. Confirm the full editor can run the same unchanged example.
An unavailable GPU backend, worker startup failure and a Python exception require
different fixes; do not infer success from the presence of an empty canvas.

## A preview disappeared

Documentation previews are released when the tab is hidden, another preview is
opened, or Close is pressed. This avoids leaving several rendering contexts and
Python workers active. Reopen and Run explicitly. For sustained editing, use the
full editor instead of the documentation preview.

## Import or native build fails

Use the pinned Rust toolchain and supported CPython version. The development
extension command needs the correct `PYTHONPATH`; an installed matching wheel
should not. A wheel for another interpreter, ABI or platform will not become
compatible by renaming it. Consult [native Python details](native-python.html)
and the candidate guidance in [the reference](reference.html#initial-release-candidates).

## Text or equations differ from expectations

Check the text constructor, font resources and whether you are using Typst or
LaTeX. LaTeX preparation has its own host requirements. Start from the matching
text example in [the reference](reference.html), not a different renderer's
implicit asset or font behavior.

## Seek, restart, or export is unavailable

A live callback, unrecorded input, unfinished source continuation or unsupported
replay domain may prevent the operation. This is not solved by forcing a seek
button to appear. Check [playback](playback.html), the source's dependencies, and
the export profile's documented admission requirements.

## MP4 creation fails

Confirm FFmpeg is installed with the needed encoder, the graphics driver supports
capture, and the destination policy allows the output. Test a finite unchanged
lesson first. Consult [native export details](export-details.html) for cancellation,
limits and output handling.

## How to report a useful issue

Include a minimal source, exact revision or candidate filename, host details,
expected result and complete diagnostic. A screenshot helps explain visual
behavior; a short reproducible program helps distinguish semantics, host and
renderer failures. Never include access tokens or private paths unnecessarily.
