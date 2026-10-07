# Visual-effects contract fixtures

M0 work for [#1897](https://github.com/yongkyuns/noon/issues/1897).
`docs/architecture.md` is the sole architecture/roadmap authority; this is an
acceptance specification and reference-fixture description, not another roadmap.
Inspection base: `9028672c1691d3a318c87da50e8b08e0802245f0`. The subsequent API
review checked the unchanged public authoring hooks at `6f7b7f008c45288bf3b92a417078b907d5e4ef5c`.

**Status: partial M0, not an accepted API freeze.** Shared Rust parameters and
real authored leaf attachments now exist in the normal SemanticStore. Rust Scene,
Mobject and target-copy operations are implemented; their tests exercise actual
transactions, identity, mutation, copying and retirement. Effect execution, Python
bindings, actual animation activation/ownership and GPU rendering remain unavailable.
Running-scene publication and execution bootstrap fail explicitly rather than
accepting an invisible effect. No rendering, seek or pulse-lifecycle pass follows
from these declaration tests.

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
Nonstructural parameter changes emit `EffectParameters`, distinct from structural
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
animation/channel ownership, and Python wrappers remain future integration work.
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

## Revised creator-facing grammar (Python/composition design)


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
New Noon effects live in one discoverable Noon-native namespace; they are not
asserted to be existing ManimCE effects or added to compatibility exports blindly.
`noon.effects` remains the proposed Python namespace, not a module shipped here.

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

### Introductory Python review specimen

```python
from noon import Circle, Scene, RIGHT, linear
from noon.effects import GlowPulse

class LuminousExplanation(Scene):
    def construct(self):
        dot = Circle(radius=0.08).set_fill("#FFFFFF", opacity=1).set_stroke(width=0)
        dot.set_glow(radius=0.15, intensity=0.35)
        self.add(dot)
        self.play(dot.animate.shift(RIGHT * 2).set_glow(intensity=1.2),
                  run_time=1.5, rate_func=linear)
        self.play(GlowPulse(dot, intensity=2.0), run_time=0.6)
        self.wait(0.5)
        self.play(dot.animate.set_glow(intensity=0.0), run_time=0.4, rate_func=linear)
        dot.remove_glow()
```

At t=0/.75/1.5, x=0/1/2 and intensity=.35/.775/1.2. The pulse captures 1.2 at
activation, reaches 2 at 1.8 and restores 1.2 at 2.1. Wait ends at 2.6, intensity
is .6 at 2.8 and exactly 0 at 3.0. Explicit removal retires the binding without
advancing time. The source resumes only after the normal completion barrier.
No per-frame Python callback is needed for deterministic effects.

The future rendered Rust counterpart must use the same filled, unstroked circle
and values. The declaration-only Rust example now exercises real `Scene::set_glow`,
target editing and `declare_transform_to`, but it does not yet play this animation.
Do not publish a cold `declare_animation` directly against a running store.
Runtime parameter channels, actual effective observations, temporary pulse lifecycle
and mixed-feature source continuation remain implementation gaps.

### Mixed-content/group review specimen

```python
import numpy as np
from noon import Circle, Group, ImageMobject, Scene, Text, VMobject, UP, linear
from noon.effects import Glow, GlowPulse, Pixels
from effect_fixture import ScanBand

class MixedExpression(Scene):
    def construct(self):
        title = Text("Signal").shift(UP * 2)
        path = VMobject().set_points_as_corners([(-2, -1, 0), (0, 1, 0), (2, -1, 0)])
        image = ImageMobject(np.array([[[255, 0, 0, 255], [0, 0, 255, 0]],
                                     [[0, 255, 0, 128], [255, 255, 255, 255]]],
                                    dtype=np.uint8))
        dot = Circle(radius=0.08)
        inner = Group(path, image)
        group = Group(inner, dot)
        self.add(group, title)
        group.add_effect(Glow(radius=Pixels(12)), name="accent", scope="composed")
        title.add_effect(ScanBand(width=0.2, phase=0.0), name="scan")
        self.play(group.animate.set_effect("accent", intensity=1.4),
                  run_time=1.0, rate_func=linear)
        self.play(title.animate.shift(UP).set_effect("scan", phase=1.0),
                  run_time=1.0, rate_func=linear)
        self.play(GlowPulse(dot, intensity=2.0), run_time=0.6)
        group.set_effect("accent", intensity=0.25)
        self.wait(0.5)
        group.remove_effect("accent")
```

Both Python blocks are syntax-checked review specimens, not executable support.
Pin regular DejaVu Sans Mono at 48 points, source paint, image sizing and those
exact 2x2 bytes in the future native Rust/Python pair. The Rust source fixture uses
ordinary `Scene::text`, paths, `image_rgba8` and family operations, not an effects
scene builder. `effect_fixture.ScanBand` is a future external extension fixture,
not an available package: normalized local x, width in (0,1], phase in [0,1], yellow
band where abs(x-phase)<=width/2, clipped by source alpha. At width=.2, phase=.5,
x=0/.5/1 gives mask 0/1/0. Independently supersample its discontinuous edges.

Scene/output effects must have the same parameter and timing grammar, but the
creator-facing Scene entry is still under review. This revision removes the
unjustified requirement to call `output_view()` in ordinary scripts; it does not
replace it with an equally unqualified scene `.animate` API. HDR Bloom mathematics,
custom WGSL/resource ABI and the outside-central-dispatch extension proof remain M4.

## Identity, family scope and lifecycle requirements

`set_glow` creates/updates the canonical binding `glow`, never accumulates hidden
attachments. `add_effect(..., name="glow")` creates the same binding kind, but is
not an upsert: duplicate names fail atomically. The name is reserved for Glow.
Additional names are independent and painter-ordered. Omitted fields preserve
current values; an empty patch creates documented defaults only when no binding
exists. An empty patch on an existing binding is a no-op.

Family treatment and composed filtering are different. Default `scope="family"`
means the existing unique render-leaf family semantics; on a leaf it means that
leaf. `scope="composed"` explicitly means filtering one composed painter result.
The same scope option must exist on the convenience operation, with generic and
canonical equivalence. Scope is structural, not animatable. Switching scope
requires explicit removal/re-attachment rather than silent conversion.
Do not redefine Group/VGroup deduplication or layout just to simplify filtering.

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
no live effect. At ordinary completion requested target values persist. Removing
an attachment is structural: animate to neutral, then remove. Copy creates
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
its touched channels, and prevents whole-attachment cleanup. Zero duration admits
and completes once without a leaked attachment or visible peak. Ordinary FadeOut
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
python3 -m unittest discover -s tests/visual-effects -p 'test_*.py' -v
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

M0 remains incomplete until the creator-facing Rust/Python contract and supported
composition/lifecycle boundaries are qualified. The authored Rust surface is now executable and tested,
but is not a substitute for effective-channel/lifecycle integration or M1 rendering.
