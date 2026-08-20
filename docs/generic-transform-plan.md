# Generic object Transform

## Goal

Broaden Noon `Transform` from a path-only convenience into one atomic, language-neutral animation that interpolates a source scene object toward a detached target object snapshot while preserving the source object's stable identity.

This slice keeps Python out of the frame loop and preserves the current fixed-topology GPU path morph path.

## Representation

Use the existing timeline/track identity system instead of inventing a second animation namespace.

Add a first-class timeline property:

```text
Property::Transform
TrackValues::Object {
    from: ObjectSnapshot,
    to: ObjectSnapshot,
}
```

`ObjectSnapshot` contains no scene identity:

```text
ObjectSnapshot
  geometry: GeometryRef
  transform: Transform2D
  style: Style
```

The target can therefore be authored without attaching it to `SceneDefinition`. `TrackId` remains the stable animation identity used by live patches and browser authoring identity reconciliation.

## First supported geometry contract

Supported:

- identical geometry -> transform/style interpolation only;
- `VectorPath -> VectorPath` -> existing fixed-topology morph representation;
- sequential transforms on the same source object when they do not overlap.

Rejected for this slice:

- cross-kind primitive geometry (`Circle -> Rectangle`, `Line -> Path`, etc.);
- differing analytic primitive parameters (`Circle(r=1) -> Circle(r=2)`) until parameter interpolation is compiled explicitly;
- vector-path transforms whose fill topology would require fill retessellation;
- vector-path stroke-width changes, because current fixed-topology stroke vertices bake width into geometry;
- overlapping generic Transform tracks on the same object.

These restrictions are intentional: unsupported cases fail explicitly instead of falling back to per-frame tessellation.

## Runtime semantics

`Transform` is atomic. At progress `u` the runtime evaluates:

- translation: linear interpolation;
- rotation: linear interpolation;
- scale: linear interpolation;
- fill/stroke colors: component interpolation when representable;
- opacity: linear interpolation;
- stroke width: linear interpolation only when the geometry contract permits it;
- geometry:
  - identical geometry remains static;
  - path-to-path uses one compiled/cached source+target morph geometry and sets the existing per-object morph progress to `u`.

At `u=0` and `u=1`, object transform/style are exactly the authored snapshots.

The source `ObjectId` never changes. The detached target never appears in the scene object table.

## Precedence with low-level tracks

Apply generic `Transform` before narrower property tracks. Explicit `Position`, `Rotation`, `Opacity`, `Reveal`, or `Morph` tracks therefore override the corresponding generic Transform channel when both are active.

This preserves composability and avoids hidden Transform precedence.

## Sequential transforms

For:

```python
scene.play(Transform(a, b), start_time=0, duration=1)
scene.play(Transform(a, c), start_time=1, duration=1)
```

the second Transform snapshots `b` as its `from` state. The runtime selects the latest started Transform for the object, so direct seek and sequential playback produce the same state.

The Python authoring layer rejects overlapping generic transforms for the same source object in this first slice.

## Renderer behavior

For path morphing, compile/cache a geometry representation equivalent to:

```text
source VectorPath + target VectorPath
             -> fixed source/target path mesh
```

The morph progress remains instance-only. Steady-state frames must not:

- run correspondence planning;
- flatten paths;
- tessellate;
- upload geometry.

A transform may cause a geometry-cache transition once when a new sequential path pair becomes active.

## Python API

Introduce detached `Mobject` snapshots plus ergonomic constructors while retaining existing `scene.circle`, `scene.rectangle`, `scene.line`, and `scene.path` helpers.

Target form:

```python
shape = scene.path(source, stroke=BLUE, fill=None)
target = Path(
    target_path,
    position=(1.0, 0.5),
    rotation=0.4,
    stroke=RED,
    fill=None,
)

scene.play(Transform(shape, target), duration=2.0)
```

Compatibility:

```python
Transform(shape, target_vector_path)
```

continues to mean geometry-only path Transform with source transform/style preserved.

## Required validation

Core / IR:

- detached target serializes without an `ObjectId`;
- Transform remains a normal stable `TrackId`;
- JSON round-trip preserves snapshots;
- malformed type/property combinations fail deterministically.

Compiler:

- same-geometry transform accepted;
- path-to-path transform accepted;
- unsupported cross-geometry transform rejected;
- path stroke-width-changing transform rejected;
- dynamic-property classification marks generic Transform.

Runtime:

- exact source/midpoint/target transform and style;
- direct seek equals forward stepping;
- sequential transforms are continuous and deterministic;
- source identity is preserved;
- target is not added to frame objects;
- explicit low-level property tracks override generic Transform;
- path Transform drives morph progress without changing reveal.

Renderer:

- steady path Transform updates instance data only;
- geometry cache misses are zero after the pair has been prepared;
- sequential pair transition incurs at most one new geometry preparation for the new pair;
- no per-frame geometry upload.

Python:

- detached targets do not increase scene object count;
- VectorPath compatibility remains;
- sequential non-overlapping transforms lower to two atomic Transform tracks;
- overlapping generic transforms are rejected;
- authored source stable key/ID remains unchanged.

## Follow-ups

1. analytic primitive parameter interpolation;
2. GPU-side dynamic path stroke width;
3. fill morph topology policy;
4. `ReplacementTransform` and `TransformFromCopy` lifecycle policies;
5. child matching / `TransformMatchingShapes`;
6. shortest-arc rotation option and custom path functions.
