# Interactive runtime and live authoring

## Purpose

Noon should combine three different upstream strengths rather than copy one Manim implementation wholesale:

- use **ManimCE v0.21** as the semantic/API and visual/timing compatibility reference for supported Manim behavior;
- use **current 3b1b/manim (ManimGL)** as an interaction and live-authoring capability reference;
- preserve Noon's own retained, language-neutral, worker-isolated execution architecture and stronger `O(active + dirty + visible)` scalability goals.

The key design conclusion is:

> Borrow ManimGL's interaction affordances and live-authoring immediacy, but do not borrow its mutable-Python-object ownership model.

This document separates interactive runtime behavior from live authoring, defines the additional session-state layer Noon needs, and records renderer lessons from current ManimGL.

Related trackers: #56, #64, #68, #69, #70, #569, #835, #846, #847.

## Upstream architecture comparison

### Current 3b1b/manim

Current ManimGL is best understood as a **live mutable Python graphics runtime with a retained/versioned GPU backend**.

Its `Scene` owns live Python `Mobject` instances. Interaction, animations and updaters mutate those same objects directly. The interactive loop repeatedly advances scene time, updates all scene mobjects, and captures the current scene. Mouse/camera state is represented by live mobjects as well.

That ownership model makes interactive code extremely direct:

```text
window event
    |
    v
Python Scene handler / updater
    |
    v
mutate live Mobject graph
    |
    v
renderer resolves current graph
```

There is no semantic/runtime/renderer ownership boundary to cross. That is a major reason features such as camera dragging, object grabbing, live updaters and embedded interactive authoring can feel immediate.

The current ManimGL renderer is nevertheless substantially retained:

- each drawable mobject maps to a persistent renderer-side `Drawing`;
- CPU structured arrays carry versions so unchanged data is not recopied unnecessarily;
- compatible consecutive drawings are merged into runs;
- shared GPU buffers are reused;
- once the draw topology remains stable for a small number of frames, the renderer can capture it in a WebGPU render bundle;
- the bundle keeps draw order/pipeline/buffer ranges while reading current buffer contents when replayed, so transform/color/camera data can keep changing without rebuilding command topology.

This is a useful separation of **stable draw topology** from **mutable data**, even though the Python scene graph is still resolved each frame.

### Manim Community Edition

ManimCE preserves the same broad Python-mutable object heritage but is designed primarily as a general animation/video authoring framework.

Its scene update path still calls mobject updaters from the Python scene graph. Its OpenGL renderer updates a frame by walking the current scene mobjects and rendering each eligible object. CE also has interactive OpenGL support, queued interaction actions, file-triggered reruns and callbacks, but interactivity is less central to the architecture than in 3b1b/manim.

For Noon, CE should remain the compatibility oracle because its public behavior/version can be pinned and differentially tested. It should not be treated as the target realtime execution architecture.

### Noon

Noon intentionally separates:

```text
language authoring
      |
      v
canonical semantic scene
      |
      v
lowered retained execution
  /          |          \
 timeline  reactive VM  host slots
      \       |         /
          atomic deltas
              |
              v
       retained renderer
```

This is more complex than mutating a live Python graph, but it gives Noon important properties that Manim's direct-mutation model cannot guarantee cheaply:

- deterministic direct seek for native/seekable behavior;
- language-neutral semantics shared by Python/Rust/JavaScript;
- worker isolation and a renderer that does not synchronously depend on arbitrary Python;
- local runtime mutation and dirty propagation;
- retained spatial queries;
- `O(active + dirty + visible)` as an explicit scaling target;
- immutable shared resource ownership rather than repeated geometry/text transport.

Noon should not weaken these properties to imitate ManimGL's implementation.

## Two distinct interaction requirements

### Interactive runtime

A finished scene reacts to live input while it is running.

Examples:

- pointer-following objects;
- drag constraints;
- hover/click behavior;
- keyboard-controlled state;
- sliders and controls;
- camera pan/zoom;
- interactive graph/geometry manipulation.

This is primarily the domain of #69 plus the reactive VM, spatial index and host-callback fallback.

### Live authoring

The author edits and explores a scene while preserving useful current state.

Examples:

- pause at a frame and inspect objects;
- select an object by clicking it;
- move or resize it directly;
- move the camera without advancing animation time;
- modify source and rerun a block/scene;
- preserve compatible playhead/control/selection state after re-execution;
- undo a direct manipulation or semantic edit;
- inspect coordinates, bounds and identity;
- execute additional authoring commands against the current semantic scene.

This is not merely another native input source. It requires explicit **session state** above the semantic scene. #846 owns that contract.

## Interactive session state

Noon should introduce a conceptual `InteractiveSession` layer. Exact type/module names may differ.

It owns ephemeral interaction/editor state that should not become persistent semantic scene content:

```text
InteractiveSession
  selection
  hover target
  pointer capture
  active tool / manipulation mode
  drag origin / reference transform
  camera-navigation gesture state
  temporary property drivers
  undo/transaction grouping
  inspector state
  editor overlay projection
```

The semantic scene remains authoritative for authored content. Selection rectangles, grab handles, crosshairs and inspector labels should normally be editor overlays, not semantic mobjects inserted into the user's scene.

This differs deliberately from current ManimGL `InteractiveScene`, where selection/highlight/helper graphics are ordinary mobjects and are updated through the same mutable scene.

## Input, event and frame ordering

#69 already distinguishes sampled state from discrete events. The full interaction scheduler should make the ordering explicit.

Recommended shape:

```text
DOM/native collection
      |
normalize coordinates and units
      |
      +-- sampled state -> coalesce to latest applicable sample
      |
      `-- discrete events -> preserve ordered occurrences
      |
      v
pointer capture / hit-test / session state machine
      |
      v
native reactive dependency closure
      |
      v
property-driver arbitration
      |
      v
host callback phase when required
      |
      v
atomic semantic/runtime commit
      |
      v
bounds/culling update
      |
      v
presentation
```

Sampled pointer movement may be coalesced when intermediate positions have no semantic significance. Pointer/key press and release, commits and other discrete events may not be silently lost.

## Time model

Interactive authoring needs more than one notion of time.

At minimum distinguish:

- **timeline time**: position in the authored animation;
- **presentation/wall time**: realtime pacing and deadlines;
- **input event ordering/time**: ordering of external interaction relative to a committed frame.

A paused timeline must still allow:

- camera movement;
- pointer/hover/selection changes;
- direct manipulation;
- inspector/overlay changes;
- source edits and semantic transactions;
- reactive control changes whose semantics are defined to operate while paused.

These actions must not implicitly advance the animation timeline merely because frames are presented.

## Direct manipulation and property ownership

A direct manipulation can conflict with an animation, reactive binding or host updater already driving the same property.

A mutable-Python system often resolves this implicitly through execution order. Noon requires an explicit contract.

Conceptually, manipulation should acquire a temporary interaction driver/lease over the affected property set. The public contract must define what happens on release. Useful policies include:

- cancel and restore the pre-drag driven value;
- commit a new semantic/base value and let the existing driver continue from its defined model;
- replace/remove the previous driver;
- rebase the remaining animation where the animation type explicitly supports that operation.

Do not choose one global implicit "last writer wins" rule for all of these cases.

The generic multi-driver phase semantics remain owned by #56. #846 defines the user-facing acquisition/release semantics for direct manipulation on top of that foundation.

## Hit testing and selection

Interaction should consume the existing retained spatial index rather than scan semantic or renderer objects.

Recommended flow:

```text
pointer position
      |
      v
ExecutionSpatialIndex broad phase
      |
ordered candidates
      |
optional precise geometry hit test
      |
      v
session hover / selection target
```

Selection order should preserve painter order. The interaction layer must not introduce another bounds tree.

## Live source re-execution

ManimGL achieves live authoring cheaply because an embedded interpreter directly sees and mutates the live Python scene.

Noon should provide similar ergonomics through **semantic reconciliation**, not shared mutable Python authority:

```text
edited source / authoring command
       |
       v
new draft semantic generation
       |
       v
stable-source-identity reconciliation (#64)
       |
       v
local semantic mutation transaction
       |
       v
localized execution relowering / state migration
```

Compatible state should be migrated deliberately, including as applicable:

- timeline playhead;
- user controls/signals;
- session selection/hover when identities remain valid;
- editor camera/navigation state;
- compatible execution slots;
- host state according to its declared replay/migration class.

Stale or ambiguous selection/interaction identity should be cleared or surfaced explicitly rather than rebound heuristically to the wrong object.

## Demand-driven paused presentation

When the timeline is paused and there is no scene, input, session-overlay, camera or renderer dirtiness, Noon should not keep doing meaningful frame work merely because a browser `requestAnimationFrame` callback fires.

Target behavior:

```text
paused + no dirty work
    -> engine sleeps / no evaluation
    -> no GPU presentation required

event / edit / camera change / timeline resume
    -> wake affected execution path
    -> present resulting frame
```

This strengthens the existing `static frame CPU work ~ O(0)` invariant into a product-level interaction requirement.

## Renderer lesson from current ManimGL

The most transferable renderer idea is **stable draw-topology reuse**.

Current ManimGL can bundle a stable draw sequence while still updating the contents of buffers read by that bundle. Noon should benchmark the analogous design instead of assuming it is either necessary or useless. #847 owns the measurement/decision gate.

Potential Noon model:

```text
resident renderer state
      |
      v
visible ordered draw plan
      |
      +-- topology/pipeline/binding/layout identity -> reusable command plan
      |
      `-- transforms/style/reveal/camera -> mutable buffers only
```

A reusable WebGPU command/bundle plan should not be invalidated by ordinary value changes when all referenced buffers/bindings/ranges remain valid. It should be invalidated by changes such as:

- visible/painter topology changes;
- pipeline/material binding changes;
- buffer relocation/layout generation changes;
- renderer/device generation changes;
- draw-count/range changes baked into the command sequence.

WebGL2 remains on the normal draw path.

Because Noon already has analytic batching, mega meshes and retained draw preparation, this is a **measurement question first**, not a mandatory redesign.

## Preview versus output quality

Interactive presentation resolution is a renderer/presentation policy, not semantic state.

The editor may render at current canvas size/DPR/MSAA policy while offline/high-quality export renders at the requested output resolution. Changing preview resolution must not alter semantic geometry, timeline state or object identity.

Future video export should similarly pipeline GPU readback/encoding where possible rather than impose a synchronous readback stall on every frame.

## Product/reference policy

Use the upstream projects for different purposes:

```text
ManimCE v0.21
    -> semantic/API/visual/timing compatibility oracle

current 3b1b/manim
    -> interaction and live-authoring capability reference
    -> renderer optimization ideas to benchmark

Noon
    -> authoritative implementation architecture
    -> deterministic retained execution
    -> language neutrality
    -> browser-native interaction
    -> worker isolation
    -> O(active + dirty + visible)
```

Noon does **not** target source-level compatibility with current ManimGL's experimental interactive APIs unless an API is separately adopted as a Noon feature.

## Required capability gates

### Native runtime interaction

- pointer/keyboard/wheel/control behavior can update a scene with zero Python frame callbacks when expressible through native signals/reactivity;
- high-frequency sampled input reevaluates only its dependency closure;
- discrete input ordering is deterministic and tested;
- hit testing uses the retained spatial index;
- arbitrary host interaction remains available through the host callback protocol without blocking renderer deadlines.

### Direct manipulation/session state

- hover/selection/pointer capture are session state, not user-scene mobjects;
- drag begins from an explicit hit/capture decision and ends transactionally;
- property conflicts with timeline/reactive/host drivers use explicit arbitration semantics;
- undo groups one manipulation gesture as one user action;
- paused direct manipulation does not advance timeline time;
- editor overlays do not pollute semantic painter/family identity.

### Live re-execution

- local source changes reconcile by stable identity rather than whole-scene replacement on the normal path;
- compatible playhead/control/session state survives re-execution;
- stale/ambiguous identities fail predictably;
- unchanged semantic/execution/resource identities remain resident;
- static deterministic playback still requires no Python participation after authoring.

### Demand-driven rendering

- paused, clean scenes perform no meaningful runtime/render work;
- camera/input/editor-overlay changes wake only the necessary path;
- renderer presentation can occur without advancing timeline time.

### Draw-plan reuse benchmark

- benchmark stable large scenes under transform/style/camera-only changes;
- measure CPU command encoding/submission separately from runtime/preparation/upload cost;
- compare ordinary WebGPU encoding against stable draw-plan/render-bundle reuse;
- verify exact painter/render equivalence and correct invalidation;
- adopt the optimization only when the measurements justify its complexity.

## Architectural invariant

A useful review question for interaction work is:

> Is this state part of the authored scene, part of executable scene behavior, or only part of the current editor/interaction session?

Those are three distinct ownership classes and should not be collapsed merely because a mutable Python implementation can represent all three as `Mobject`s.