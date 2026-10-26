# Visual-effects contract fixtures

## Current authored GlowPulse extension

`GlowPulse(obj, intensity=2.0)` is an ordinary authored animation for one supported
leaf already present in the execution scene. It owns only the canonical glow's
intensity and defaults to one second with `there_and_back`. Its Rust counterpart is
`live.declare_and_activate_glow_pulse(&obj, 2.0, AnimationOptions::new())`;
`Scene::declare_glow_pulse` also creates a replayable inert declaration.
Construction does not enroll an attachment or capture a source value.

The shared compiler captures coherent effective intensity at activation and emits
an ordinary `Property::GlowIntensity` track with `EffectTrackEndBehavior::RestoreSource`.
The existing composition time map runs before the leaf amplitude map. At the mapped
child endpoint the channel restores its captured source, including when a Linear
or other non-returning rate override makes that restoration discontinuous. A later
non-overlapping Succession child captures this restored endpoint. A reversing
parent can revisit the child; reaching the enclosing root's end forces restoration.
No additional effect clock, runtime, scheduler or frontend lifecycle state is used.

| Situation | Shared behavior |
| --- | --- |
| Existing canonical glow | Capture effective intensity; preserve color, radius, source and attachment identity |
| No glow | Atomically enroll a neutral canonical attachment using documented glow defaults and intensity zero |
| Child completes before its enclosing composition | Restore intensity immediately; retain neutral temporary identity so a reversing parent can revisit it |
| Enclosing segment completes | Remove only the exact temporary attachment generation that remains unadopted |
| Explicit persistent color or radius edit | Adopt that attachment and preserve the pulse intensity channel |
| Explicit persistent intensity edit | Adopt that attachment and supersede its pending pulse intensity tracks, including later leaves in the same prepared segment |
| Explicit same-valued edit | Preserve its write intent; adoption and channel supersession do not depend on dirty-value impacts |
| Ordinary glow parameter target beside a pulse | Adopt the shared attachment for its persistent result; retain the normal pending-driver guard for those ordinary parameters |
| Overlapping intensity drivers | Reject the whole activation before publication |

Persistent edits use the ordinary semantic transaction and execution publication
boundary. Structural removal/replacement retains the existing pending-segment
guard. Live parameter edits retain the existing executable schema: radius units
and source mode cannot change within a live attachment. Animated method targets
retain explicit parameter intent in shared Rust,
so `.animate` motion or radius edits do not accidentally take ownership of intensity
merely because an effective pulse differs from authored base state. The equivalent
Rust composition uses `TransformToRequest::method_target()`. An ordinary full
`Transform` still targets the complete copied appearance, including returning to
an untouched earlier snapshot; explicit same-valued writes own their parameter
in either form.

Intensity must be finite and in `[0, 8]`; duration must be finite and positive.
The normal animation/local/play option precedence applies. Omitted numeric options
cross Python/WASM as `None`; an explicit NaN is preserved for shared validation.
Pulse leaves reject introducer/remover flags, nonzero lag and Transform-only path
arcs. Shared bounded rate functions, including reversing amplitude maps, are
supported. Elastic/Back leaf rates are rejected because they overshoot the supported
parameter interval; bounded floating-point roundoff is normalized at sampling.

The paired `glow_pulse` examples exercise a persistent halo, temporary enrollment,
disjoint motion/radius editing and forward/reversed Linear overrides through the
existing authored segment path.
Runtime, transaction and publication tests establish temporal and ownership
semantics. The native `authored_glow_pulse_pixels_restore_and_replay` fixture
compares six persistent/temporary and default/Linear/reversed-Linear profiles
against an independent full two-dimensional Gaussian at five rational times. It
checks full RGBA error, visible pulse controls, exact repeated and rewind samples,
baseline restoration, retained persistent identity and temporary retirement.
The existing browser smoke samples actual halo and restored pixels and compares
the paired Rust/WASM and Python programs. Qualification is recorded from executed
checks on the owning PR. This does not enable click-triggered pulses, family
broadcasting, stacks, group/backdrop scopes or custom shaders; those remain separate work under
[#1897](https://github.com/yongkyuns/noon/issues/1897).

## Current image-source extension

The executable leaf-glow profile now includes immutable planar raster images,
alongside geometry and Text/MathTypst. Image effects use the same `set_glow`,
`.animate.set_glow`, named attachment, and explicit removal operations. The paired
`image_glow` Rust/Python examples prepare one RGBA texture, share it between two
images, and animate only ordinary pose and glow parameters.

Image coverage is its sampled intrinsic alpha, including soft and fully
transparent texels. `source="painted"` also includes image fill opacity;
`source="silhouette"` ignores that paint modulation but retains intrinsic alpha.
Neither mode substitutes the rectangular image bounds for a transparent cutout.
Final object opacity and appearance apply once to the composed source and halo.
All three existing image sampling modes use the ordinary premultiplied-alpha
image shader; hidden RGB in alpha-zero texels does not supply painted coverage.

Image capture borrows the resident texture and image pipeline. Its disposable
normalized instance binding is distinct from the ordinary instance, so enabling
an effect does not modify the image used elsewhere. Radius/color/intensity edits
reuse immutable image pixels. Source capture remains a renderer responsibility,
not another authored resource, shader clock, or scene representation.

`image_glow_preserves_texture_alpha_sampling_replay_and_lifecycle` in the existing
native `glow_scene_raster` target compares all RGBA channels against an independent
full two-dimensional Gaussian using separate ordinary image renders. It covers
nearest/bilinear/bicubic sampling, both source modes, paint and scope opacity,
affine movement, same-time and seek equality, neutral restoration and removal.
This qualifies filter/composition behavior, not independent image rasterization
or physical-GPU speed. The existing browser authoring smoke executes the paired
Python example and verifies completion and the retained ordinary image.

The image-fill extension below covers supported planar vector geometry.
Procedural paint, effect stacks, composed groups, backdrop capture and custom shaders remain separate
work under [#1897](https://github.com/yongkyuns/noon/issues/1897). The following M0
sections preserve the original design and declaration-stage evidence; their
historical execution guards are not a current support matrix.

## Image-painted planar geometry

`ImagePaint` supplies immutable pixels inside an ordinary circle, rectangle or
planar vector path. It uses the existing geometry coverage and solid stroke;
intrinsic image alpha, fill opacity and final object opacity remain independent.
The Rust counterpart is `ImagePaintOptions` with checked `ImageMapping` and
`ImageUvTransform` values. Construction creates no scene object or GPU resource.

```python
from noon import ImageMapping, ImagePaint, Rectangle, Scene, linear

class PaintedCard(Scene):
    def construct(self):
        card = Rectangle(width=4, height=2, stroke=None)
        card.set_fill(ImagePaint("photo.png", mapping=ImageMapping(
            fit="cover", scale=(-1, 1)
        )), opacity=0.8)
        self.add(card)
        self.play(card.animate.set_fill_image_mapping(
            ImageMapping(fit="cover", translation=(0.15, 0), scale=(-1, 1))
        ), run_time=1, rate_func=linear)
```

The mirrored image keeps each scale's sign in its translated target. Animating
from a positive scale to a negative scale is rejected because the interpolation
would cross zero. Immediate replacement may change scale signs, fit, addressing,
sampling or resource identity.

Pixels may come from PNG/JPEG, a uint8 image array or an existing `ImageMobject`.
An existing bitmap shares immutable pixels and preserves its sampling unless
explicitly overridden; encoded/array inputs default to linear sampling. Nearest,
linear (tent) and bicubic use the shared premultiplied image reconstruction, so
hidden RGB in alpha-zero texels cannot bleed into visible paint.

Mapping uses the lower-left normalized local geometry box. Image rows are
top-first; positive rotation turns counterclockwise. `stretch` fills the box;
`cover` preserves aspect ratio and crops; `contain` preserves aspect ratio and
clips to its fitted rectangle, keeping empty margins. Alignment ranges from
zero to one per axis. Addressing is `transparent`, `clamp` or `repeat`, applied
per filter tap. Translation, rotation and signed scale place the fitted image
about the local box center. Geometry motion, camera zoom and output resolution
do not rewrite these authored values.

Ordinary target animation interpolates mapping and fill opacity through the
shared Fill channel. Endpoints require the same
resource, sampling, fit and address modes and matching scale signs; exact
endpoint samples preserve authored values. A mapping-only edit admits/uploads
no pixels. Image fills share the existing bitmap texture pool, including when
one resource also belongs to an `ImageMobject`. Resource roots include snapshots
and timeline endpoints; retirement follows the existing publication lifecycle.

Blur, Shadow and both Glow source modes reuse the painted geometry's isolated
source capture. Image stroke/text paints, callback paint overlays, transient
family presentations and cross-kind image crossfades remain outside this finite
profile. Extreme mappings that cannot be represented safely by the GPU are
rejected. This profile does not promise arbitrary f64 sampling precision or
alias-free minification.

Executable matching programs are `noon::example_scenes::image_fill`, native
`image_fill`, and `web/python/examples/image_fill.py`. The actual native pixel
oracle tests all fits, address modes and filters across transformed geometry,
alpha/opacity and resource reuse; separate whole-image comparisons check source
filters and unchanged solid strokes. The browser qualification script is
`scripts/image-fill-qualification.mjs`: it compares direct Rust/WASM and Python
on WebGPU/WebGL2 against an independent physical-fit/inverse-transform/filter
reference, including inherited bitmap sampling and authored-time samples.
Premultiplication/addressing shader mutations must fail that reference. Exact
source/package identities, executed results and remaining gates live in #2330;
these correctness checks do not establish physical-device frame pacing.

## Continuous gradient source capture

Ordinary planar circle, rectangle and retained path linear fills can carry the
same named glow attachment. Paint edits and glow parameter edits use their
ordinary independent channels, with the existing authored clock, target capture,
pulse ownership and completion. Gradient text/strokes, groups and
effect stacks remain outside this profile.

Painted capture includes interpolated stop alpha and fill opacity. Silhouette
capture ignores paint while preserving the same geometry and stroke coverage.
Final object opacity applies once. The retained gradient binding is shared with
the ordinary draw; its immutable value invalidates source capture on paint-only
edits. Clean frames upload nothing; warmed intensity edits reuse capture and blur.

The native `gradient_glow_preserves_paint_alpha_seek_and_local_capture` oracle
uses separate no-effect renders and a slow full 2-D Gaussian, comparing every
RGBA channel within two bytes. It covers circles, rectangles and stroked paths,
reflection/rotation/nonuniform scale, both source modes, animated alpha/stops,
transparent and colored backgrounds, exact seek, neutral/removal output and
resource reuse. Temporarily disabling the actual gradient capture route must
make it fail. This qualifies filtering/composition; the separate linear-paint
oracle qualifies paint rasterization. Neither establishes physical GPU speed.

`linear_gradient::glow_program`, the native `gradient_glow` example and
`web/python/examples/gradient_glow.py` author the same paint/effect lifecycle.
`scripts/linear-gradient-qualification.mjs` checks real Rust/WASM–Python image
pairs on WebGPU and WebGL2, independent interior paint values, a visible halo
and exact removal. Existing CI owns both native and browser qualification.

## Historical M0 contract and evidence

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

## Linear-gradient values and reference boundary

The first paint prerequisite for #1897 defines checked inert Rust `GradientStop`
and `LinearGradient` values in `noon-core::object_state`. The integration now
carries these immutable definitions through semantic fill, checked planar
lowering, compiled/effective rows, atomic style publication, the genuine worker
codec, and retained analytic/path draws. No placeholder Scene setter or Python
class is exported. This remains **an integration prerequisite, not a supported
cross-language paint/animation feature or M3 completion**.

The finite GPU definition admits at most 16 stops, distinct endpoints after f32
conversion, and no distinct stop positions that collapse to one GPU position.
Text, images and spatial paint are rejected by semantic lowering. Gradient source
capture with glow and transient derived-display packing are explicitly rejected
until their shared integration is implemented. Ordinary gradient animation,
effective target capture, paired public authoring, actual browser execution and
the broader coverage/quality/performance gates remain outstanding.

The native `continuous_gradient_pixels_match_literal_encoded_ldr_reference`
test checks analytic circles/rectangles, convex paths and tessellated concave
paths against literal encoded-LDR interior references. Its eight profiles cover
solid baseline, local paint edits, transparent-stop premultiplication,
right-continuous coincident stops, diagonal axes, separate fill/object opacity
and affine transforms. It requests a real adapter and fails if none is available;
it is not a software/noop pixel test. Retained structural tests check no paint
program on an ordinary frame, one reused uniform per changed paint, zero clean
uploads, unchanged path meshes and bounded painter-chunk rebuilding. A 100,000
object runtime test checks atomic, sparse paint-only publication and idempotence.
These component tests do not qualify coverage edges, temporal paint interpolation,
export equivalence, physical-device performance or browser/backend parity.

For the first encoded-LDR profile:

- Endpoints are distinct finite points in the normalized local geometry box.
  `(0, 0)` is its lower-left corner and `(1, 1)` its upper-right. Motion, camera
  zoom and object scale do not rewrite the paint definition.
- At least two stops are explicitly ordered by finite positions in `[0, 1]`.
  Repeated positions are hard edges; the last coincident stop wins exactly at
  the edge. Positions are not silently sorted or deduplicated. Before the first
  stop, pad uses the first color; after the last, it uses the last color. In
  particular, sampling before a repeated stop at zero differs from sampling
  exactly at zero. Do not clamp the projected coordinate before stop selection.
- Sample by projecting the local point onto the endpoint axis. Interpolate
  **premultiplied encoded-sRGB RGBA**, preventing invisible RGB from leaking into
  partially transparent samples. This profile introduces no linear-light/HDR
  transfer. Coverage, fill opacity and final object opacity multiply the result
  once each at their existing owners; they are not baked into stop colors.
- Definition clones share immutable stop storage. Invalid values fail before a
  definition exists. A valid short axis must not divide by an underflowed squared
  length; renderer lowering must separately qualify representable GPU inputs.

`reference.linear_gradient_pixel` uses exact rational arithmetic from decimal
input spellings, independent of Rust's rescaled f64 projection and binary stop
lookup. Both implementations check the 25 specification-derived samples in
`linear-gradient-vectors.txt`; the Python oracle independently recomputes their
expected values. Literal vectors and seeded defects cover projection, units, hard edges,
transparent color, and double-applied opacity. The existing `test_reference.py`
entrypoint includes these tests. These component tests do not establish geometry
rasterization, GPU pixels, temporal behavior, cross-host parity or performance.

The next authoring slice exposes `Mobject::set_fill_gradient` and
`LiveSession::set_fill_gradient` with an immutable checked `LinearGradient`.
Python uses the ordinary `obj.set_fill(LinearGradient(stops, start=..., end=...))`
and `obj.animate.set_fill(...)` operations. Omitted opacity preserves its multiplier.
The paired executable examples are `noon::example_scenes::linear_gradient`,
`crates/noon-native/examples/linear_gradient.rs`, and
`web/python/examples/linear_gradient.py`; the same Rust program has a direct
WASM host. Python owns argument coercion only.

Paint interpolation uses the existing Fill channel and authored clock. Matching
stop counts, ordered positions and axes interpolate with premultiplied encoded-sRGB
colors; solids/absent paint promote to the other endpoint's mapping. Exact endpoint
paint kinds, immutable definitions and authored f64 opacity survive reconciliation.
The compiler rejects axis collapse anywhere in the interval, GPU precision loss,
opening/closing subpixel stop gaps, overshooting leaf rates and unsupported sources
before publication. Immediate edits may change stop count. Native reactive fill
drivers retain existing conflict admission. Initial support is planar positive-area
circle/rectangle/path fills. Strokes, text/image fills, content morphs,
derived displays, callback paint edits and group fills remain explicitly unsupported.
Scalar color queries reject continuous paint; fill opacity reports its multiplier.

`paint-transition-vectors.txt` supplies 12 literal temporal/spatial/opacity cases.
The independent rational Python oracle recomputes them and seeded-defect tests reject
straight-alpha temporal mixing, frozen time and repeated opacity. Shared Rust
sampling checks the same literals. `linear_gradient_raster` qualifies public Rust
animation through the retained native renderer with 80 interior RGBA comparisons
(maximum byte error 1), including analytic/path coverage routes and clean uploads.
`scripts/linear-gradient-qualification.mjs` runs the paired program through real
Rust/WASM and Python browser hosts on WebGPU and WebGL2; it requires identical host
pixels and independently checks 30 interior samples per time/backend. These commands
are qualification gates, not claims of a pass on every physical device.

## Implemented shared Rust boundary

`crates/noon-core/src/object_state/glow.rs` contains the production,
renderer-independent `Glow` and `GlowSource` values. Its sibling
`effect_parameters.rs` owns `EffectUpdate`, `EffectRadius`, `Pixels`, typed
parameter errors and `PreparedEffectUpdate`, shared with `Blur`. The existing
object-state module exports them.
They use Noon's existing `Color`, not a parallel color type or Python validator.
No new crate, dependency, identity allocator, scene store, timeline or shader exists.

`Glow` is a complete validated value with private fields. `EffectUpdate::default()`
is an empty partial request, not a reset to appearance defaults. A whole request
is validated before a replacement value is returned. The following is actual Rust
API, also covered by a compilable rustdoc example:

```rust
use noon_core::{Glow, EffectParameterError, EffectUpdate, Pixels};

fn parameters() -> Result<(), EffectParameterError> {
    let halo = Glow::new(
        EffectUpdate::default().radius(Pixels(12.0)).intensity(0.25),
    )?;
    let brighter = EffectUpdate::default().intensity(1.4).apply_to(halo)?;
    assert_eq!(brighter.radius(), halo.radius());
    assert_eq!(halo.intensity(), 0.25);
    Ok(())
}
```

These are inert shared inputs, not wrapper-owned mutable attachment state.
`request.prepare(captured)` accepts an explicit coherent activation-time value;
it does not obtain that value from a store, activate a driver, or check a lease.
`PreparedEffectUpdate::sample(mapped_alpha)` returns a **partial update**, never an
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
use noon::effects::EffectUpdate;

fn authored_contract() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let dot = scene.circle(0.08)?;
    scene.add(&dot)?;
    scene.set_glow(&dot, EffectUpdate::default().intensity(0.25))?;
    let mut target = dot.target_editor()?;
    target.shift(2.0, 0.0)?;
    target.set_effect("glow", EffectUpdate::default().intensity(1.4))?;
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

`EffectUpdate::parameters()` enumerates only explicitly supplied channels without
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
| Persistent canonical glow | `obj.set_glow(**patch) -> obj` | Scene/live `set_glow(&obj, EffectUpdate)`, fallible coherent publication |
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
Rust's `EffectUpdate` expresses the same distinction with `Option`. Boolean, string
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

## Real Pyodide binding qualification without a browser

`shared-authoring-pyodide-contract.mjs` runs the same checked-in
`effect_authoring_contract.py` fixture against the **actual generated bindings,
WASM Rust authoring store and pinned Pyodide interpreter**. This is not the facade
routing suite: invalid values, duplicate names, source/target isolation, exact-owner
and stale references, startup rejection, cleanup and subsequent live rejection
all cross the language boundary. Factories match the worker's typed operations;
there is no mock effect store or replacement Python implementation.

```sh
# First generate web/pkg from this source, with the repository's pinned tools.
NOON_PYODIDE_ROOT=/path/to/pinned/pyodide/full \
NOON_EFFECT_REPORT=/path/to/effects-binding-result.json \
node scripts/shared-authoring-pyodide-contract.mjs
```

The supplied runtime directory must contain the worker's pinned interpreter and
its NumPy dependency. An absent path or version mismatch fails; there is no
successful skip. The generated package checker requires the effect value/handle
exports and authored/live methods, so a stale package without the new bindings
cannot pass just because the Python sources exist. No new product dependency,
workflow or renderer is required for this optional host test.

This path calls the complete fixture's construct directly and uses the existing
Rust `ordinaryWait` synchronous completion. It does **not** qualify JSPI, portable
source rewriting, worker ownership transfer, browser presentation, pixel identity,
GPU execution or a native CPython adapter. Those remain with the existing browser
and native-host suites. In particular, rejected effect execution followed by a
successful plain wait is not evidence that a glow was rendered.

A useful mutation control bypasses `_noon_effects._call`'s live context and calls
the raw authored handle instead. The fixture's post-wait unsupported-operation
assertion must fail; restore the exact source and require it to pass. Separately,
removing the exact `WasmEffectDefinition` class export must make `check-web-package.mjs` fail
without confusing `WasmEffectUpdate` for that missing class. Preserve red/green logs
and source/package hashes; these are correctness controls, not performance runs.

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


### M1 ordinary parameter-channel integration (still guarded at Scene orchestration)

`CompiledEffect::parameter_channels` derives only requested color, radius and
intensity endpoints from a coherent activation-time runtime value. These become
ordinary `TrackDefinition` values with the existing TrackIds, timing, nested time
maps and scheduler. Attachment generation travels with each track; the compiler
and sparse transaction preflight reject missing/stale attachments or incompatible
radius units before publishing any transaction prefix. Raw timeline overlap keeps
the ordinary channel order; this does not replace authored driver-lease admission.

Runtime samples radius/intensity in f64 through the existing precise timing and
M0 glow interpolation contract. Staged publication retains the same atomic row
boundary, channel removal restores only its own base parameter, and seek/returning
rates reuse the ordinary timeline. The renderer consumes the resulting effective
definition through `RendererPublication`; it owns no parameter clock or mirror.

The canonical GPU qualification requires a distinct animated-publication stage
in addition to the operator, real shader mutation, restored positive, painter,
and static semantic stages. It compares 30 frames (six cases with forward samples
and rewind) against independent endpoint arithmetic and an extended-canvas 2D
Gaussian/painter reference. Intensity-only motion-free samples additionally assert
no source recapture or blur, 32 uniform bytes uploaded, and no texture/pipeline
allocation. Zero intensity must produce bit-exact ordinary rendering. Raster
capture origins preserve the output 2x2 derivative-quad grid; integer pixel
alignment alone can change SDF antialiasing as capture bounds move.

These are actual ordinary runtime-track and GPU-publication tests, not yet a
complete public `Scene.play`/Python-worker test. Normal Scene execution remains
guarded pending semantic target activation/completion, live attachment setters,
automatic host preparation and worker serialization. The source-alpha oracle still
uses production primitive rasterization; GPU timing and physical-hardware
performance are separate acceptance requirements. Pixel tolerances remain 2
RGBA8 bytes and mask error remains 1e-5.

## RGBA Gaussian raster prerequisite

The reusable renderer's `GaussianBlurFilter` consumes an encoded-premultiplied
RGBA8 capture, before final scope opacity. Sigma uses physical output pixels,
is bounded to [0, 64], and has square support `ceil(3*sigma)` in each axis.
Normalize the complete kernel once and treat pixels beyond the capture as zero;
do not clamp edges or renormalize at borders. The caller owns padded capture,
painter placement, submission and in-flight retirement. Exact zero radius uses
the ordinary source path. This operator alone does not enable Scene/Python Blur,
shadow, stacks, group isolation or an additional semantic scope model.

Three portable RGBA8 attachments retain 24 bits per premultiplied component
between horizontal and vertical passes. No float render-target extension or
color transfer is assumed. The seven retained textures cost 28 bytes per tile
pixel, excluding source capture and in-flight resources. Three programs are
lazy/shared across scopes; clean calls allocate/upload/encode nothing. An
opacity-only edit uploads 16 bytes and reuses the blurred color. Invalid
preparation preserves the prior valid scope; abandoned encoding requires
invalidation. A reduced neutral budget can retire the disposable texture cache.

The native ignored test
`gpu::gaussian_blur::tests::rgba_gaussian_pixels_match_independent_reference_and_retained_revisions`
compares all channels and packed intermediates against a full independent 2-D
convolution, including asymmetric color/alpha, faint edges and maximum sigma.
Replacing actual shader RGB with alpha must make it fail. The JS oracle has a
literal rational 7x7 impulse witness. `scripts/gaussian-blur-qualification.mjs`
borrows the normal browser host's device for explicit test readback on WebGPU
and WebGL2; the maximum-radius field has independent full-support point probes.
These are raster correctness/resource checks, not public animation, source
capture/placement, group semantics or physical-device performance evidence.

### Shared Blur authoring and capture

`Blur` is an immutable finite built-in with a checked `radius`. It uses the same
`EffectDefinition`, generational attachment, `EffectUpdate`, ordinary radius
channel, completion and replay publication as Glow. `EffectRadius::Scene` resolves
against the full output view; `Pixels` uses final physical output pixels. Radius
is Gaussian standard deviation, independent of object scale. The retained LDR
profile supports sigma through 64 pixels with full square support `ceil(3*sigma)`
and transparent extension. Zero radius restores the ordinary source submission.

```rust
use noon::effects::{Blur, EffectUpdate, Pixels};
// Given a shared Rust Scene and its ordinary Mobject:
let soften = Blur::new(EffectUpdate::default().radius(Pixels(10.0)))?;
scene.add_effect(&image, soften, "soften")?;
let mut target = image.target_editor()?;
target.set_effect("soften", EffectUpdate::default().radius(Pixels(20.0)))?;
```

The equivalent Python declaration is
`image.add_effect(Blur(radius=Pixels(10)), name="soften")`, with
`image.animate.set_effect("soften", radius=Pixels(20))` using the ordinary target
builder. Complete paired scenes are `noon::example_scenes::blur::program` and
`web/python/examples/blur.py`; the direct WASM factory is
`createDirectBlurSmokeRenderer`. Glow-only fields on Blur fail in shared Rust,
including explicit same-valued writes. Glow conveniences also reject a Blur
attachment named `glow`. Attachment kind changes require a new generation.

The existing isolated source capture and painter placement now accept both
finite definitions. Glow retains its alpha-mask operator; Blur filters all four
encoded-premultiplied RGBA channels through the qualified Gaussian operator.
Common capture accounting includes 32 bytes per pixel for Blur's source and
seven filter textures, plus source MSAA where required. Each warmed scope retains
one boxed finite filter state. Clean publications upload and encode nothing.
The same sparse row publication supplies native and direct Rust/WASM hosts.
The genuine Python worker envelope is generation 20 and carries a checked tagged
effect definition; unsupported fields are rejected at that external boundary.
No compatibility aliases, semantic identity allocator or effect clock is added.

`public_scene_blur_rgba_pixels_seek_neutral_and_removal` compares normal public
Scene output against an independent direct two-dimensional RGBA convolution of
separately rendered ordinary sources. Circle, rectangle, asymmetric image,
mirrored stroked path, alpha gradient, text and math profiles cover scene/pixel
units, partial opacity, forward/backward seek, neutral
submission, exact removal and zero-work clean frames. Local Apple M6/Metal
qualification produced 70 full-image samples across 14 profiles with maximum
RGBA8 error 1. Browser parity and merge qualification remain separate gates;
`scripts/blur-qualification.mjs` requires identical real Rust/WASM and Python
pixels on WebGPU and WebGL2, including a visible outside-source Blur witness.

This slice admits one finite attachment per qualified 2D leaf. Composed groups,
ordered stacks, shadows, backdrops, HDR/bloom and custom WGSL remain owned by
#1897 and are not implied by the Blur operator.

### Shared colored soft shadow

`Shadow` attaches through the same finite definition, identity, parameter update,
ordinary timeline channels, completion and publication as Glow/Blur. Its default
is black with alpha 0.5, sigma 0.15 scene units and offset (0.1, -0.1) scene units.
Color alpha controls shadow opacity. `radius` is Gaussian standard deviation;
zero sigma is a hard shadow, while zero color alpha is neutral. Shadow rejects
Glow intensity/source fields; Glow and Blur reject offsets. A Shadow named
`glow` cannot be activated by GlowPulse or edited with Glow conveniences.

```rust
use noon::effects::{Shadow, EffectOffset, EffectUpdate, Pixels};
let depth = Shadow::new(EffectUpdate::default()
    .radius(Pixels(4.0)).offset(EffectOffset::Pixels([12.0, -10.0])))?;
image.add_effect(depth, "depth")?;
let mut target = image.target_editor()?;
target.set_effect("depth", EffectUpdate::default()
    .offset(EffectOffset::Pixels([-12.0, 12.0])))?;
```

Python uses `image.add_effect(Shadow(radius=Pixels(4), offset=Pixels((12,-10))),
name="depth")` and `image.animate.set_effect("depth", offset=Pixels((-12,12)))`.
A plain offset pair uses scene units. Both unit modes use +X/right and +Y/up in
the view, independent of object rotation/scale. Unit changes are discrete and
cannot interpolate. Fractional offsets use transparent bilinear extension;
the backend rejects offsets that lose more than 1/256 pixel on f32 conversion.

The renderer convolves painted alpha using the existing RGBA Gaussian, then
composites the original encoded-premultiplied source over its offset colored
mask. The RGBA8 Gaussian output alpha is quantized before bilinear translation.
Paint alpha belongs in the captured mask. Final object opacity is removed from
capture and applied once to the complete result; `set_object_opacity` exercises
this distinct operation, while `set_opacity` edits paint alpha. Visual padding
does not change layout or picking. Source/translated Gaussian support use one
conservative local capture tile, including geometry outside the viewport.

Shadow adds one RGBA8 composition image to Gaussian's seven filter textures.
Common capture charges 36 bytes per tile pixel, plus MSAA for general sources.
Programs are lazy/shared. Warm color edits only upload the 32-byte composition
uniform and reuse source/convolution; clean calls encode/upload/allocate nothing.
Removal retires the derived resources. Queue submission/recovery remain with
the normal host; no GPU readback runs during ordinary playback.

`public_shadow_pixels_offset_animation_hard_shadow_and_retirement` compares
public Scene output with direct 2-D convolution, independent bilinear translation
and source-over composition. The corpus includes analytic shapes, asymmetric
image, mirrored stroke, alpha gradient, text/math, scene/pixel units, partial
object opacity, seek, hard shadows, neutral/removal and retained counters.
`offscreen_shadow_support_and_foreground_painter_order_match_extended_field`
uses an independently generated extended RGBA field: entirely offscreen sources
cast into the viewport behind foreground geometry on a colored background.
Real RGB-from-alpha and reversed-offset WGSL negative controls must fail this
oracle before the exact restored shader is requalified.

Paired programs: `noon::example_scenes::shadow::program`, the native `shadow`
example and `web/python/examples/shadow.py`. `scripts/shadow-qualification.mjs`
requires identical direct Rust/WASM and Python images on actual WebGPU/WebGL2,
visible outside-source shadow, animated radius/color/offset, exact neutral/removal
and idle source completion. These are qualification commands, not a claim of
physical-device pacing or completion of the remaining M0–M5 scope.
