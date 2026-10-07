# Visual-effects contract fixtures

M0 work for [#1897](https://github.com/yongkyuns/noon/issues/1897).
`docs/architecture.md` is the sole architecture/roadmap authority; this is an
acceptance specification and reference-fixture description, not another roadmap.
Inspection base: `9028672c1691d3a318c87da50e8b08e0802245f0`. The subsequent API
review checked the unchanged public authoring hooks at `6f7b7f008c45288bf3b92a417078b907d5e4ef5c`.

**M0 design review:** the named leaf-glow contract below is the implementation
baseline. This is a design decision, not enabled effects or a merge qualification.
The authored Rust subset is implemented and tested through the actual shared
owners. The complete paired source review is in `authoring_review.py` and
`authoring_review.rs`; it includes proposed M1–M4 operations and is **syntax-checked,
not executed as a supported cross-language product**. Do not add mock operations
to make those sources appear to run. The existing native declaration example
remains the executable proof of the supported M0 subset.

M0's four design/reference deliverables are separate from the later runtime and
GPU implementation gates in #1897. Required PR checks and human acceptance remain
separate from this recorded review. No rendered-glow, direct-seek, pulse-lifecycle,
cross-host binding execution or physical-performance result follows from declaration tests.
The separate leaf-adapter section records implemented binding source and its
validation boundary without reclassifying the future-feature review programs.

## Implemented shared Rust boundary

`crates/noon-core/src/object_state/glow.rs` contains production, renderer-independent
values: `Glow`, `GlowUpdate`, `GlowRadius`, `Pixels`, `GlowSource`, typed parameter
errors, and `PreparedGlowUpdate`. The existing object-state module exports them.
They use Noon's existing `Color`, not a parallel color type or Python validator.
No new crate, dependency, identity allocator, scene store, timeline or shader exists.

`Glow` is a complete validated value with private fields. `GlowUpdate::default()`
is an empty partial request, not a reset to appearance defaults. A whole request
is validated before a replacement value is returned. The following is actual Rust
API, also covered by a compilable rustdoc example:

```rust
use noon_core::{Glow, GlowParameterError, GlowUpdate, Pixels};

fn parameters() -> Result<(), GlowParameterError> {
    let halo = Glow::new(
        GlowUpdate::default().radius(Pixels(12.0)).intensity(0.25),
    )?;
    let brighter = GlowUpdate::default().intensity(1.4).apply_to(halo)?;
    assert_eq!(brighter.radius(), halo.radius());
    assert_eq!(halo.intensity(), 0.25);
    Ok(())
}
```

These are inert shared inputs, not wrapper-owned mutable attachment state.
`request.prepare(captured)` accepts an explicit coherent activation-time value;
it does not obtain that value from a store, activate a driver, or check a lease.
`PreparedGlowUpdate::sample(mapped_alpha)` returns a **partial update**, never an
old complete appearance snapshot. Intensity-only sampling therefore cannot write
radius/color/source. An explicitly supplied unchanged parameter still has a write
channel; omitted parameters do not. Callers remain responsible for real attachment
generation, publication and channel ownership before applying the returned writes.

The Rust tests feed the new interpolation through **existing**
`resolve_composition_schedule`, `CompositionTimeMap` and `RateFunction` APIs.
They cover literal linear/pulse samples, lag and duration rescaling, nested maps,
explicit capture, discrete rejection and omitted-field preservation. They are
parameter/timing-component tests, not `Scene::play` or Runtime lifecycle tests.
The positive rustdoc checks exports; a compile-fail rustdoc rejects boolean intensity.

## Implemented Rust attachment and target-editing surface

`semantic_store/effects.rs` owns ordered names and definitions. Attachments are a
non-renderable variant of the **existing generational semantic node arena**, not
new IDs, fake drawable objects, child-family nodes or a second scene registry.
`create_effect` and `update_effect` extend the ordinary transaction vocabulary;
preflight validates names, owner references, values and duplicate writes before
commit. Removing an owner retires its attachments through the retained reverse
reference index. Removing/re-adding a name never revives a stale handle.

`Scene::set_glow`, `add_effect`, `get_effect`, `set_effect`, `remove_effect` and
`remove_glow` now exist. Mobject target/pre-execution edits use the same checked
preparation. Names and exact handles are distinct selectors; a foreign or stale
handle cannot fall back to name lookup. `EffectHandle::authored_definition` is
explicitly an authored observation, not an effective-runtime getter. Dropping the
wrapper does not detach the binding. Rust mutation results follow existing checked
Scene/Mobject conventions rather than pretending Python return syntax is mandatory.

The following is a runnable Rust contract example, not a proposed signature or
mock scene. Its complete source is `crates/noon/examples/effect_authoring_contract.rs`:

```rust
use noon::{AnimationOptions, Scene};
use noon::effects::GlowUpdate;

fn authored_contract() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let dot = scene.circle(0.08)?;
    scene.add(&dot)?;
    scene.set_glow(&dot, GlowUpdate::default().intensity(0.25))?;
    let mut target = dot.target_editor()?;
    target.shift(2.0, 0.0)?;
    target.set_effect("glow", GlowUpdate::default().intensity(1.4))?;
    let movement = scene.declare_transform_to(
        &dot, &target, AnimationOptions::new().run_time(1.5),
    )?;
    assert_eq!(movement.options()?.run_time, Some(1.5));
    assert_ne!(dot.get_effect("glow")?.node_id(), target.get_effect("glow")?.node_id());
    assert!(scene.execution_session().is_err()); // explicitly unavailable before M1
    Ok(())
}
```

Ordinary object/target/family copying creates independent attachment identities
inside the same transaction; aliased leaves are copied once. Names and order are
retained. Unrelated geometry/layout and painter membership are unchanged.
Nonstructural parameter changes emit `EffectParameter`, distinct from structural
`EffectAttachment` impacts, and touch one attachment slot. Name lookup/removal is
linear in that owner's effect count, not total scene size. Nodes without effects
allocate no attachment collection. The store maintains an allocator-derived count
for constant-time capability rejection, not a second collection/identity authority.

**Current unsupported boundary:** the M0 compiler rejects execution of an
**effect-bearing store**, even for neutral attachments or detached target copies.
This deliberately conservative O(1) guard prevents appearance from being silently
discarded; M1 must replace it with real effect lowering rather than remove it
without implementation. Live setters use the ordinary publication path and reject
new effect requests before changing either semantic or runtime publication.
State-only `become`/replacement is also rejected while effect declarations exist;
its capture currently has no attachment correspondence. Those conservative gates
are explicit limitations, not claims of supported live animation or restoration.

The actual attachment slice is leaf-only and named. Family propagation, composed
filters, unnamed attachments, effective effect queries, GlowPulse, ordinary effect
animation/channel ownership, and effective Python queries remain future integration work.
The leaf declaration adapter is implemented below; effect execution remains gated.
No native or browser setter silently skips unavailable rendering.

### Parameter-addressed authored transactions

Explicit write ownership is `(attachment identity, parameter)`, not the complete
attachment. Two updates in the same ordinary semantic transaction may change
intensity and radius independently, together with an object's translation. Two
writers to the same parameter fail atomically even when they request equal values.
Empty patches own no channels but still validate the attachment handle. Scalar,
color, radius, and source writes retain the same typed validation rules.

`GlowUpdate::parameters()` enumerates only explicitly supplied channels without
allocation. Preflight composes disjoint updates against the staged value. Commit
emits `SemanticMutationImpact::EffectParameter` only for changed parameters;
unchanged explicit values still participate in conflict admission without dirtying
unrelated parameters. Failures preserve all earlier ordinary/effect state and the
scene revision. This is real authored-transaction behaviour, not yet runtime
animation leases or effect rendering. Existing execution support guards remain.

## Implemented Python declaration adapter

`Glow`, `Pixels`, and `EffectHandle` are exported from `noon`, through its existing
lazy public-export mechanism. The supported calling convention is **`from noon
import Glow, Pixels`**, not `from noon.effects import ...`: the current Python API
is a module, not a package. This follows the existing top-level shape/style/native
extension convention without inventing an import alias or subpackage. Rust retains
its idiomatic `noon::effects` module; import spelling need not be identical.

`web/python/_noon_effects.py` owns signatures, numeric/type coercion and fluent
receiver returns only. `Glow` holds an immutable Rust definition, not a Python
parameter dictionary. `EffectHandle.authored_definition` asks its Rust handle for
a checked authored value each time; it is deliberately not named an effective
runtime getter. Copies of a reference preserve that reference; copies of an owning
Mobject remain independent through Rust's existing copy operation. Failed writes
have no Python rollback implementation. Temporary argument objects are freed on
both success and failure; Python wrapper collection never removes an attachment.

`set_glow`, `add_effect`, `get_effect`, `set_effect`, `remove_effect`, and
`remove_glow` route to the existing object handle or the existing live context.
Names versus explicit handles select distinct typed boundary calls so stale/foreign
handles cannot silently resolve by name. `None` is an omitted setter/constructor
field, like ordinary optional style arguments; unknown keywords, strings/booleans
as numbers, and unsupported schemas fail. Rust owns numeric ranges/defaults and
source parsing. The bridge validates RGBA before narrowing f64 to the shared f32
Color so out-of-range inputs cannot become valid through rounding.

The existing `.animate` builder calls these normal target methods unchanged:
`dot.animate.shift(RIGHT * 2).set_glow(intensity=1.4).set_effect("accent", intensity=.6)`.
There is no custom effect builder, timing helper, Python attachment registry or
per-frame effect callback. This specifies declaration routing, not proven cross-host playback.

Omitted scope denotes a leaf binding in this slice. Explicit `each`, `composed`,
and `view` scopes and group receivers remain unsupported; there is no implicit
`family` broadcast or many-owner handle. This preserves the scope decisions below.

The complete paired leaf example is
`web/python/examples/effect_authoring_contract.py`, alongside the native Rust
example of the same name. It verifies source/target independence, named updates,
pixel units, stale-handle rejection and explicit effect-execution rejection. After
removing the declarations, its normal wait exercises ordinary source continuation.
The existing `shared-authoring-smoke.mjs` runs it through actual Pyodide/WASM/Rust;
its final no-effect frame is **not** an enabled-glow visual reference. Success must
be recorded from the real browser run, not inferred from the mocked routing tests.
Native CPython execution is not claimed by this existing Pyodide host adapter.

Native Rust tests compare parsed adapter inputs to direct Rust values/authoring,
and exercise the real live-session publication guard without changing time,
identity or revision on rejection. Python unit tests isolate routing, return values,
coercion and cleanup using spies; they do not implement a fake effect scene as
parity evidence. Group/composed/view scope and callback mutations still reject
explicitly before a raw authored write. Temporary `GlowPulse` remains unexported
until ordinary runtime activation/ownership exists.

## First-glow calling conventions

The ordinary API stays object-centred. Do not make effect handles another required
kind of animated scene object or introduce `play_effect`, `GlowTo` or `AnimateGlow`.

| Intent | Python design | Rust design / existing owner |
| --- | --- | --- |
| Persistent canonical glow | `obj.set_glow(**patch) -> obj` | Scene/live `set_glow(&obj, GlowUpdate)`, fallible coherent publication |
| Add a named treatment | `obj.add_effect(Glow(...), name="accent") -> obj` | Scene/live attachment creation through the same semantic transaction |
| Inspect a binding | `obj.get_effect("accent") -> bound handle` | typed generational lookup, not a copied state object |
| Update one binding | `obj.set_effect("accent", **patch) -> obj` | Scene/live typed parameter update; no whole-object ownership |
| Animate motion + appearance | `obj.animate.shift(...).set_effect("accent", intensity=...)` | existing target editor and ordinary declaration/composition lowering |
| Temporary emphasis | `GlowPulse(obj, ..., run_time=...)` | ordinary shared animation intent, not an independent sampler/driver |
| Remove | `obj.remove_glow()` / `obj.remove_effect(name_or_handle)` return obj | structural Scene/live operation, no time advancement |

This supersedes the earlier handle-returning `add_effect`, `obj.effect(...)`, and
handle-centric `.animate.set(...)` examples. Python setters return their receiver.
Rust follows existing checked Scene/LiveSession and target-editor conventions;
copying Python's fluent syntax is not a reason to bypass live publication.
Python imports use **`from noon import Glow, GlowPulse, Pixels`**, alongside
ordinary animation names. The current frontend is the module `noon.py`, not a
`noon` package; do not require a new package hierarchy or inject a synthetic
`noon.effects` module. Rust uses its existing `noon::effects` module. These are
idiomatic namespaces over the same operations, not different effects engines.
Do not add new names to the Manim compatibility export inventory as if ManimCE
already defined them. The leaf declaration exports/bindings below are implemented;
`GlowPulse`, effective queries, group/view scopes and effect execution remain later.

`Glow` describes values; `GlowPulse` describes a timed action. Python appearance
parameters are keyword-only. On setters, `None` means omitted/preserve, like
ordinary style setters; on initial construction it selects the documented default.
Rust's `GlowUpdate` expresses the same distinction with `Option`. Boolean, string
numbers, nonfinite numbers and unknown keys are rejected, not coerced permissively.
The operator reference's narrow scalar parser is not the shipping wrapper parser.
Names are required, case-sensitive nonempty strings. The first slice has no
anonymous-stack append or rename API. Creation, query and update are distinct:
`add_effect` never upserts; `set_effect` requires a binding; only `set_glow` upserts.

`get_effect(name)` resolves **only the exact receiver's attachment**, never its
children, parents, original copy or aliases. Rust target editors use the copied
binding's name/handle; a source handle is a wrong-owner error on that target.
Python `.animate.set_effect("accent", ...)` resolves source identity and target
correspondence in shared Rust. It must preserve the source generation: removing
and re-adding `accent` before activation cannot redirect an old animation to the
replacement. No recursive getter, list-or-handle return, or last-name-wins rule.
The first animated surface uses names; arbitrary raw-handle remapping behind
`.animate` is not implicitly promised. Static handle updates remain generation-safe.

Current `EffectHandle::authored_definition()` is explicitly an authored query.
The proposed Python handle `get_parameters()` observes a coherent effective value
once execution is supported (authored value before bootstrap); Rust live queries
use the existing live/session owner, not an effect handle's private runtime.
Returned parameter records are observations, never independently mutable scene state.

### Manim comparison and intentional extensions

Review baseline: Manim Community v0.21.0. Primary references:

- [Mobject.animate](https://docs.manim.community/en/stable/reference/manim.mobject.mobject.Mobject.html#manim.mobject.mobject.Mobject.animate): chained target edits and per-animation options.
- [VMobject source](https://docs.manim.community/en/stable/_modules/manim/mobject/types/vectorized_mobject.html): partial style setters, fluent return values and family propagation.
- [Composition source](https://docs.manim.community/en/stable/_modules/manim/animation/composition.html): normal animation preparation, lag, succession and duration rescaling.
- [Indication source](https://docs.manim.community/en/stable/_modules/manim/animation/indication.html): temporary animation intent, including Indicate's there-and-back pattern.
- [Mobject source](https://docs.manim.community/en/stable/_modules/manim/mobject/mobject.html): method-animation building and override chaining restrictions.

`set_glow` follows the setter/target-animation pattern; `GlowPulse` is a new Noon
indication animation, not a change to `Indicate` or `Flash`. Generic `set_effect`
must work in that same chained target expression, not an `override_animate`
shortcut that prevents chaining ordinary motion. Constructors, `.animate(...)`,
`play` overrides, AnimationGroup, Succession and LaggedStart must use the existing
option-precedence and composition rules. Constructor spelling alone proves none
of those behaviours. Keep precise disjoint-channel support a tested Noon extension;
do not infer it from Manim's warning against separate simultaneous method
animations of the same object.

### Existing appearance composition

`set_fill`/`set_stroke` and their ordinary target edits keep ownership of paint,
including the existing `SemanticPaint` gradient model. A local procedural paint
must extend that same appearance-value seam rather than introduce unrelated
`set_shader`, raw-uniform or material-only animation APIs. A filter such as Glow
operates on source output and therefore uses an attachment. The author chooses
appearance and scope; whether the renderer needs one draw or extra passes stays
an implementation detail. Custom program/schema resources remain M4 and must use
the same parameter/composition contract; the current closed Glow enum is not
claimed to be that external-extension proof.

### Complete paired source review

`authoring_review.py` and `authoring_review.rs` each contain **both complete
programs**. Proposed operations are marked with their implementation milestone.
Python uses sequential `play`/`wait`; Rust uses normal `LiveContinuation` and
`ContinuationStep::Await`. Neither source invents per-frame dispatch, a shader
clock, renderer resources, `Scene.seek`, or an effects-specific playback loop.
The Rust review uses the existing declaration-and-activation naming convention
for its proposed pulse/parameter leaves. It must not publish the cold
`Scene::declare_animation` helper into an already-running store.

| Program / checkpoint after normal logical completion | Expected state in both languages |
| --- | --- |
| Luminous, t=0 / .75 / 1.5 | x=0 / 1 / 2; intensity=.35 / .775 / 1.2 |
| Luminous pulse, t=1.8 / 2.1 | intensity=2 / 1.2; capture is 1.2, not its declaration-time .35 |
| Luminous wait/fade, t=2.6 / 2.8 / 3.0 | intensity=1.2 / .6 / 0; source continuation then removes `glow` without advancing time |
| Mixed parallel, t=1.5 | dot x=2, glow=1.2; title accent=1.4 |
| Mixed pulse, t=1.8 / 2.1 | dot glow=2 / 1.2; other attachments unchanged |
| Mixed succession, t=2.6 / 3.1 | scan phase=1, then dot glow=.5; live group-halo edit sets .25 at 3.1 |
| Mixed view, t=3.6 / 3.75 / 3.9 | bloom intensity=.5 / .25 / 0; source then removes bloom, group-halo, scan and click binding |

The mixed fixture pins Text("Signal"), regular DejaVu Sans Mono at 48 points,
three path points (-2,-1), (0,1), (2,-1), white two-unit stroke/no fill, and the
inline straight 2x2 RGBA8 bytes. Image height is one scene unit; both dots have
radius .08, opaque white fill and no stroke. The nested group is displayed, while
the separate aliased family is for query/copy review and is not also isolated.
The new actual Rust mixed-content test uses text/path/image/nested aliases to
check independent leaf attachments and unchanged source membership; it does
**not** claim that the proposed group filter or custom effect renders.

The external M4 `effect_fixture.ScanBand` contract remains: local normalized x,
width in (0,1], phase in [0,1], yellow band where abs(x-phase)<=width/2, clipped by
source alpha. Width=.2, phase=.5, x=0/.5/1 gives mask 0/1/0. Independently
supersample discontinuous edges. Bloom's HDR mathematics and the custom
shader/resource ABI remain M4; this review fixes their **calling convention**,
not unspecified HDR output or an already-extensible production enum.

**View scope decision:** use the existing authored camera frame:
`self.camera.frame.add_effect(Bloom(...), name="bloom", scope="view")` and
`self.camera.frame.animate.set_effect("bloom", ...)`. Rust obtains the same
semantic controller with existing `Scene::camera_frame()` before adding objects.
The review uses `MovingCameraScene`; it does not pretend every Manim Scene has
an animatable Camera or that `Scene.animate` exists. The scope means the camera's
composed authored output, **not the invisible frame rectangle's paint or opacity**.
The controller's motion changes the view through the ordinary camera path.
An ambiguous camera shared by multiple distinct authored views is rejected in
the initial view profile; multiple hosts rendering the same authored view are not
such ambiguity. No new ordinary `output_view()` object is required.

**Live/input/seek review:** register animation intent with `on_click(target,
animation)`; `None` clears that one binding. A new registration atomically replaces
it, rather than accumulating hidden listeners. Rust follows the same intent via
`Option<&DeclaredAnimation>`. Implementation extends existing shared admission,
not Python event-driven uniform writes. During the mixed authored intensity write,
a click is suppressed. During its wait it captures .5; clicks at interaction times
0 and .2 start only one .6-second pulse, restoring .5 at .6. A new click at .7 may
activate again. Seek/reload retires transient interaction invocations.

Forward/rewind tests are host-controlled comparisons at the same authored time
and publication phase. An endpoint sample before source continuation may still
have a neutral attachment that the subsequent continuation removes; do not compare
those as identical phases. Effect sampling uses the same rational authored-time
input as other properties; transport latency/frame count does not select its phase.

## Identity, family scope and lifecycle requirements

`set_glow` creates/updates the canonical binding `glow`, never accumulates hidden
attachments. `add_effect(..., name="glow")` creates the same binding kind, but is
not an upsert: duplicate names fail atomically. The name is reserved for Glow.
Additional names are independent and painter-ordered. Omitted fields preserve
current values; an empty patch creates documented defaults only when no binding
exists. An empty patch on an existing binding is a no-op.

**Scope and group decision:** the first supported named glow binds one leaf.
Later group attachment creation must explicitly request `scope="each"` or
`scope="composed"`; omitted scope on a new group binding is an error, not an
implicit isolation or a style-family broadcast. Each choice creates **one binding
on that exact group**, so `group.get_effect(name)` still returns one handle and
`group.animate.set_effect(name, ...)` edits its parameters normally.
`each` treats each unique current displayed leaf independently with those shared
parameters; `composed` treats one composed painter result. Neither choice rewrites
leaf attachment state. Independent leaf effects precede group treatment.
Ordinary Manim-style `set_fill`/`set_stroke` family propagation is unchanged; it
is deliberately not used to fake a many-owner effect handle. Authors can also
explicitly attach independent effects to chosen leaves, as the current Rust test
and API already permit.

Scope is fixed at binding creation. Later parameter setters do not repeat or
change it; convenience `set_glow` updates the existing canonical binding's scope.
New group canonical bindings also require explicit scope. Initial group stacks
must not mix `each` and `composed`: loss of source separation is not silently
resolved by reordering passes. Membership changes use existing shared family
semantics and atomic validation, with alias leaves processed once. Scope support,
its typed Rust options and family target overloads are M3/M4 work; no supported
leaf call changes meaning as a consequence of this review.

For initial composed support, validate actual painter-contiguous isolation.
External interleaving or unsupported cross-scope aliasing fails before publication;
never reorder external content or silently filter children independently. Nested
laminar scopes compose inner-first. An edit making isolation invalid fails as one
transaction. Neutral effects must not require isolation that changes pixels.
Each stacked attachment consumes the preceding output; painted masks use that
input alpha, while silhouette masks use the declared source geometry. Stable
attachment order is observable and filters need not commute.

An absent-to-present animated glow begins only at activation, from intensity zero
and validated target/default values for other fields; a constructed builder has
no live effect. Ordinary completion persists the **sampled endpoint under the chosen
time map**, not unconditionally the requested target. In particular a returning rate restores
the captured scalar value; a newly introduced ordinary binding may persist neutral
until explicitly removed. This is distinct from GlowPulse's temporary lifecycle.
Removing an attachment is structural: animate to neutral, then remove. Copy creates
independent attachment identities/values and shares immutable program inputs.
Target copies retain correspondence; unsupported stack/program transitions fail
before playback. Save/restore includes order/parameters; restoring a retired
attachment creates a fresh identity. `become` preserves its receiver object ID
but replaces its attachment set with independent copies and retires old handles.
Detach/re-add preserves persistent identity/values and retires transient drivers.
Python wrapper collection does not detach effects. Renderer recreation changes
caches, not semantic identity. Failed preparation retains the prior valid scene.

A GlowPulse owns intensity only, captures at activation, and defaults to the
existing `ThereAndBack(p)=smooth(1-abs(2p-1))` with normalized logistic inflection
10. Ordinary composition maps time first; no frame count or effect clock is used.
A rate override replaces the amplitude map; a non-returning map may be discontinuous
at the required lifecycle restoration. Completion restores only still-owned
channels. An introduced temporary binding is removed only if its generation and
temporary ownership remain valid. A persistent live edit adopts it, supersedes
its touched channels, and prevents whole-attachment cleanup. Timed glow/pulse
animations require finite **positive** duration through the same ordinary option
resolver. Zero/negative/nonfinite duration fails before allocation
or publication. Use the untimed setter for an immediate change; do not introduce
an effects-only zero-duration admission/completion path. Ordinary FadeOut
must remove source and attached output together; intensity is not object opacity.

Click-triggered GlowPulse must extend the existing shared native action/admission
path with its current filled circle/rectangle target limits, not a host callback
or new event dispatcher. Repeated clicks during an active invocation neither
restart nor queue; conflicts suppress admission, seek/reload retires it, and a
settled invocation sleeps. Use the existing interaction clock without advancing
authored time. Actual ownership/activation/recovery tests remain required.

## Parameters, units and errors

Defaults: opaque white; radius .15 scene units; intensity .35; source painted.
The full definition differs from an empty update. RGBA channels are finite in
[0,1], intensity is finite in [0,8], radius is finite and nonnegative. Radius is
Gaussian sigma, not diameter or cutoff. Plain Rust f64/Python spatial values mean
world scene units. `Pixels(12)` denotes final physical output pixels, independent
of CSS size, DPR or export-resolution guesses.

For a supported uniform planar view, sigma_px=radius*output_height/world_view_height.
Object scaling, including nonuniform scaling, deforms source geometry but does not
multiply world radius again. Camera zoom and output resolution affect scene units;
Pixels stays fixed. Reject unsupported nonuniform/perspective projection. Reject
nonfinite/overflowed conversion and invalid view dimensions before publication.
Semantic radius range is not a GPU limit; preparation additionally enforces the
reference profile's sigma_px<=64 and actual format/texture/memory capabilities.

Scalar/radius and encoded color components interpolate after the existing time
map, with exact endpoints. Unit and source changes are discrete: static updates
can change them, interpolation cannot. Invalid enum representations cannot be
constructed through typed Rust; Python rejects bool-as-number and unknown fields
before/at its shared checked operation. No permissive raw uniform-setting API.

| Invalid case | Required failure |
| --- | --- |
| Negative/NaN radius, invalid RGBA or intensity | typed parameter error; no partial replacement |
| Animate Scene radius to Pixels, or change source | discrete-transition error even at zero intensity |
| Duplicate name or stale handle after re-add | semantic duplicate/generation error, not accidental replacement |
| Two writers for the same parameter | existing channel-conflict admission with attachment/parameter context |
| Interleaved composed group / unsupported alias isolation | explicit isolation error, no painter reordering |
| Input/resource cycle, unsupported profile/scope | validation/capability error before coherent publication |
| Invalid shader edit or radius/view exceeds device limits | preparation error retaining previous valid resources/view |

## Visual model: `glow-encoded-ldr-v1`

The candidate model is an artistic tinted mask halo, not emission or HDR bloom.
It preserves encoded-premultiplied LDR composition: no global linearization,
tone map or extra output transfer. A later HDR profile must be separately named
and qualified. Existing encoded/quantized compatibility output stays unchanged.

Let S be original encoded-premultiplied RGBA before final scope opacity. The
painted mask M is its alpha, including intrinsic fill/stroke/image alpha.
Silhouette instead means declared geometric coverage independent of paint alpha;
for M1 filled circle/rectangle scope, interior plus declared stroke footprint.
A transparent painted source need not imply an invisible silhouette effect.
Sanitize alpha-zero straight source colours in normal source preparation.

At output pixel centres, R=ceil(3*sigma_px), and for integer dx,dy in [-R,R]:

    K(dx,dy) = exp(-(dx^2+dy^2)/(2*sigma_px^2)) / Z
    Z = sum of weights over the complete square support
    B(x,y) = sum K(dx,dy) * M(x-dx,y-dy)

Outside the padded capture is transparent zero. Never renormalize at a viewport
edge. Reference quality is full output resolution. Square truncation gives exact
90-degree/reflection symmetries, not an exact arbitrary-angle tail identity.
An analytic distance halo or multiresolution blur is not equivalent merely because
it is cheap; it must satisfy this model/tolerances or be named differently.

For straight encoded tint C and intensity I:

    h = min(1, I * C.alpha * B)
    H = (C.rgb * h, h)
    T = S + (1-S.alpha) * H
    output = scope_opacity * T

Source is above the halo. Final composed-scope opacity applies once after the
stack, not once to each child. Existing per-child opacity remains part of source.
The h clamp is the explicit LDR alpha model, not early HDR clamping. Absent,
removed, zero-intensity, zero-radius or zero-tint-alpha treatment uses the ordinary
path exactly. Radius-zero glow is identity even though zero-sigma convolution
alone is identity. Scope opacity zero proves no contribution here; base alpha zero
does not prove it for silhouette. Offscreen sources whose halo reaches the view
must survive contribution culling.

Render bounds expand by R pixels per axis plus source AA footprint; capture needed
source beyond the viewport before final clipping. Semantic layout/next_to/default
geometry picking do not expand. No hidden crop, quality reduction or CPU fallback.
The consistent model may require a minimal primitive mask/blur/composite path in
M1; broader source/group pass planning stays in M3, in the existing renderer.

## References, negative controls and evidence

The first glow uses the shared option precedence: animation defaults, then
animation-local options (including `.animate(...)`), then explicit `play` overrides.
Its normal target-animation defaults remain one second and Smooth; GlowPulse uses
one second and ThereAndBack unless overridden. Current supported bounded shared
rate functions are the initial surface; arbitrary host rate callables/unbounded
overshoot are not secretly accepted or clamped by the glow sampler. Generic
parameter/pulse leaves reject Transform-only path-arc options through the normal
resolver. A motion-plus-glow Transform retains its existing path-arc contract.

The independent Python `reference.py` has no product imports. It evaluates a slow
direct 2D sum with doubles, unlike a production separable/multiresolution blur.
For sigma=1/sqrt(2 ln2), unnormalized taps are 2^(-k^2); the hand-derived 1D taps
are [1,32,256,512,256,32,1]/1090. Their outer product is the exact 7x7 impulse
field with centre 262144/1188100 and total energy 1. `vectors.json` also supplies
literal timing and alpha-composition values; smooth is independently expressed
through tanh. Reference fields are bounded at 65x65/sigma<=16 for test cost,
not because those are product limits.

The Python suite checks impulse, edge, constant/asymmetric fields, border, finite
support, units, alpha, neutral state, order/group witnesses, timing and supplied
restoration tables. It deliberately rejects unnormalized/offset/cropped fields,
wrong units, ignored updates, bad alpha/extra transfer, frame-count phase and
stale restoration. Those are oracle-level defects, not production shader/driver
injection. Repeated pure samples do not demonstrate actual Runtime seek. The new
Rust suite separately exercises real shared value/timing components; neither
suite by itself proves live effect playback. The additional Rust authoring tests
exercise actual attachment transactions and public target operations, with explicit
negative admission at execution/publication boundaries.

Use exact IDs/order/events/endpoints; normalized floating operator buffers have
absolute error<=1e-5, encoded RGBA8 <=2/255 per channel over the entire expanded
ROI. Inspect alpha and composition over black/white/saturated backgrounds. Keep
separate support/tail witnesses; a whole-frame mean cannot hide a clipped halo.
Same-backend neutral and restoration output must be exact; existing no-effect
ratchets are not relaxed. Qualified source images may feed filter references, but
that proves filtering/composition, not independent glyph/source rasterization.
Retain assets, seeds, rational sample times, source/program hashes, view/backing
size, quality, actual backend/adapter and all failed attempts.

Commands (success must be recorded from actual runs, not inferred):

```sh
cargo test -p noon-core --lib object_state::glow
cargo test -p noon-core --lib semantic_store::effects
cargo test -p noon --no-default-features --lib effect_authoring
cargo run -p noon --no-default-features --example effect_authoring_contract
cargo test -p noon-core --doc
cargo test -p noon-web --all-features --lib authoring_effects
cargo test -p noon-web --all-features --lib canonical_authoring_scene::effects
python3 -m unittest discover -s web/python -p test_noon_effects.py -v
# Requires the actual browser package/Pyodide host, not Python routing mocks:
node scripts/shared-authoring-smoke.mjs
python3 -m unittest discover -s tests/visual-effects -p 'test_*.py' -v
rustfmt --edition 2021 --check tests/visual-effects/authoring_review.rs
node --test .github/ci/visual-effects-reference.test.mjs
bash scripts/check.sh fast
```

Rust unit tests use existing Cargo/PR Fast discovery; positive/negative rustdocs
run in the normal full Rust gate. The bounded Python reference stays in existing
`.github/ci/*.test.mjs` discovery and the local check entrypoint. Do not add a
parallel workflow/framework or mark missing execution as a test pass.

## Qualification baseline and implementation handoff

`qualification.json` retains the exact G0-G6 corpus, reference quality, views,
targets, budgets and repeated-pair protocol. Every device flag is false; the base
revision is an identity, not a measured performance result. Native Rust, direct
WASM, native Python and Pyodide must be qualified separately on their supported
common surface; offscreen uses the same runtime/renderer. #1896 encoder work is
not required to qualify exact-time frames. Restore/loss/reload tests use existing
resource generations and publication, not an effects-specific recovery engine.

Reuse #1653's per-PR and cumulative-anchor gates unchanged: three serial B/C,
C/B,B/C pairs, one completed cold source pass before each trial, 30 warmup and
540 measured frames, authored horizon>=10s, declared output backing and 60Hz
target. Freeze resolved dependencies, compiler/browser, source/package hashes,
actual adapter, view and quality first. Retain all samples, means, dispersion and
invalid attempts; never select the best trial. For enabled-effect comparisons,
use the first qualified enabled baseline at identical quality, not a no-effect
image. Before one exists report absolute cost plus independent correctness.

Physical targets remain Intel MacBook Pro/Radeon Pro 5300M and iPhone14 Pro Safari;
a connected phone is not assumed. Linux software GPU is correctness/controlled
regression evidence, not physical presentation. Goals remain p95 gap<=20ms,
p99<=33.4ms, fraction >25ms <=.02, p95 input-to-present<=50ms and extra scratch
<=64MiB desktop/32MiB mobile. Existing literal physical-FPS gates are not relaxed.
Measure 60 interactions one second apart after warmup, with three full repeats;
CPU submit time does not prove input-to-present, and unsupported GPU timers are
null, not zero. Report cold compilation/first-use separately.

Intensity-only warmed edits must compile zero pipelines, rebuild zero geometry,
dirty zero unrelated objects and allocate zero new scratch targets. Measure 1/100
active glows among 600 static objects and 1000 attach/remove cycles, including
passes, upload ranges/bytes, CPU time and resident/peak/in-flight memory. Radius,
view or topology changes may resize affected local targets. Absent effects add no
passes, targets, global scan or unconditional time updates.

| Existing owner | Reuse / missing prerequisite |
| --- | --- |
| `noon-core::object_state::glow` | Implemented typed values, validation and partial interpolation; not an attachment registry |
| SemanticStore and canonical transactions | Implemented named leaf attachment identity/order, atomic creation/removal/copy and partial updates in the existing arena; generic scope/program resources remain later |
| `animation/timeline.rs`, composition and time maps | Reuse shared timing. Existing scalar `Property::Appearance` is NOT an effect map; add finite parameter-channel addressing |
| Scene/live/target and effective capture | Implemented authored setters, handles, independent object/target/family copies; live execution rejects unsupported effects, and effect channels/capture/cleanup remain later |
| Runtime/session | Still need actual activation, write leases, ownership-safe pulse cleanup, seek and wake/sleep |
| Python authoring adapters | Only coercion/signatures/return values/exception mapping; no Python effects state or interpolation |
| Retained renderer, presentation/inset and Cairo colour | Minimal primitive mask/filter work, safe source bounds and local parameter updates; no new renderer |
| Existing native/web/offscreen hosts | Same semantic/runtime path, capability queries and resource-generation recovery |

The M0 source review, first-glow decisions, independent references and reuse
inventory are recorded here. Actual execution tests of those decisions belong to
the named implementation promotions, not to a mock scene added to satisfy a test.
M0 merge still requires applicable repository CI; neither this review nor old-head
performance evidence establishes a current-head merge pass.
