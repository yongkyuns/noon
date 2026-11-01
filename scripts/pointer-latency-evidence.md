# Offline pointer-latency evidence validator (#2298)

Experimental qualification tooling only. This is not engine code, a browser
collector, a performance budget, or acceptance evidence. `docs/architecture.md`
remains the architecture authority. Neither #2296's product branch nor its
source-pinned running qualification needs these files.

Run deterministic schema/aggregation tests:

```sh
node --test scripts/pointer-latency-evidence.test.mjs
node --test scripts/pointer-latency-observer.test.mjs
```

The module exports `summarizePointerLatency(trace)` and
`comparePointerLatency(baseline, candidate)`. The tests show complete synthetic
inputs. They exercise the reducer; they do not simulate browser qualification.

`observePointerLatency(ExecutionWorkerClient, { clockId, capacity })` is a separate
Window-only qualification observer. Install it before endpoint message handlers and gestures, label each
physical action with `observer.action(name)`, then retain `snapshot(metadata)`.
It joins a synchronous canvas DOM occurrence to its exact normalized input and
MessagePort request, then matches replies and deferred controls by their issued
IDs and unchanged sequence strings. It preserves the product promise, messages,
errors and input behavior. Suppressed DOM occurrences, failed posts, ambiguous
joins and bounded-record loss remain explicit. `stop()` removes its own hooks.

The observer records presentation receipts but leaves control-to-presentation
association unknown: the existing reply supplies no exact required receipt.
Its reply reading precedes the existing client message handler. These limitations,
observer overhead, full provenance and a frozen scoring plan must be addressed
before its traces can qualify performance; an installed observer is not a gate.

## Evidence requirements

Each trace identifies an exact source commit/tree/runtime build, original fixture
SHA-256, instrumentation SHA-256, and predeclared measurement-plan hash. Profile
identity includes browser/version, backend, render host, build profile/flags,
device, workload, viewport, and pixel ratio. The named scoring window must be
frozen before collection. The reducer checks matching declarations; it cannot
attest whether the source actually produced them or whether a plan was declared
before observing results. Source/build receipts and a published plan own that
provenance.

All timestamp fields must come from `performance.now()` in ONE Window navigation,
identified by `clockId`. No worker timestamp, epoch reconstruction, AudioContext
clock, clock clamping, or negative-duration normalization is accepted. Separate
baseline/candidate navigations can have different clock IDs and absolute origins.
Only equal-duration, identically named phases are compared.

Every physical press/release attempt needs a globally unique logical `attemptId`
that is meaningful across paired runs, its endpoint-generation identity, and the
original `domMs`. An observed post additionally needs its exact `requestId` and
`postedMs`. A worker reply needs the matching same-Window arrival `replyMs`.
Request IDs may repeat across endpoint generations, not within one generation.
Do not create DOM-to-request links by selecting the nearest timestamp. A future
collector must establish these links or retain the input without downstream
measurements.

`outcome` is `accepted`, `rejected`, `cancelled`, `error`, or `pending`.
`applied` is an explicitly observed boolean or unknown. `controlReplyMs` is the
Window arrival of the matching control-result acknowledgement, NOT the worker's
internal application time. It may be the original pointer reply or a subsequent
deferred-Play completion. A true `applied` value requires this acknowledgement.

`presentedMs` is usable only with equal, complete `expectedReceipt` and
`observedReceipt` values: session, sequence, presentation, and view revision.
The same endpoint, timestamp, and receipt must exist in the raw `presentations`
stream. An acknowledgement may arrive before the control reply. Never choose the
next/latest presentation or assume an arbitrary received frame belongs to the
control. The current production control reply does not by itself establish all
such links; missing links remain unknown. This reducer does not change that API.

## Reporting rules and limitations

Score attempts collected in `[startMs, endMs)`; score their downstream observations
only through `endMs`. A late known outcome remains recorded but its latency is
right-censored. Keep all raw attempts, including warmup, rejection, cancellation,
missing replies, late outcomes, failed runs, and aborted runs. Empty distributions
are null, not zero. Report press and release separately as well as the combined
cohort. Every conditional latency distribution has an explicit count alongside
the complete attempt/outcome denominators. Quantiles use the usual even-sample
median and nearest-rank p95.

Presentation-arrival intervals are grouped by endpoint, session, and view. They
are Window-observed acknowledgement intervals, NOT GPU-present intervals or an
FPS claim. Sequence gaps stay in the report and their intervals are retained.
Duplicate observations or regressing per-group time/identity fail validation
rather than being sorted or deduplicated away. No interval is joined across a
new endpoint, session, or view. Publication-issue timestamps and worker-internal
application timing remain outside this reducer's current scope.

Dropped records or observer errors remain visible in individual summaries and
prevent paired summaries. Different profiles, phases, window lengths, fixture,
instrumentation, or plan pins prevent pairing. Unmatched logical actions are
reported instead of intersecting them away. A failed/aborted run remains failed/
aborted in a descriptive pair; it is not excluded to improve the candidate.

The output always says `acceptance: "not-evaluated"`. No threshold or performance
pass is defined. #2298 still needs source-matched paired
Firefox/WebGL2 measurements, observer-overhead assessment, complete stage joins,
and an explicitly agreed acceptance rule. Existing 15-case audio reports lack
these timestamps and must not be converted to invented latency evidence. Preserve
the prior adverse 300-ms and mixed 282-ms traces and #2300's unresolved anchor
failure independently of this tooling.
