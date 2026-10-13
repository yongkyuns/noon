# Temporary #2047 integration evidence

This file is confined to the one-shot application branch. It is not included in the product commit, which must be parented directly on `8dff4426796a94a753defe4f9935deb0a356cb0f`.

Current local validation of the prepared product edits:

- 39 Python tests pass: 29 exception-snapshot tests plus 10 tests executing the actual worker Python body under a minimal host.
- 124 JavaScript tests pass: snapshot, client, transport, presentation, package-ordering and worker-source contracts.
- Seven real Chromium 144.0.7559.96 DOM checks pass using actual Python-produced diagnostics and the actual presentation modules supplied in memory: closed details, exact textarea source selection, disclosure, edit invalidation, literal/inert message content, generic fallback, and no unhandled browser errors. This is not Pyodide/renderer recovery qualification.

Original hosted preflight run 38410886373 failed seven outdated interpreter-mock expectations after compilation was moved before dependency loading. Those assertions have been updated, not removed; dependency ordering, failure cleanup and source-execution count remain checked. A later local source-compilation regex expected the old anonymous filename call; it now verifies the explicit filename and exactly one compiler invocation. Original failed evidence is retained.

Full hosted preflight, architecture checks, and real browser integration remain required. A subsequent ref update alone did not produce a visible workflow run; this evidence commit also requests a new ordinary push validation event on the same branch.
