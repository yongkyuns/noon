# Visual-effects contract fixtures

## Candidate typed WGSL profile

`WgslEffectProgram` prepares immutable trusted source and a versioned
`PaintedPointwiseV1` schema. `ShaderEffect` uses the ordinary named attachment,
parameter animation, removal and seek paths. The shader receives straight encoded
RGB and returns RGB plus a coverage multiplier; the renderer associates coverage
and applies object/group opacity once. This finite profile uses compatibility
color output, not HDR or arbitrary render/compute passes.

Declare local normalized, scene or output-pixel coordinates and any authored-time
or seed dependency. Typed parameter keys bind to their defining schema; edits
reuse immutable programs. Source point morphs are unsupported, and composed groups
require scene or output-pixel coordinates because they have no intrinsic local
box. Invalid source/schema/coordinate declarations reject before mutation.

See [the animated material lesson](../../web/python/examples/showcase_shader_material.py),
[the authored-time example](../../web/python/examples/shader_time.py), and their
[shared Rust builders](../../crates/noon/src/example_scenes/shader_sweep.rs).
`node scripts/shader-time-qualification.mjs` checks ordinary Rust/Pyodide pixels,
an independent coverage reference, different presentation cadences and seek/replay
on WebGPU and WebGL2. It requires a clean renderer-smoke browser package.

Late Rust attachments use `live.prepare_effect_attachments(...)` and yield
`ContinuationStep::Publish`: native, direct-WASM and offscreen hosts prepare owned
immutable modules before the normal publisher revalidates and commits the batch.
The existing retention barrier then admits the frame before source resumption.
Parameter edits and detached animation targets need no new module preparation;
headless prepared prefixes explicitly reject this resource-dependent operation.
The shader-time qualification also checks real GPU compilation failure and retry.

Python-worker attachment publication still needs the same pre-publication resource
barrier. Native Python, broader recovery and the physical-device performance
matrix are also unqualified; #1897 owns promotion.

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

## Seeded procedural noise paint

`NoisePaint(low, high, seed=0, frequency=(8, 8), phase=(0, 0))` supplies a smooth
two-color local field through ordinary `set_fill` and `.animate.set_fill`.
Rust uses checked `NoisePaint::new` and `Mobject`/`LiveSession::set_fill_noise`.
Complete paired scripts are `noon::example_scenes::noise_paint` and
`web/python/examples/noise_paint.py`, with the native `noise_paint` example and
direct Rust/WASM host. They animate frequency, phase, transparent colors and
opacity, hold, change to solid paint and return. No per-frame callback or extra
clock runs a static texture.

Frequency counts lattice cells along each normalized local-box axis; phase
shifts the lattice in cell coordinates. The lower-left box corner is zero.
The fixed u32 hash, its upper sixteen output bits and quintic interpolation
define seeded value noise; no platform random generator is involved. Lattice
indices wrap every 65,536 cells. Late projection splits phase into an integer
cell and fraction before f32 narrowing, including the rounded periodic seam.
Authored phase stays f64 and may retain multiple unwrapped cycles.

The renderer derives an axis-aligned lattice footprint from the physical pixel,
camera and inverse object transform. Widths below 1/256 cell use point sampling;
larger widths integrate the quintic field over a lattice-aligned box using at
most nine lattice values. Beyond one cell, a cubic fade reaches the seed
ensemble mean at two cells. This is the finite minification profile, rather
than a claim of arbitrary alias-free reconstruction. Centers clamp to the local
box before mapping; the filter window may extend beyond that center. The GPU
profile accepts positive representable frequencies up to 256 per axis.

Color interpolation uses encoded-premultiplied RGBA. Intrinsic color alpha,
fill opacity and final object opacity remain independent. Same-seed targets
interpolate frequency, phase and colors through the shared Fill channel;
solid/absent endpoints promote during interior samples. Exact endpoints retain
kind, seed and authored values. Changing seed is an immediate edit; attempting
to animate it rejects atomically. Existing stroke/text, callback-paint and
transient-family restrictions remain. Geometry, layout and picking stay intact.

The native `noise_paint_raster` oracle uses separate masked integer arithmetic
and piecewise Gauss quadrature over the point polynomial, without production
axis integrals or uniforms. It covers transformed primitives and concave paths,
detail/box/coarse profiles, authored times and exact rewind. A separate native
period-seam regression prevents phase packing from retargeting the other axis.
Existing whole-image oracles verify Blur/Shadow/both Glow modes and unchanged
geometry/strokes. Actual WGSL mutations ignoring seed and disabling filtering
must fail, then exact restored source must pass. Browser qualification is
`scripts/noise-paint-qualification.mjs`, with an independent BigInt/quadrature
reference, full u32 seed, phase-wrap and minification profiles on WebGPU/WebGL2.
Executed source/build evidence and unfinished promotion gates belong to the
owning PR/#1897; these are correctness/resource checks, not frame-pacing claims.

## Scene-unit stripes and crosshatching

`HatchPaint` fills an ordinary planar circle, rectangle or vector path with
parallel lines. `cross=True` unions a perpendicular family with those lines.
It uses the existing fill value, target animation and retained paint program.
The static pattern has no clock, generated image, offscreen pass or per-frame
callback. It is useful for uncertainty regions, material differentiation and
print-style diagrams whose meaning should remain visible without color alone.

```python
from math import pi
from noon import HatchPaint, Rectangle, Scene, linear

class UncertaintyRegion(Scene):
    def construct(self):
        region = Rectangle(width=4, height=2, stroke=None)
        region.set_fill(HatchPaint("#F4D35E", background="#132A3A",
                                  spacing=0.4, width=0.06, angle=pi / 4), opacity=1)
        self.add(region)
        self.play(region.animate.set_fill(
            HatchPaint("#F4D35E", background="#132A3A", spacing=0.4,
                       width=0.06, angle=pi / 4, phase=(1, 0))),
            run_time=2, rate_func=linear)
        self.wait(0.5)
```

The default background is transparent. Defaults are `spacing=0.25`,
`width=0.04`, `angle=pi/4`, `phase=(0,0)`, and `cross=False`. Rust uses
`HatchPaint::new(foreground, background, HatchPaintOptions { ... })`, followed by
`Mobject::set_fill_hatch` or `LiveSession::set_fill_hatch`. The full paired
playback scripts are `noon::example_scenes::hatch_paint` and
`web/python/examples/hatch_paint.py`; the native executable is `hatch_paint`.

Spacing is the perpendicular distance between line centers and width is the
line width, both in object-local scene units. The local origin anchors the
pattern. Angle is the line direction in counterclockwise radians from local
positive X. Resizing geometry adds more lines at the same spacing; scaling
the object scales the lines along with it. Phase advances each family in
periods; integral phase repeats the appearance. The second phase affects
crosshatching only. Phase and angle remain unwrapped authored f64 values, so
an animation can intentionally travel through multiple periods or turns.

For local point `(x,y)`, line coordinates are
`u=(-sin(angle)*x+cos(angle)*y)/spacing+phase.x` and
`v=(cos(angle)*x+sin(angle)*y)/spacing+phase.y`. Each family covers a band of
width `width/spacing` centered at every integer. The point rule is half-open;
zero width yields only the background and width equal to spacing yields only
the foreground. Spacing must be finite and positive, width must be finite in
`[0,spacing]`, angle and phase finite, and RGBA components in `[0,1]`.

The physical pixel is mapped through camera and inverse object pose, then
through the two line coordinates. Its axis-aligned bounding box in line
coordinates is the declared reconstruction domain. Each axis analytically
averages periodic band coverage; below 256 cells the calculation uses complete
periods plus bounded interval overlaps, and at 256 cells or more it uses the
exact period mean. A zero footprint uses the point rule. There is no small
pixel threshold that silently disables filtering. Crosshatch coverage is
`a+b-a*b` for the two axis averages, so crossing lines do not double opacity.
This finite box profile is distinct from integrating the exact rotated pixel
parallelogram. GPU admission checks finite projected coefficients and limits
the all-angle local coordinate extent to 65,536 cells. Unsupported values
reject before publication.

Foreground/background mix in premultiplied encoded sRGB, including their
intrinsic alpha. Fill and final object opacity remain independent. Matching
`cross` targets interpolate spacing, width, angle, phase, colors and opacity
through the existing Fill channel. Solid/absent endpoints lift onto the
pattern only for interior samples, and exact endpoints preserve their paint
kind and authored values. Changing `cross` is an immediate edit; animating
between one and two families rejects atomically. Existing text/stroke,
callback paint, spatial and transient-family restrictions remain explicit.

The native `hatch_paint_raster` and browser
`scripts/hatch-paint-qualification.mjs` use independent interval enumeration
and clipping, rather than production period decomposition, sampling functions
or uniforms. Coverage includes actual paths, primitive coverage, reflections,
nonuniform scale, rotation, physical pixel density, detail/coarse profiles,
transparent colors, duty endpoints, wrapped phase, ordinary animation and
backward seek. Existing retained counters check one local uniform update,
clean zero uploads and no additional texture allocation. Browser comparisons
exercise direct Rust/WASM and Python on WebGPU and WebGL2. Actual production
shader negative controls must fail the original numerical tolerance before
restored source passes. Executed results and exact source/build identities
belong in the owning PR/#1897; these checks do not establish physical-device
frame pacing.

## Radial paint authoring and qualification

`RadialGradient(stops, center=(0.5, 0.5), radii=(0.5, 0.5))` uses the same
ordinary `set_fill`, `.animate.set_fill` and independent opacity grammar as linear
paint. Rust uses `RadialGradient::new` and `set_fill_radial_gradient` on `Mobject`
or `LiveSession`. Complete paired scripts are
`noon::example_scenes::radial_gradient` and
`web/python/examples/radial_gradient.py`; native `radial_gradient` and the direct
Rust/WASM host run that same Rust program. The scripts animate an elliptical
field's center, radii, transparent stops and opacity, then transition to solid
paint and back, using ordinary play/hold completion.

Center is in the normalized local box, from lower-left zero to upper-right one.
Radii are positive lengths along those normalized axes. Equal radii form an
ellipse on a nonsquare box. The field coordinate is
`hypot((x-center.x)/radii.x, (y-center.y)/radii.y)`, with pad extension and the
shared right-continuous, encoded-premultiplied stop interpolation. These are
object-local coordinates, rather than effect radii in scene/output-pixel units.
Geometry transforms carry paint with the object; camera zoom does not edit it.

The finite planar circle/rectangle/path profile admits at most 16 stops and
radii/stop intervals representable by its GPU arithmetic. Center, radii, colors,
positions and fill opacity interpolate; matching stop counts are required.
Solid/absent endpoints promote to the radial topology during the interior of an
animation; exact endpoints preserve their original kind. Linear/radial kind
changes require immediate replacement. Invalid profiles reject atomically.
Stroke/text paints, transient family presentation and callback paint edits keep
the existing continuous-paint exclusions.

The native `radial_gradient_raster` oracle independently computes the ellipse,
premultiplied stop/time vectors and LDR quantization across transformed primitive
and path draws, with same-time/rewind and clean-upload checks. Replacing the
actual WGSL elliptical distance by a one-axis field must fail it; restoring the
shader must pass. `scripts/radial-gradient-qualification.mjs` exercises both
Rust/WASM and Python on WebGPU/WebGL2 against a separate temporal field formula.
These tests establish only the executed finite correctness profile; owning PR
evidence records exact revisions and promotion gates.

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

Compiled effect channels retain that exact attachment generation alongside the
object slot and parameter property. Track lookup, scheduler events, edits,
completion, replay and retirement use the captured address; an object/property
query without an attachment cannot alias an effect channel. Explicit slot
compaction relocates the derived object address and preserves semantic identity.
This execution prerequisite does not promote multiple attachments or group
filtering: the qualified public profile still admits one attachment per leaf.

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

**Historical M0 boundary:** the initial declaration-only checkpoint rejected
execution of an effect-bearing store rather than silently discarding appearance.
The execution, live mutation, animation and composed-scope integration described
in the subsequent qualification sections replaces that early gate. Unsupported
profiles still fail before publication; state replacement now carries its checked
attachment values through the ordinary semantic transaction.

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

## Retained ordered raster operators

`EffectStackFilter` composes up to eight finite Glow, Blur and Shadow operators
over one caller-owned padded RGBA8 capture. Existing single-effect capture uses
this same implementation. Each active pass consumes the preceding premultiplied
encoded-LDR result. Painted glow uses its alpha; silhouette glow uses the original
declared geometry mask. Intermediate outputs quantize to RGBA8. Final scope
opacity applies once, on the last non-neutral pass. An entirely neutral stack
returns ordinary source submission without copying or allocating GPU images.
If unit conversion underflows a positive Blur radius to exact output sigma zero,
that Blur is also an identity; it cannot claim a nonexistent filtered output or
take final opacity away from the preceding active pass.

The caller must supply the complete cumulative padded extent, original mask,
source-content revision, and normal submission/abandoned-encoder lifecycle.
These are renderer operators, not public authored-stack or composed-group
admission. Ordered semantic target activation, capture bounds, painter isolation,
paired creator examples and their publication/lifecycle qualification remain M3.

Programs are shared; per-pass uniforms and intermediates are independent. A
parameter edit leaves earlier passes intact and invalidates its actual suffix.
Retained filters charge 12/28/32 bytes per pixel for Glow/Blur/Shadow, excluding
source/mask, MSAA and host in-flight resources. Warm neutral slots remain charged;
truncation/clear retires them. Every later-pass profile/capability/budget rejection
precedes prefix mutation or queue writes. Clean calls upload/encode/allocate
nothing. No physical-device performance guarantee follows from these counters.

`gpu::effect_stack::tests` includes literal CPU witnesses, sparse invalidation,
aggregate budget, neutral, retirement and late-failure tests. Its ignored native
pixel test uses a separate direct 2-D convolution/compositor and checks every
prefix and RGBA channel, with a predeclared two-byte bound for up to three
mixed quantized operators and four bytes for the eight-Gaussian boundary witness
(successive RGBA8 quantization can accumulate). It covers repeated kinds, both orders, original silhouettes,
transparent input, fractional signed shadows, scene/pixel units, sigma 64, final
opacity, clean exact repeat and invalidation recovery. These tests qualify the
operator contract; they do not assert public group or animation-stack support.

### Nested original coverage and private Visibility

Nested capture keeps painted output and original coverage separate. An ancestor
silhouette consumes the child's original coverage before authored Blur/Glow,
with child scope opacity applied once after that child is composed. Dissolve's
private Visibility binding clips both streams using the same normalized source
domain and seeded mask. An ordinary named NoiseDissolve remains a painted filter;
it does not acquire the private lifecycle role or clip original coverage.

The immutable compiled attachment column preserves the exact trailing private
binding generation through initial, prepared and target lowering. Parameter
updates cannot change that role. The genuine worker boundary carries it in
protocol 27; direct native and Rust/WASM execution remain typed in process.

The nested native raster corpus checks partial masks, empty/full endpoints,
reverse progress, cleanup, ordinary named-filter witnesses and clean reuse
against scalar noise and full two-dimensional convolution references. The normal
Rust Scene/LiveSession capture regression separately checks leaf and family
Dissolve under an ancestor silhouette, including exact empty output and rewind.
These native checks do not establish browser or physical-device pacing support.

The paired `scope_opacity` creator programme additionally animates child and
ancestor opacity, including a returning rate, then dissolves and restores the
child beneath an ancestor silhouette. `scope-opacity-qualification.mjs` samples
50 authored times through the normal direct Rust/WASM and Python-worker paths
on WebGPU and WebGL2. Literal rectangles, independently evaluated seeded fields
and full two-dimensional Gaussian composition define its whole-frame reference,
with a predeclared two-byte bound. Empty endpoints are exact; returning samples
and completed restoration compare byte-for-byte with their earlier same-backend
frames. The same Rust programme runs through native fixed-time export. The
composed, scope-opacity, Dissolve and named NoiseDissolve browser scripts retain
raw images and reports in CI. These are correctness and reuse checks; the
physical-device frame-pacing gate remains separate.

### Appearance replacement and saved state

Ordinary object/family `become` replaces ordered attachment values while retaining
receiver object identity. A saved copy is an independent semantic snapshot;
restoration creates fresh receiver-owned effect generations, retires the old
bindings and leaves the saved copy unchanged. An exact self replacement retains
its bindings. Nested and unequal families reuse the existing topology/alias
reconciliation; target object IDs never transfer to the receiver.

Live replacement captures coherent effective effect parameters and composed
opacity, including after completion and rewind when authored endpoint values
differ. Private Visibility remains a distinct trailing role beside named mask
filters. Driver/callback/segment publication barriers still apply; copying a value
does not copy animation ownership. A stale attachment-column capture produces the
typed `effect.capture_mismatch` diagnostic. Deferred geometry/property writes and
terminal attachment retirements publish as one transaction, so late isolation
failure retains the original scene, resources, handles and runtime frame. Stack
admission checks the final eight-binding profile after retirement.

The paired `effect_restore` programme saves an ordered leaf stack and a composed
family, changes parameters, removes attachments, restores, edits the saved copies
without changing displayed output, restores those edited values, and cleans up.
Native fixed-time export and `effect-restore-qualification.mjs` compare complete
restored frames exactly with the earlier same-backend frame and check independent
interior alpha witnesses. Existing operator references separately qualify the
filters. Capture raster viewports follow the current logical footprint; retained
texture capacity may grow without changing that projection or restored pixels.

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


### Bounded square-cell sampling

`Pixelate(size=Pixels(12))` attaches to an ordinary object or isolated composed
family with `add_effect(..., name="detail")`. Animate the same checked size using
`obj.animate.set_effect("detail", size=Pixels(1))`. The executable Rust and Python
`pixelate` lessons reduce detail, reach exact identity, change units immediately,
and remove the treatment through ordinary scene/live operations.

Cells follow screen axes and are anchored at the original source's top-left
projected bounds, excluding capture padding and retained texture capacity. The finite raster profile rounds cell lengths and the source anchor to
1/256 output pixel (nearest, ties away from zero) and computes cells with signed integer arithmetic, so
decimal lengths and negative padding have stable ties on every backend. Each
cell selects the nearest input texel at its mathematical centre; texels beyond
the preceding image are transparent. All encoded-premultiplied channels sample
together. Final scope opacity applies once. A resolved size at most one output
pixel bypasses the filter exactly. The finite backend profile accepts sizes up
to 128 output pixels and conservatively expands each input's contribution bounds
by the cell size. Ordered filters consume the preceding output; they retain the
original source anchor. Geometry, layout and picking remain authoritative.

A plain size uses scene units resolved through the uniform planar view; `Pixels`
means final output pixels. Object rotation/scale affects source geometry, while
cell axes and resolved scene/pixel length follow the view. Unit changes are
immediate value edits and cannot interpolate; nonfinite/negative sizes reject.
The same `Size` channel, generational bindings, copy/restore and driver arbitration
serve native Rust, direct Rust/WASM and the Python wrapper. Only the genuine
worker boundary advances to protocol 28; older envelopes reject atomically.

The retained sampled-image owner is shared by seeded masks and cells. Programs
and per-scope images survive parameter edits; clean preparation uploads and
encodes nothing. There is no extra scene, clock or renderer. Native independent
cell-rectangle references cover RGBA/BGRA sources, fractional/negative origins,
transparent borders, exact identity and warmed reuse. Ordered convolution/mask
references exercise both operator orders. The authored native export and paired
browser corpus use literal colored rectangles and alpha composition, excluding
only a declared one-pixel source-AA band from the source oracle; every texel of
the sampling operator is independently checked separately. The browser helper
retains complete paired images and reports. This is a distortion filter over its
explicit object/group input; backdrop dependencies, HDR/bloom and custom WGSL
remain separate #1897 gates. Source/build-qualified results live in its issue/PR.


### Clipped backdrop replacement operator prerequisite

The renderer's `BackdropCompositeFilter` consumes three explicit, aligned
encoded-premultiplied RGBA inputs: processed painter prefix, target paint before
final owner opacity, and the target's original coverage. It clips paint to that
coverage and partitions the panel into painted and glass-only areas. It first
attenuates the existing destination by covered panel opacity, then adds the
prepared contribution. This replaces the backdrop through transparent content;
ordinary source-over would duplicate background alpha. Panel opacity applies
once to the entire result. This finite profile uses RGBA8/BGRA8 unorm,
single-sample 2D inputs and a single-sample output attachment. It applies no
additional color transfer and does not qualify HDR or sRGB attachments.

Two retained RGBA8 images cost eight bytes per capture pixel. Three lazy shared
pipelines prepare both images in one MRT pass and place them with erase/add
draws. Warm input/opacity edits reuse the images, buffer and pipelines; placement
changes upload only the uniform. Clean preparation uploads and encodes nothing.
Cold zero opacity allocates nothing; warmed zero opacity retains and charges its
resources but emits no draw. Retiring a scope releases them. All checked input,
budget, placement and revision failures precede mutation; abandoning preparation
requires invalidation before retry, as for the other retained raster operators.

`cargo test -p noon-render-wgpu --lib gpu::backdrop_composite::tests --
--include-ignored --nocapture` checks retained lifecycle/locality and actual GPU
pixels against an independent f64 scalar layer-area mixture. RGBA/BGRA input and
output profiles cover transparent/nonblack fields, soft coverage, paint outside
the clip, opacity endpoints, negative placement and viewport clipping. The byte
bound is two per channel; zero-opacity and fully offscreen images are exact.
Actual corrupted shaders prove the same oracle rejects ordinary source-over
and double scope opacity. Set `NOON_BACKDROP_ARTIFACT_DIR` to retain every
input, actual/reference RGBA buffer and exact profile, including seeded defects.
CI runs these tests on its raster adapter and retains these raw artifacts.

This operator alone does not select or validate painter-prefix dependencies or
expose a creator-facing attachment. The finite integrated profile below supplies
shared admission, local invalidation, capture ordering and creator-facing
execution. Ordinary object/group filters keep their existing input roles.


### Painter-prefix capture prerequisite

`PainterPrefixCaptureFilter` records a bounded, explicitly selected painter
prefix into an input texture. It uses the same retained geometry, text, images,
original draw order and source-capture encoder as ordinary isolated effects. Its
request names an existing target painter anchor, an immutable candidate list,
physical capture origin/size, source-content revision and the consuming view's
encoded-premultiplied clear color. Prefix output is a separate input; preparing
it does not replace or repaint the target.

Candidates must be unique, strictly increasing in current painter order, and
strictly before the anchor. Spatial gaps and an empty prefix are valid here;
ordinary isolated groups still require their complete contiguous intervals.
Previously prepared leaf filters and outermost earlier isolated groups contribute
their actual retained results, including each group's final opacity once. A
selected group replacement must contain every member of its interval in order;
a missing member cannot consume a different following candidate. Stale child
outputs reject before committing a new prefix image.

The execution/publication caller owns dependency discovery and must supply the
complete expanded-window closure, including needed offscreen content and whole
isolated intervals. It advances the source revision for selected content, order,
mesh or text changes. The operator validates the supplied selection; it does not
prove that an incomplete spatial query selected every required dependency. It
owns no scene membership, identity allocator, semantic values or clock.

This RGBA8/BGRA8 unorm profile captures an exact physical window using a
four-sample source attachment and a resolved image: twenty retained texture bytes
per pixel. Prefixes keep the same view clear color through transparent gaps and
outside the viewport. Warm fixed-window work reuses textures, buffers and image
bindings; clean preparation with the same immutable selection/revision performs
no uploads, allocations or passes and checks only its retained key. Parameter
changes remain local to this selected capture. Retiring or recreating a scope
releases its derived resources. Hosts own submission and in-flight budgets;
abandoned preparation requires invalidation before retry. An existing GPU resource
identity binds each scope to its renderer; replacement requires a fresh/cleared
scope, and a new device also requires a new filter. Old renderer image bindings
cannot silently follow a new context.

`cargo test -p noon-render-wgpu --lib gpu::source_capture::prefix::tests --
--include-ignored --nocapture` checks order/admission, atomic failures, reuse,
resize, revision exhaustion, earlier filtered inputs and stale-input rejection.
Actual RGBA/BGRA captures compare against literal independent image/rectangle
layers on transparent, opaque and translucent nonblack backgrounds, including
negative origins and windows outside the viewport. The accepted byte bound is
two per channel; empty or fully offscreen captures are exact. A real wrong-clear
capture proves the same reference rejects a transparent-clear substitution and
recovers through normal invalidation. `NOON_PREFIX_ARTIFACT_DIR` retains complete
actual/reference RGBA buffers and exact fixture profiles; CI runs and retains the
corpus. These are capture/filter composition proofs over an independently defined
fixture, not proof of arbitrary source rasterization or physical frame pacing.

This capture prerequisite is used by the integrated finite `BackdropBlur` profile
below. Its resource identities remain derived GPU identities, never semantic
membership or an alternative clock.

### Finite painter-prefix glass profile

`BackdropBlur(radius=Pixels(12))` processes the earlier painter content beneath
an object or one isolated composed family. Attach it with
`card.add_effect(..., name="glass")` and animate it with
`card.animate.set_effect("glass", radius=Pixels(18))`. Rust uses the same checked
`BackdropBlur` value, generic named attachment and family method-target operation.
A plain radius uses scene units; `Pixels` uses final output pixels. Radius is the
Gaussian standard deviation under the existing finite Blur kernel profile.

The target's original coverage masks the background treatment, independently
of target paint alpha. Preserve child object opacity in that coverage; apply the
owning scope's final opacity once. Its own path, image and text paint stays sharp.
A trailing private Visibility mask treats both the paint and original coverage,
so a complete dissolve removes the background treatment too. Radius zero bypasses
exactly to ordinary output; removing a neutral attachment is also exact.
The mathematical replacement/color profile above remains authoritative: this is
encoded-premultiplied RGBA8/BGRA8 unorm composition, not the later linear HDR/bloom
profile.

Admission uses the shared semantic/compiled dependencies and Runtime spatial
index. Prefixes use physical painter anchors, include required offscreen inputs
and earlier qualified filtered contributions, and prepare in dependency order.
Unsupported spatial/external prefix inputs, noncontiguous/aliased isolation,
nested backdrops, earlier backdrop prefix inputs and mixed ordinary-filter/backdrop stacks fail the finite
profile's checks. The checked authored stack is one backdrop plus optional
private Visibility. Effective animated prefix-capability failures reject GPU
preparation without replacing the retained output/resources; the Runtime frame
can already have advanced. This is not a Runtime rollback guarantee.

The paired `glass_lens` Rust/Python examples move a mixed path/image/text card
over a prepared checker image, animate radius, neutralize/remove/reattach, and
dissolve out/in through ordinary authored segments. Native `noon-export` captures
that same Rust live program. `scripts/glass-lens-qualification.mjs` compares all
pixels at 22 authored times on each of WebGPU and WebGL2; exact neutral/removal
and dissolve restoration supplement a literal full-out background oracle.
These are frontend/composition checks, not independent source-raster proofs.
The native coordinator corpus separately checks padded direct 2D Gaussian
references, opacity/masks, offscreen inputs, camera/physical resize, resource
plateaus and real wrong-input/bounds/sampling controls at a two-byte bound.

`scripts/glass-lens-recovery-qualification.mjs` uses actual browser device
or context destruction. Direct Rust/WASM survives repeated viewport churn,
a zero-sized surface and GPU replacement at the same authored state. Completed
Python source replay agrees with forward Rust output, including resized views.
WebGL workers recover in place; WebGPU workers use an explicit host `restart()`
after confirmed device loss. Idle WebGPU worker loss does not currently emit a
callback diagnostic in this profile: automatic idle-loss notification/recovery
is not qualified. Recreation must retain the current shared scene/time without
a repair seek, retire old workers and reproduce the previous pixels exactly.
A paused nonneutral backdrop is observed for 150 ms with no extra presentations
or pending updates; that bounded observation is not a physical latency or
performance claim. Direct live-program hosts retain their existing monotonic
source progression contract; this test does not add backward live-source seek.

These finite correctness/recovery profiles do not close #1897. Remaining host,
physical-device performance and failure/capability qualification, scene HDR/bloom,
public typed WGSL extensions and creator support documentation remain required.

### Linear HDR bloom renderer prerequisite (#1897 M4, not scene completion)

`gpu::HdrBloomFilter` processes an actual linear alpha-associated
`Rgba16Float` image. Its caller must provide finite nonnegative radiance,
alpha in [0,1], RGB <= 64*alpha and transparent black when alpha is zero.
It cannot reconstruct emission from an already clamped compatibility frame.
No scene property, Python placeholder, second scene/clock or browser transport
is introduced by this prerequisite. Scene-owned emission, complete linear
compositing, view attachment/animation, paired public authoring, seek/export and
browser capability/recovery evidence remain required before HDR/bloom is a
supported creator feature. The ordinary encoded-premultiplied LDR path does not
call this operator and retains its existing color contract.

The finite operator is deliberately specified independently of its WGSL:

1. For premultiplied linear source `S`, use Rec.709 luminance
   `L=.2126*S.r+.7152*S.g+.0722*S.b`. Bright extraction is zero for
   `L<=threshold`; otherwise `E=S*(L-threshold)/L`. Threshold is [0,64]
   in alpha-associated linear luminance, rather than unassociated brightness.
2. Blur all four extracted channels with a normalized Gaussian of standard
   deviation `sigma` [0,64] in final physical pixels and support
   `ceil(3*sigma)` in each direction. Outside the supplied image is transparent
   black. The full kernel remains normalized at edges. At sigma zero the kernel
   is an exact point sample. The caller owns the padded view extent.
3. With intensity `i` [0,8] and blurred extraction `G`, retain linear HDR
   `C.rgb=S.rgb+i*G.rgb` and
   `C.a=S.a+(1-S.a)*clamp(i*G.a,0,1)`. RGB is not clamped to one or alpha.
   Intensity zero keeps the actual HDR base and skips extraction/blur work.
4. Resolve once: transparent output is exact zero; otherwise unassociate by
   `C.a`, apply componentwise Reinhard `x/(1+x)`, then the piecewise sRGB
   transfer (12.92*x below/equal .0031308, otherwise 1.055*x^(1/2.4)-.055),
   and associate once by `C.a`. The final `Rgba8Unorm` bytes are explicitly
   encoded-premultiplied SDR. They require a presentation path without an
   additional sRGB transfer. The separate float output remains linear HDR.

A fresh disabled-Bloom scope prepares only combination and display resolution:
one float image and one unorm image, **12 retained image bytes per pixel**.
First enabling Bloom adds extraction and horizontal/vertical blur images and
programs, bringing the image total to **36 bytes per pixel**. These remain warm
until resize or clear, avoiding allocation churn during animated intensity edits.
Source images, the 1056-byte uniform, program/binding objects and in-flight host
frames are excluded. Adapter/device support and parameters/extent/budget are
validated before changing coherent state; rejection preserves the last output.
Each scope belongs to its creating filter/device until cleared.
A clean scope uploads/allocates/encodes nothing. Intensity edits upload 16 bytes
and encode only combination/resolution; radius edits reuse extraction; source
or threshold changes invalidate their real dependency closure. Disabled Bloom
defers extraction/blur work. Source replacement rebinds only its consumers;
resize prepares two images when disabled or five when enabled, reusing programs.
Clearing retires all scope images/bindings; an abandoned encoder requires explicit
invalidation before retry. Submission and device recovery remain host responsibilities.

The native GPU qualification uses independent f64 full two-dimensional
convolution, not the production separable kernel or GPU intermediate readback
as expected data. It covers 84 profiles: impulse, asymmetric varying alpha,
edge energy, nonblack constant, black/translucent, near-black and transparent;
sigma 0/.65/2.25/64 and intensity 0/1.5/8. All source values are exactly
representable as float16. Per-channel linear acceptance is
`abs(actual-reference) <= .006 + .004*abs(reference)`; final display acceptance
is two bytes. This combined absolute/relative bound accounts for float16 pass
rounding and is not an absolute .006 bound. Readback must actually retain RGB
above one and exactly preserve an empty input. Real GPU source replacement,
encoder abandonment/retry, resize, retirement/recreation and cross-device
rejection supplement structural resource/locality tests. Actual mutated WGSL
programs that clamp HDR early, apply output transfer twice, or premultiply twice
must fail the same display oracle. CI retains actual float16 bytes, independent
f64 expectations, actual/reference display bytes, profile metadata, adapter
capabilities and negative-control captures. Native evidence does not establish
WebGPU/WebGL2 support or physical-device performance.

Retained capture retirement must precede painter-dependency sorting when a
publication shrinks after replay restoration or explicit runtime row reclamation.
Retired execution slots have no rank in the current publication. The structural
regression test grows five tail glow sources, publishes them, removes/reclaims
those rows before renderer publication, and repeats the cycle. Old capture
mappings/resources must retire, the surviving capture must remain coherent, and
a clean follow-up must do no work. Browser showcase replay separately exercises
the actual source-completion/restart path; successful first-pass playback is not
accepted as evidence that arbitrary-order replay succeeds.

The planar `Rgba16Float` draw path now specializes the existing source-color
helper to linear radiance: associate by effective alpha exactly once, preserve
RGB above one, and skip compatibility UNORM/Cairo source quantization. Analytic,
path and continuous-paint pipelines choose the same specialization. Other target
formats retain the original helper. An offscreen float scene target must never
inherit the WebGL drawing buffer's presentation decode; display transfer still
belongs after HDR resolution. This target contract expects **already-linear
packed source values**. Authored Color decoding, checked emission and semantic
view/Bloom controls still require integration; this is not a public scene HDR
feature or a claim of spatial/image/text/profile support.

The native float-draw oracle uses the ordinary frame preparer, uploads and painter
submission, with two pixel-aligned rectangle/path layers. Sixteen profiles vary
path/analytic routing, solid/continuous-paint programs, painter order and
transparent/nonblack clears. Independent
f64 source-over arithmetic accounts for color alpha, object opacity and appearance
before comparing every float channel. The produced float image then feeds the
real bloom operator; display expectations come from the independent 2D bloom
reference applied to the mathematical scene expectation. The actual previous
clamping shader is compiled into an otherwise identical float render pipeline
and must fail the linear oracle. A clean same-frame upload must remain zero.
This numerical raster-input proof does not substitute for the required paired
public Rust/Python authoring, authored-time/seek/export or browser HDR gates.

The in-progress authored view contract uses `ViewColorProfile` on the existing
semantic 2D camera. Compatibility is the default; LinearHdr declares the shared
linear-radiance/display contract. The ordinary semantic transaction validates the
camera role and supports transaction-local creation, staged reads and atomic
rollback. Profile selection lowers to the camera's existing compiled execution
slot. A static live edit uses the ordinary execution transaction, changes only
that slot's metadata/publication and leaves effective object rows untouched.
The Runtime publication borrows this metadata from its current compiled revision;
it introduces no second state map, scene, identity allocator or clock. Retained
replay exchanges the same compiled revisions, so profile queries follow authored
time without a renderer-owned history. Repeating an identical edit is a no-op.
Ordinary object creation allocates no view-profile shadow entries. Visual become
preserves the receiver's camera/view meaning; explicitly removing its camera role
clears the profile to compatibility.

Native Rust tests exercise initial/live lowering, pending camera creation,
compiled transaction staging, mixed valid/invalid rollback, receiver ownership,
clean no-op publications, unchanged unrelated frame rows and repeated replay.
The same publication now carries the compiler-derived primary camera identity.
`FramePreparer::prepare_publication` reads that view's current profile and decodes
authored sRGB RGB channels before linear association/blending. Alpha, appearance
and object opacity retain their existing meanings. Solid fill/stroke, path
background stroke and reveal-head packing use the same conversion; continuous
paint stops are decoded before premultiplied interpolation. Compatibility and raw
already-linear raster entry points preserve their existing packing contract.
A global profile change repacks derived colors once; subsequent clean frames
repack/upload nothing and an ordinary local color edit changes only its row.

`cargo test -p noon --test authored_view_color_raster -- --ignored --nocapture`
qualifies the typed native Rust Scene -> compiler -> Runtime publication -> planar
float draw -> real Bloom -> SDR path. Sixteen cases vary analytic/path sources,
solid/varying-gradient paint, painter order and transparent/nonblack linear
clears. Independently computed f64 sRGB decoding, premultiplied interpolation and
source-over provide the scene expectation; the separate full-2D Bloom reference
consumes that mathematical expectation rather than GPU readback. Every linear
channel must differ by at most 0.002; every display channel by at most 2 bytes.
An actual draw omitting the profile consumer must fail the linear oracle by more
than 0.05. A clean same-publication repeat must upload zero bytes. Raw pixels,
mathematical expectations and a manifest can be retained with
`NOON_AUTHORED_VIEW_ARTIFACT_DIR` and are collected by native CI.

This is a finite planar color-preparation proof, not a public HDR creator surface.
Composed filter captures and native/browser/export hosts still need profile
integration; finite ordinary retained input qualification follows below.
Checked emission, Bloom view attachment/animation, explicit view-ambiguity and
capability admission, paired Python/direct-WASM/export evidence and the remaining
M4/M5 gates are open. The authored test does not yet claim radiance above one,
physical-device performance, or browser HDR execution.


The canonical `RetainedFramePreparer` now propagates that same publication profile
through ordinary geometry, mixed scratch geometry, outlines and the candidate
visibility projection. Prepared geometry/retained views expose the derived color
profile. Raw/transport entry points explicitly retain compatibility packing; a
prior direct publication does not turn them into another profile authority.

For a float working target, straight sRGB bitmap texels are decoded before each
reconstruction tap is associated. Nearest, bilinear and Keys cubic (`a=-1`) sample
the resulting linear premultiplied values with their existing addressing rules.
Intrinsic reconstruction/clamping precedes final object opacity, applied once.
This matters for cubic overshoot: scaling taps before the intrinsic clamp changes
output and made bitmap objects disagree with image fills. Compatibility keeps its
existing reconstruction and opacity order. The HDR repair has an actual failing
old-order pixel witness; the 0.002 bound was unchanged.

Mask glyph coverage remains linear alpha; authored tint RGB is decoded at the
vertex. Color glyph atlas pages already use `Rgba8UnormSrgb`, so texture loads
perform the one input decode. HDR color texels are associated before bilinear
interpolation, then multiplied by linear tint and final alpha. Decoding the atlas
again or associating only after interpolation is incorrect. Compatibility shader
branches preserve their existing operations. The asset conversion needs no
additional retained texture images or per-frame pixel upload.

The authored-view integration test now runs the same 16-case independent planar
source/Bloom oracle through direct, canonical retained, and retained-visible
preparation. Its `retained_color_assets` module adds 24 real Rust Scene bitmap/
image-fill cases: three reconstruction modes, painter reversal, transparent/
nonblack clears, 24 exact opacity restorations, immutable texture reuse and clean
zero uploads. Every float channel uses the frozen 0.002 bound; display uses 2
bytes. A real omitted-asset-decoder shader must fail; the old opacity-order
shader also fails the independent reconstruction contract.

`glyph_working_color_matches_independent_tint_atlas_alpha_and_rejects_real_negatives`
runs eight float mask/color-atlas cases against independent f64 tap reconstruction
and tint/source-over arithmetic, including transparent colored texels and nonblack
clears. Actual compiled shaders for encoded tint/unassociated filtering and double
atlas decoding must fail. Empty dirty ranges upload zero glyph instance bytes.
The real Rust-authored `HDR` text scene adds three live tint/restoration phases
composited over ordinary geometry, with clean zero uploads and no geometry upload
for the text-color edits. The ordinary white-text compatibility render supplies
qualified glyph **coverage only**; the independent color/composition reference does
not claim to replace the font rasterization oracle. Native CI preserves the raw
float images, expectations, coverage source and manifests alongside the existing
linear-scene proof.

These input proofs do not yet qualify HDR composed/filter/backdrop/inset/spatial
captures, host format selection/final presentation, explicit view capabilities,
checked authored emission, Bloom attachment/animation or the public creator
surface. Rust/WASM compilation is distinct from browser execution evidence.
The complete M4/M5 host, time/seek/export, extension and performance gates remain
owned by #1897.


### Linear composed Blur capture prerequisite (#1897 M4)

The same retained Gaussian filter now specializes its derived storage by working
format. Compatibility retains the six packed RGBA8 reconstruction images and one
RGBA8 output (28 bytes/pixel). Linear Blur uses one RGBA16Float image per axis and
one RGBA16Float output (24 bytes/pixel), with one attachment per pass and no unit
clamp, packing, input decoding or output transfer. Its source is already linear
and alpha-associated. Radius/transparent-border normalization and the final
once-only scope opacity retain their existing semantics. Programs are shared per
renderer/format; each capture owns only disposable resources and revisions.

Composed and mixed source captures allocate their existing working format. Tile,
source, optional original-coverage and multisample budgets use that format's
actual texel size, and the ordered stack checks every pass before mutating its
prefix. Unsupported HDR Glow, Shadow, Pixelate, Dissolve, Visibility and backdrop
routes return an explicit preparation error; they do not run RGBA8 operators on
float radiance. Spatial and inset capture support remains a separate gate.

`gpu::gaussian_blur::tests::hdr` qualifies 36 already-linear float inputs against
an independent full two-dimensional f64 Gaussian and then the real Bloom/output
operator. It checks all float/display channels against the frozen bounds
`0.006 + 0.004 * abs(expected)` and 2 bytes, respectively. Four source patterns,
three radii and three opacities cover bright radiance, alpha, edges and low
values. A real compiled early-clamping shader must fail the same oracle. Resource
checks cover exact 24-byte storage, clean reuse, opacity-only final work, source
binding/revision changes, resize, invalidation, clear, format switches and atomic
budget rejection. A later neutral unsupported operator must reject before an
ordered prefix allocates or changes its coherent output.

`hdr_captures::authored_composed_and_mixed_blur_preserve_linear_color_and_local_revisions`
runs the ordinary Rust Scene/LiveSession -> source visibility -> retained capture
-> one/two ordered Blur passes -> painter placement -> real Bloom/SDR route.
Its 36 authored phases cover geometry groups and geometry/image groups, overlapping
alpha, a nonblack clear, radius/opacity/child edits, zero radius/opacity, unrelated
edits, exact restoration, fresh/incremental equality and zero clean work. The
reference independently decodes authored colors/asset texels, composes literal
pixel-aligned bounds, applies full two-dimensional convolutions and scope opacity
once. Native CI preserves raw float/display output and mathematical expectations.
These authored inputs are unit-valued: their above-one witness occurs after
Bloom. The separate raw-input proof establishes above-one radiance before Blur.

This slice does not establish public checked emission, camera Bloom authoring,
HDR text-capture qualification, view/host capability admission, browser execution,
export/recovery or physical-device performance. The complete M4/M5 goal remains
open in #1897.


### Authored camera Bloom and native capture integration (#1897 M4)

`Bloom` is a checked view-input definition on the existing Camera2D controller,
with radius in scene/final-pixel units, associated-linear Rec.709 threshold
[0,64], and intensity [0,8]. `EffectUpdate::threshold` has its own ordinary typed
track property; it shares capture, target intent, activation, reconciliation,
Runtime sampling and attachment generation with other effect parameters.
Explicit zero remains a write. Radius units cannot change during interpolation.

The initial view stack admits one Bloom attachment on a LinearHdr Camera2D.
Ordinary objects, families, mixed view/object stacks and duplicate view effects
reject before publication. Removing Bloom and leaving HDR in one transaction is
valid; changing to Compatibility with Bloom still attached rejects atomically.
A camera controller is non-drawable by semantic role. Its paint and opacity never
scale or become the view input. Renderer publications borrow effective view
attachments directly from that camera's ordinary Runtime row.

The same `HdrBloomFilter::prepare_view` resolves the selected camera's effective
values. A LinearHdr view without an attachment still resolves its linear base
with zero added Bloom. The ordinary native `SessionCapture`/`capture_frames` host
selects the working format from that publication, decodes the authored clear,
renders through the same retained renderer, and uses the same Bloom/SDR output.
Its metadata distinguishes `LinearHdrSdrRgba8Opaque` from compatibility UNORM
bytes. It checks float usages, blending, four-sample support, image budget and
its finite primary-2D/no-inset/no-spatial profile before allocating the HDR view.
Profile changes rebuild only disposable renderer state; they add no authored
scene, view-parameter map or playback clock.

The camera proof animates radius, threshold and intensity, samples forward and
backward authored times, checks exact seek restoration/removal and clean reuse,
and verifies nonzero camera-frame opacity cannot affect output. An actual wrong
opacity-uniform consumer must fail its independent full-2D f64 reference. The
normal capture-host proof covers persistent edits, zero energy, removal and
Compatibility/HDR/Compatibility transitions over a nonblack authored clear,
with independent display expectations and exact restored output.

This is native capture integration, not the complete M4 creator release. Checked
emission, native interactive/browser/Python host wiring, broader view/capture
capabilities, the public WGSL extension and remaining M5 gates stay open. Ordinary
retained hosts whose working format disagrees with the authored profile reject
explicitly. Cross-worker transport rejects Bloom until its view/profile metadata
is qualified. Existing video/file format admission has not been expanded by this
slice; HDR-derived SDR files need their separate output qualification.
