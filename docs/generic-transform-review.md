# Independent review checklist: generic Transform

This review track intentionally starts from the pre-implementation branch and the public architecture contract. It should evaluate the implementation as an external consumer rather than reproducing implementation assumptions.

## Semantic correctness

- Source `ObjectId` must remain stable for the entire Transform and after completion.
- Detached target must never become a rendered scene object unless explicitly added.
- `u=0` must equal the authored source snapshot exactly.
- `u=1` must equal the authored target snapshot exactly for every supported channel.
- Direct seek and sequential forward playback must agree byte-for-byte at representative times.
- Two non-overlapping transforms on one object must be continuous at their boundary.
- Overlapping generic transforms must have an explicit policy; silent last-writer behavior is unacceptable in the first slice.
- Narrow property tracks must have documented precedence relative to generic Transform.

## Geometry correctness

- Path-to-path Transform must reuse the fixed-topology morph planner and GPU interpolation.
- No per-frame flattening, correspondence planning, tessellation, or path-vertex upload.
- New source/target path pair may prepare geometry once at activation.
- Unsupported cross-kind primitive geometry must fail before rendering rather than disappear silently.
- Path stroke-width changes must not accidentally trigger per-frame geometry-cache churn.
- Fill changes must not imply unsupported fill morph topology.

## Runtime / performance

- Transform should be one atomic runtime/timeline definition, not N unrelated frontend tracks.
- Runtime work should be O(active transforms) and independent of completed history during forward playback.
- Steady path Transform should mark only the affected object dirty.
- Geometry-cache misses after warm-up should be zero for a stable pair.
- Existing 600/1,000/3,000 stress benchmark should continue to batch into the expected reusable path meshes.

## IR / live editing

- Stable TrackId semantics must survive JSON round-trip.
- Existing scene documents without generic Transform must remain decodable.
- Live track replacement/removal must rebuild the correct Transform state at the current playhead.
- Browser identity reconciliation must not need a second animation identity namespace.

## API ergonomics

- Detached target construction should be natural in Python.
- Existing `Transform(source, VectorPath)` should remain valid as geometry-only compatibility sugar.
- Generic target style/transform should snapshot by value; mutating a target after scheduling must not retroactively alter the scheduled Transform.
- Adding a detached target to a scene should be explicit.

## Failure modes to look for

- accidental target object insertion;
- source geometry permanently mutated at authoring time;
- `TrackValues` ownership changes causing hidden clones in the frame loop;
- transform/style interpolation allocating each frame;
- geometry pair rebuilt every frame;
- path mesh cache key changing because style fields that should be instance-only leak into geometry identity;
- transform track overriding an explicit position/rotation/opacity track unexpectedly;
- target endpoint visually correct but frame semantic state still holding the source object snapshot;
- sequential transform #2 using base source state instead of transform #1 target state;
- seek at exact transform boundary choosing the wrong pair;
- patching base style/transform while a Transform is active producing nondeterministic results.

## Acceptance bar

Do not approve solely on green compilation. The verification suite must demonstrate endpoint exactness, seek parity, sequential continuity, precedence, unsupported-case rejection, and renderer cache/upload invariants.
