# Immutable geometry resource arena

Heavy geometry must not be copied through semantic objects, animation snapshots,
compiled objects, frame state, and renderer caches. `GeometryResourceArena`
introduces stable immutable resource ownership while keeping cheap analytic
primitives inline.

## Storage split

`StoredGeometry` has two classes:

- circle/rectangle/line payloads remain inline and copy cheaply;
- vector paths use a small `GeometryResourceHandle` into the arena.

Promoting a legacy `GeometryRef::VectorPath` clones its command payload exactly
once into arena ownership. Every later semantic or execution snapshot can copy
the small handle rather than the path command array.

## Versioned replacement

A resource handle contains stable `GeometryId` plus a version. Replacing the
payload keeps the ID but increments the version, invalidating old handles and
renderer caches deterministically. Removal similarly invalidates the current
version without renumbering other resource IDs.

This gives live editing explicit cache/lifetime semantics instead of relying on
pointer identity or cloned geometry values.

## Instrumentation

The arena reports live resource count and deterministic retained/path-command
byte estimates. Tests exercise 100,000 references to one 10,000-command path,
50,000 semantic snapshots of a promoted path, replacement invalidation, and
stable unrelated IDs.

## Migration boundary

The current `GeometryRef` and frame representation remain available while the
architecture reset lands in parallel. Wave 2 stable execution slots (#58) and
renderer packing (#59) can consume `StoredGeometry`/resource handles without
changing the arena contract. Text/math resources (#65) should follow the same
immutable-resource lifetime model rather than embedding glyph outlines directly
in object snapshots.
