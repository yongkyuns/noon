# Stable semantic store

The authoritative authoring model needs identity semantics that are independent
of compiled/runtime vector positions. `SemanticStore` introduces that layer
without requiring existing `SceneDefinition` consumers to migrate in the same
PR.

## Stable identity

`SemanticNodeId` is a generational slot handle. Removing one node never renumbers
another node. Reusing a vacant slot increments its generation, making stale
handles invalid instead of accidentally rebinding them to a new object.

The current `SceneDefinition` can be imported through a compatibility adapter.
Its `ObjectId` remains available as a lookup key during migration, but it is not
the long-term execution/render index.

## Families are references, not transform ownership

A semantic node may be an object or a family. Family membership is an ordered,
many-to-many relation:

```text
family A ─┐
          ├── object X
family B ─┘
```

This is intentional. Manim submobjects/groups may alias the same object, so Noon
must not model a group as a conventional unique-parent transform tree. A compiler
may later optimize an exclusive family into hierarchical execution, but that is
a specialization and must not change semantic aliasing.

Cycles are rejected. Cycle validation walks only the candidate member's reachable
family subgraph; ordinary unrelated insertion/removal does not scan the scene.

## Source identity

Nodes may carry a `SourceIdentity`:

- an explicit authoring key; or
- a source location plus lexical construction path.

The store enforces uniqueness. Wave 2 hot-reload reconciliation (#64) can use
these hooks without changing object identity again.

## Complexity contract

- insert: O(1) amortized;
- unrelated lookup: O(1) expected;
- remove: O(direct family edges), never O(total scene nodes);
- add/remove family edge: O(local edge work), plus reachable-subgraph cycle check;
- no operation renumbers unrelated nodes.

Tests include 100,000 semantic objects and assert that deleting an unrelated
node preserves the tail handle and writes only the deleted slot.

## Migration boundary

This PR intentionally leaves `SceneDefinition`, timeline compilation, renderer
packing, and Python wrappers in place. They consume adapters until Wave-2 stable
execution slots (#58) and the Python semantic-handle migration (#61). This keeps
Wave-1 storage work mergeable independently of the timeline and renderer tracks.
