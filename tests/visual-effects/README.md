# Visual-effects contract fixtures

This directory specifies and independently checks the **M0 first-glow contract**
for [#1897](https://github.com/yongkyuns/noon/issues/1897). It is a test specification,
not another architecture, roadmap, renderer, or Python effects implementation.
`docs/architecture.md` remains authoritative; implementation checklists stay in the
issue. Inspection base: `9028672c1691d3a318c87da50e8b08e0802245f0`.

**Status:** the examples below are contract specimens, not supported/runnable
Noon effects today. M0 introduces no shipping API or shader. The Python reference
executes only a small mathematical/value-table oracle. Passing it does not qualify
a production implementation, Rust/Python parity, GPU output, or device speed.
M1+ must compare real engine observations to these expectations rather than use
this module to implement the feature.

## First-glow authoring vocabulary

Python's Noon-native extension names live in `noon.effects`: `Glow`, `GlowPulse`,
`Pixels`. Existing `noon` shapes, Scene, composition, `.animate` and rate functions
remain unchanged. Do not add these names to the Manim compatibility namespace.

| Operation | Python contract | Idiomatic Rust contract |
| --- | --- | --- |
| Persistent glow | `obj.set_glow(**patch)` returns `obj` | `Scene::set_glow(&obj, GlowUpdate)` / `LiveSession::set_glow`; fallible scene-owned publication |
| Generic attachment | `obj.add_effect(Glow(...), name="glow")` returns handle | `Scene::add_effect` / live counterpart, same definition and identity |
| Find canonical binding | `obj.effect("glow")` returns handle, missing is an error | `Scene::effect(&obj, "glow")`, same lookup |
| Partial parameter update | `handle.set(**patch)` returns handle | `set_effect(&handle, typed_update)` on Scene/live; schema checked |
| Target-state animation | `obj.animate.shift(...).set_glow(...)` | existing `target_editor`, new `target.set_glow(GlowUpdate)`, existing `declare_transform_to` |
| One attachment animation | `handle.animate.set(**patch)` | `declare_effect_to(&handle, typed_update, AnimationOptions)`, then ordinary `play_animation` |
| Temporary emphasis | `GlowPulse(obj, intensity=2)` is animation intent | `declare_glow_pulse(&obj, peak, AnimationOptions)`, then ordinary `play_animation` |
| Removal | `obj.remove_glow()` returns obj; `obj.remove_effect(handle)` returns obj | corresponding Scene/live operations; no animation or implicit time advance |

The new Rust operations above are design decisions, not existing methods. They
extend the existing scene-owned/live publication and declaration patterns; raw
`Mobject` edits currently do not publish into a running session. Do not introduce
an effect-specific executor, an async convention, or a fluent handle mutation
that silently bypasses coherent live publication. Fallible Rust scene mutations
return `Result<(), AuthoringError>`; attachment creation/query returns a typed
handle/result; target mutators follow their existing fallible convention.

`Glow` is an immutable declaration, not a GPU allocation. Mutable attachment state
belongs to the shared semantic scene. Construction, animation-builder creation,
and target editing have no live visual side effects.

### Canonical identity, defaults and partial updates

`set_glow` creates/updates exactly the attachment named `glow`. The generic
`add_effect(Glow(...), name="glow")` creates that same kind of binding; it fails
if the name exists. It is not an upsert. Additional names, such as `accent`, are
independent and ordered. The name `glow` is reserved for a Glow definition.
Unnamed generic attachments append independently and are reachable by handle.
Duplicate names, even with identical values, fail atomically.

Initial defaults: opaque white tint; radius **0.15 scene units**; intensity
**0.35**; source `painted`; fixed `reference` quality. Omitted fields retain their
values after creation. In particular, a subsequent `set_glow(intensity=1)` does
not change radius, color, source, order, or identity. An empty patch on an absent
binding creates the defaults; on an existing binding it is a semantic no-op.

`color` is straight encoded RGBA in [0,1]; radius is finite and nonnegative;
intensity is finite in [0,8]. Booleans, NaN, infinity, unknown parameter names and
wrong schema types are errors. Radius unit, source mode, quality and program
version are discrete. They cannot be interpolated across one animation; reject
such transitions before publication. Radius and intensity interpolate linearly
in parameter value after the ordinary time map; tint components use encoded
component interpolation. A `Pixels` radius may animate to another `Pixels`
radius, not to a scene-unit radius. No hidden unit conversion at activation.

Ordinary getters observe coherent effective parameters. Explicit authored
inspection remains separate. A neutral attachment remains queryable. Removing a
missing canonical glow is an idempotent no-op; a stale explicit handle is an
error, never a request to act on a new attachment with the same name.

### Complete introductory Python specimen (M1 + M2)

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
        self.play(dot.effect("glow").animate.set(intensity=0.0),
                  run_time=0.4, rate_func=linear)
        dot.remove_glow()
```

The normal source continuation resumes only after coherent logical completion;
no per-frame Python callback or custom render loop is needed. At authored times
0 / 0.75 / 1.5, position x is 0 / 1 / 2 and intensity is .35 / .775 / 1.2.
The pulse captures **1.2**, reaches 2 at t=1.8, and restores 1.2 at t=2.1.
The wait ends at 2.6; intensity is .6 at 2.8 and exactly zero at 3.0. Removal
then retires the binding without advancing time. The separate pulse vectors use
capture .35 to prove capture is an input, not a hard-coded introductory value.

### Complete reusable Rust scene specimen (M1 + M2)

This follows today's `LiveContinuation` / `ContinuationStep::Await` convention;
the ordinary native or direct-WASM host consumes the returned program. The
continuation advances only when an existing segment completes, not once per
frame. `GlowUpdate` and the effect declarations are the proposed additions.

```rust
use noon::{AnimationOptions, ContinuationStep, DeclaredAnimation, LiveContinuation,
           LiveProgram, LiveSession, ManimGeometryOptions, Mobject, RateFunction, Scene};
use noon::effects::GlowUpdate;

type Error = Box<dyn std::error::Error>;

pub struct LuminousExplanation {
    dot: Mobject,
    movement: DeclaredAnimation,
    pulse: DeclaredAnimation,
    dark: DeclaredAnimation,
    step: u8,
}

impl LiveContinuation for LuminousExplanation {
    type Error = Error;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Error> {
        let step = self.step;
        self.step += 1;
        let segment = match step {
            0 => live.play_animation(&self.movement)?,
            1 => live.play_animation(&self.pulse)?,
            2 => live.wait_segment(0.5)?,
            3 => live.play_animation(&self.dark)?,
            _ => {
                live.remove_glow(&self.dot)?;
                return Ok(ContinuationStep::Finished);
            }
        };
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn luminous_scene() -> Result<LiveProgram<LuminousExplanation>, Error> {
    let mut scene = Scene::new();
    let mut geometry = ManimGeometryOptions::circle(0.08)?;
    geometry.set_fill(1.0, 1.0, 1.0, 1.0)?;
    geometry.disable_stroke();
    let dot = scene.geometry(geometry)?;
    scene.set_glow(&dot, GlowUpdate::default().radius(0.15.into()).intensity(0.35))?;
    scene.add(&dot)?;
    let mut target = dot.target_editor()?;
    target.shift(2.0, 0.0)?;
    target.set_glow(GlowUpdate::default().intensity(1.2))?;
    let options = |seconds| AnimationOptions::new().run_time(seconds)
        .rate_func(RateFunction::Linear);
    let movement = scene.declare_transform_to(&dot, &target, options(1.5))?;
    let pulse = scene.declare_glow_pulse(&dot, 2.0,
        AnimationOptions::new().run_time(0.6).rate_func(RateFunction::ThereAndBack))?;
    let halo = scene.effect(&dot, "glow")?;
    let dark = scene.declare_effect_to(&halo,
        GlowUpdate::default().intensity(0.0), options(0.4))?;
    Ok(scene.into_live_program(LuminousExplanation {
        dot, movement, pulse, dark, step: 0,
    })?)
}
```

`GlowUpdate::default()` is an **empty patch**, unlike the full `Glow` definition's
defaults. The dark declaration owns intensity only; it must not restore the
construction-time position. Both specimens request the same filled white circle
without a stroke. This is a reviewed source contract, not compiled or raster
parity evidence for the unimplemented effects API.

### Mixed-feature review specimen (later finite scopes)

```python
import numpy as np
from noon import Circle, Group, ImageMobject, Scene, Text, VMobject, UP, linear
from noon.effects import Glow, GlowPulse, Pixels, Bloom
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
        halo = group.add_effect(Glow(radius=Pixels(12)), name="accent")
        scan = title.add_effect(ScanBand(width=0.2, phase=0.0), name="scan")
        self.play(halo.animate.set(intensity=1.4), run_time=1.0, rate_func=linear)
        self.play(scan.animate.set(phase=1.0), run_time=1.0, rate_func=linear)
        self.play(GlowPulse(dot, intensity=2.0), run_time=0.6)
        halo.set(intensity=0.25)
        self.wait(0.5)
        group.remove_effect(halo)
        bloom = self.output_view().add_effect(Bloom(), name="bloom")
        self.play(bloom.animate.set(intensity=0.0), run_time=0.3, rate_func=linear)
        self.output_view().remove_effect(bloom)
```

This is a complete **review specimen**, not an executable M0 example.
`effect_fixture.ScanBand` denotes the required future external extension fixture,
not an available package. Its finite mathematical/schema contract is: normalized
local x; `width` in (0,1], `phase` in [0,1]; yellow tint where
`abs(x-phase) <= width/2`, zero elsewhere; no history, extra bounds, resources or
clock. Source alpha clips the band. Points x=0/.5/1, width=.2, phase=.5 yield
0/1/0. M4 must implement that fixture outside central built-in dispatch and supply
independently supersampled edge witnesses. The registry/WGSL binding ABI and
Bloom's HDR operator are **not frozen by M0**. Ordinary attachment/animation
calling conventions are; do not pretend unspecified HDR behavior is qualified.

The following paired Rust choreography accepts the source fixture's Scene and
its four already-created ordinary handles. This makes the effect code independent
of optional text/image providers; it does not move source construction into an
effects engine. The fixture must use the same `Text("Signal")`, three path points,
2x2 straight RGBA8 bytes and circle as above. Existing `Scene::text(Text::new(...))`,
`Scene::image_rgba8`, geometry/path construction and family operations supply them.
Pin the regular DejaVu Sans Mono font, 48-point size, image sizing and source paint
in the eventual executable pair; matching these is a promotion gate, not a claimed
M0 rendering result. Rust's added effect handle is schema-typed; `GlowUpdate` and
`ScanBandUpdate` are values, never mutable copies of attachment state.

```rust
use noon::{AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram,
           LiveSession, Mobject, MobjectFamily, MobjectTarget, RateFunction, Scene};
use noon::effects::{Bloom, BloomUpdate, EffectHandle, Glow, GlowUpdate, Pixels};
use effect_fixture::{ScanBand, ScanBandUpdate};

type Error = Box<dyn std::error::Error>;

pub struct MixedExpression {
    group: MobjectFamily,
    dot: Mobject,
    halo: EffectHandle<Glow>,
    scan: EffectHandle<ScanBand>,
    bloom: Option<EffectHandle<Bloom>>,
    step: u8,
}

impl LiveContinuation for MixedExpression {
    type Error = Error;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Error> {
        let options = |seconds| AnimationOptions::new().run_time(seconds)
            .rate_func(RateFunction::Linear);
        let step = self.step;
        self.step += 1;
        let segment = match step {
            0 => {
                let change = live.declare_effect_to(&self.halo,
                    GlowUpdate::default().intensity(1.4), options(1.0))?;
                live.play_animation(&change)?
            }
            1 => {
                let change = live.declare_effect_to(&self.scan,
                    ScanBandUpdate::default().phase(1.0), options(1.0))?;
                live.play_animation(&change)?
            }
            2 => {
                let pulse = live.declare_glow_pulse(&self.dot, 2.0,
                    AnimationOptions::new().run_time(0.6))?;
                live.play_animation(&pulse)?
            }
            3 => {
                live.set_effect(&self.halo, GlowUpdate::default().intensity(0.25))?;
                live.wait_segment(0.5)?
            }
            4 => {
                live.remove_effect(&self.group, &self.halo)?;
                let view = live.output_view()?;
                let bloom = live.add_effect(&view, Bloom::default(), Some("bloom"))?;
                let change = live.declare_effect_to(&bloom,
                    BloomUpdate::default().intensity(0.0), options(0.3))?;
                self.bloom = Some(bloom);
                live.play_animation(&change)?
            }
            _ => {
                if let Some(bloom) = self.bloom.take() {
                    let view = live.output_view()?;
                    live.remove_effect(&view, &bloom)?;
                }
                return Ok(ContinuationStep::Finished);
            }
        };
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn mixed_expression(mut scene: Scene, title: Mobject, path: Mobject,
                        image: Mobject, dot: Mobject)
    -> Result<LiveProgram<MixedExpression>, Error>
{
    let inner = scene.family(&[MobjectTarget::Object(&path), MobjectTarget::Object(&image)])?;
    let group = scene.family(&[MobjectTarget::Family(&inner), MobjectTarget::Object(&dot)])?;
    scene.add_many(&[MobjectTarget::Family(&group), MobjectTarget::Object(&title)])?;
    let halo = scene.add_effect(&group,
        Glow::default().radius(Pixels(12.0).into()), Some("accent"))?;
    let scan = scene.add_effect(&title, ScanBand::new(0.2, 0.0), Some("scan"))?;
    Ok(scene.into_live_program(MixedExpression {
        group, dot, halo, scan, bloom: None, step: 0,
    })?)
}
```

The effect declarations on `LiveSession` above must stage through the existing
coherent declaration/publication path; the current cold `Scene::declare_animation`
helper must not be called against a running store to bypass that barrier. This is
a named M1/M2 prerequisite, not evidence that the methods already exist. The
output-view handle is a narrowly missing M4 semantic scope, not a new Scene or
camera. The spec's one logical source resumes after each existing segment.

For these paired sequences: the group halo reaches 1.4 at t=1, ScanBand phase
reaches 1 at t=2, the previously absent dot glow exists temporarily only on the
pulse interval [2,2.6], and the persistent group halo is .25 on [2.6,3.1]. It is
removed at 3.1; view bloom is then attached, animated to neutral and removed at
3.4. All group/leaf identities and painter order survive those attachment changes.
Bloom's visual model remains M4; these expected states constrain only its lifecycle.

For live edits, `LiveSession::set_effect(&halo, patch)` is the Rust equivalent
of `halo.set`. For interaction, use the existing shared click-action admission
path extended with a GlowPulse action, not a Python click callback that updates
uniforms. Keep the current filled-circle/rectangle target restrictions initially.
Repeated clicks while active do not queue or restart; conflicts suppress admission;
seek/reload retires the invocation; a settled invocation sleeps. These are M2
acceptance cases, not permission to broaden general interaction targets in M0.

### Callback-free interaction review pair (M2)

With the same filled-circle scene, Python registers
`self.on_click(dot, GlowPulse(dot, intensity=2.0, run_time=0.6))` before its
normal `self.wait(10)`. Rust declares the same pulse with
`scene.declare_glow_pulse(&dot, 2.0, AnimationOptions::new().run_time(0.6))?`,
then registers `scene.on_click(&dot, &pulse)?` before returning its normal live
program. These proposed `on_click` calls accept **animation intent**, not arbitrary
host callbacks; they lower to the existing shared native action/admission lane.
They are not a second event dispatcher. The binding itself advances no time.
An ordinary declarative wait supplies the same ten-second authored interval in
either language, and inspection may remain responsive while authored time is
paused. The existing explicit interaction clock drives only the admitted pulse.

Admission at wall/interaction times 0 and .2 with no conflicting authored channel
starts one .6-second pulse, not two. It captures at the first occurrence and
restores at .6; an occurrence at .7 may start the next one. A conflicting authored
intensity writer suppresses activation without queueing. Inspect those channel
and wake-state observations independently of authored frame count in M2.

## Activation, ownership and lifetime

An absent-to-present `.animate.set_glow` introduces its canonical binding only at
activation. The start intensity is zero; other initial fields come from the
validated target/defaults. At completion its target values persist. Removal is
structural: animate to zero, then remove explicitly. Movement and intensity are
disjoint; parallel writes to the same attachment parameter are rejected at normal
admission, not resolved by Python or render order.

Default GlowPulse interpolates captured intensity toward the peak with
`ThereAndBack(p) = smooth(1 - abs(2p - 1))`, using the existing normalized logistic
smooth with inflection 10. It owns intensity only unless other channels are
explicitly requested in a later supported API. Ordinary composition maps the
segment's logical time before its rate evaluation. A rate override replaces the
amplitude map; a non-returning map deliberately ends with the lifecycle restore,
which can be discontinuous. No override permits stale restoration.

Capture is at activation, not declaration. Restore only still-owned channels.
When a pulse introduced the missing binding, remove it on completion only while
its generation and temporary ownership remain valid and no persistent edit has
adopted it. A live edit adopts the attachment and supersedes the touched channels;
untouched pulse-owned channels may restore, but the attachment must survive.
Retirement never targets a replacement with the same name. At zero duration,
validate/admit/complete once with no visible peak or leaked temporary binding.

Copy creates independent attachment IDs/values, sharing immutable program inputs.
Detach/re-add preserves semantic identity and values but retires transient drivers.
Destroy/replacement retires attachment handles; resource retirement follows the
existing in-flight GPU lifetime mechanism. Python wrapper collection does not
detach an effect. Target copies preserve correspondence for parameter matching;
ordinary Transform supports matching attachment schemas and the canonical absent
glow case, not arbitrary stack/program changes. Reject incompatible topology
before playback. Save/restore includes attachment order and parameters; removed
bindings restored later receive fresh IDs. Persistent `become` preserves the
object ID but replaces its attachment set atomically with independent copies,
retiring the old attachment handles. Failed preparation leaves the old state and
renderable resources valid. Renderer recreation changes caches, not identities.

Composed groups are **one filtered painter result**. They are not equivalent to
filtering each child. Initially require contiguous painter membership with no
external interleaving; reject duplicate/aliased leaves within an isolated scope
or shared leaves across overlapping non-nested isolated scopes. Nested, disjoint/laminar
scopes compose inner-first. An alias elsewhere is not silently duplicated or
reordered. Validate the actual published painter interval, not just membership
at group construction. Changing order into an invalid isolation configuration
fails as one transaction. Neutral effects do not require isolation or alter pixels.
Per-child treatment must be explicitly authored on each selected child. Each
attachment consumes the preceding attachment's output in stable order. Its
painted mask uses that input alpha; a silhouette mask still refers to the declared
source geometry. Final scope opacity applies once after the entire stack.

## First visual model: `glow-encoded-ldr-v1`

This is an artistic tinted mask halo, **not emission or HDR bloom**. It preserves
the existing encoded-premultiplied compatibility composition; it performs no
linearization, tone mapping or output-transfer conversion. Later linear/HDR
profiles are separately named/qualified and cannot silently replace this model.

Let S be original encoded-premultiplied source RGBA before final scope opacity.
The default mask M is its painted alpha, including intrinsic fill/stroke/image
alpha. `source="silhouette"` instead uses explicitly requested geometric coverage
independent of paint alpha: the filled primitive interior plus any declared stroke
footprint for the M1 circle/rectangle subset. Zero source paint is therefore not
proof of zero contribution for silhouette mode. An offscreen mask whose halo
reaches the viewport is needed. Alpha-zero straight input colors must be sanitized
by normal source preparation; they cannot leak color into M.

A plain radius is Gaussian **sigma in world scene units**, not diameter, cutoff,
object-local scale, or CSS pixels. Geometry transforms change the mask; radius
is not additionally scaled by the object, including nonuniform scale. For the
supported uniform 2D view, sigma_px = radius * physical_output_height /
world_view_height. `Pixels(12)` means sigma_px=12 at every DPR/export resolution.
Camera zoom changes scene-unit sigma, not pixel sigma. Reject unsupported
nonuniform/perspective projection in the first slice rather than choose a scalar
approximation. M1 is the finite 2D filled-circle/rectangle subset, not all 3D.

At output pixel centers, let R = ceil(3*sigma_px). For integer dx,dy in [-R,R],

    K(dx,dy) = exp(-(dx^2+dy^2)/(2*sigma_px^2)) / Z
    Z = sum of those weights over the full square support
    B(x,y) = sum K(dx,dy) * M(x-dx,y-dy)

Outside the padded capture is transparent zero. Do not renormalize at image or
viewport edges. The reference quality uses full output resolution; no hidden
half-resolution blur. Square truncation is intentional: exact 90-degree/reflection
symmetries hold; arbitrary-angle rotational symmetry is not an exact tail identity.
A different kernel, analytic distance falloff, or multiresolution approximation
must meet this model and its declared errors or use a different named treatment.
Source coverage remains the qualified source renderer's sample/AA contract;
synthetic operator fixtures isolate that substrate instead of claiming to replace it.

For straight encoded tint C and intensity I:

    h = min(1, I * C.alpha * B)
    H = (C.rgb * h, h)
    T = S + (1-S.alpha) * H
    output = scope_opacity * T

The original source is on top. Scope opacity is applied **once after** treatment,
including for composed groups; child paint/opacity is already part of their source.
Saturated h is an explicit LDR alpha operation, not accidental early HDR clamping.
Intensity 0, radius 0, tint alpha 0, or absent/removed treatment uses the ordinary
no-effect path exactly. Radius-zero glow is identity even though zero-sigma blur
alone is the identity convolution. Scope opacity 0 proves no contribution to this
operator; neither source alpha 0 nor source bounds alone does so for silhouette.

Rendering bounds expand the source by R output pixels on each axis plus the source
AA footprint. Semantic layout, `next_to`, and default geometry picking do not expand.
Allocate/capture the needed source outside the viewport before clipping final
composition. The portable reference profile permits sigma_px <=64 and I<=8;
exceeding a view-dependent limit or actual texture/memory capability is an explicit
preparation error, not cropped output or a CPU fallback. A camera/resize transition
must validate its new effective view before effect publication; a failed transition
retains the last valid effect/view publication and surfaces the error.

### Concrete invalid specimens and expected failures

| Input / event | Stable diagnostic category; prior coherent state remains |
| --- | --- |
| `set_glow(radius=-1)` / NaN / `intensity=True` / unknown field | `InvalidEffectParameter` |
| Add a second attachment with `name="glow"` | `DuplicateEffectName` |
| Animate radius from `Pixels(12)` to .15 scene units | `DiscreteEffectTransition` |
| Two simultaneous animations own the same intensity | existing channel-conflict admission error, identifying attachment + parameter |
| Use removed handle after re-adding the same name | existing stale-generation error |
| Isolate `VGroup(a,c)` with painter order a,b,c | `UnsupportedEffectIsolation` |
| Isolate nested/aliased `VGroup(a,VGroup(a,b))` | `UnsupportedEffectIsolation` |
| Bind an input/resource that creates an effect dependency cycle | `EffectDependencyCycle` |
| Use a group/view/custom/HDR treatment on M1-only support | `UnsupportedEffectScope` or `UnsupportedEffectProfile`, not a skipped effect |
| New view exceeds sigma/texture/memory capability; invalid shader edit | capability/preparation error; prior resource/version remains active |

These names specify diagnostic categories to map through the existing Rust errors
and Python exception mapping, not a new error transport. Argument errors are
Python ValueError-class; admission/resource failures use existing runtime error
mapping with a structured category. M1+ must prove transaction rollback and exact
revision/identity preservation for these cases; the M0 oracle does not simulate it.

## Independent references and promotion rules

`vectors.json` contains literal hand-derived field coefficients and value tables.
For sigma=1/sqrt(2 ln 2), one-dimensional unnormalized taps are 2^(-k^2), so the
normalized numerators are [1,32,256,512,256,32,1]/1090. Their outer product is a
fully specified 7x7 impulse image; center is 262144/1188100 and total energy is 1.
`reference.py` evaluates the operator through a direct 2D sum with Python doubles,
not a production separable convolution helper. Smooth uses an independent tanh
form; fixed samples and a logistic equation cross-check it.

Run:

```sh
python3 -m unittest discover -s tests/visual-effects -p 'test_*.py' -v
node --test .github/ci/visual-effects-reference.test.mjs
bash scripts/check.sh fast
```

The reference intentionally caps fields at 65x65 and sigma at 16 to bound slow
CPU test cost; these are not shipping capability limits. Existing `.github/ci`
Node discovery invokes it without new dependencies or workflows. The local gate
also runs the same Python suite. Missing files, nonfinite values, wrong dimensions,
missing tests and out-of-budget pixels fail closed.

Current executable coverage: rational impulse, constant field, transparent border,
asymmetric mask, translation, finite support, radius-zero/subnormal limit, world
versus pixel units, exact alpha/source-over, neutral state, explicit transparent
silhouette, group/per-child and order witnesses, activation/pulse samples,
sample-order independence, and channel-scoped restoration value tables.
Seeded malformed kernels, offsets, clipping, ignored update, doubled DPR, alpha
errors, extra transfer, frame-count phase and stale restoration are rejected by
the same field comparator. These are **oracle/harness controls**, not yet injected
production mutations. GPU-global-rebuild, pipeline/allocator, real ownership,
recovery, and HDR controls belong in their promoted implementation tests.

For each production comparison, keep semantic IDs/order/events/endpoints exact.
Use absolute error <=1e-5 for normalized floating operator buffers and <=2/255 per
encoded RGBA8 channel over the **entire expanded effect ROI**, checking alpha
separately and compositing over black, white and saturated backgrounds. These are
initial fixed-reference-profile gates, not permission to loosen existing no-effect
ratchets. Neutral output/restoration on the same backend must match exactly.
Tail/support witnesses remain separate: a whole-frame average cannot hide a missing
halo. Validate approximations against all fixtures before adopting them.

Use the existing pinned source fonts/assets for text/path/source-raster promotion;
the mixed image is inline exact RGBA. Feeding a qualified no-effect source image
into this oracle proves filtering/composition only, not independent glyph coverage.
Retain fixture/source/program hashes, exact rational time, camera/backing size,
quality, backend/adapter and numeric versus perceptual results. An image produced
only by the implementation under test is not an independent reference.

## Qualification matrix and performance baseline

`qualification.json` freezes the corpus, views, targets, quality and repeated-pair
protocol. All device qualification flags are false. The inspection revision is a
baseline **identity**, not a newly measured performance baseline. Before recording
a cohort, freeze resolved dependencies, compiler/browser versions, full source and
package hashes, actual adapter identity, runtime view/backing dimensions and quality.
A redacted/unknown adapter cannot establish physical-device performance.

The initial portable raster profile targets native wgpu, WebGPU and WebGL2 with
renderable/sampleable RGBA8 UNORM resources and normal raster passes. Query actual
format/usage/size capabilities. No compute/storage texture, float-filtering,
timestamp-query, HDR or language-based capability assumption is required. The
inspected renderer manifest requests wgpu 30.0.1; record the resolved build lock,
not just that semver request. Profile support must be the same for Rust, native
Python and Pyodide when their actual device capabilities match.

Physical targets are the Intel MacBook Pro/Radeon Pro 5300M desktop and iPhone
14 Pro Safari; no connected phone is assumed. Software-GPU Linux CI supplies
correctness and controlled regression evidence only. Native Rust, direct WASM,
native Python and Pyodide, interactive and fixed-time offscreen, all remain
separately unqualified until real engine comparisons exist. Offline encoder work
#1896 is not a prerequisite for reference-time frame capture.

Reuse #1653, including its existing per-PR and cumulative-anchor checks and
unchanged ordinary Product Gate thresholds. For new enabled-effect workload pairs,
compare identical effects/quality against the first qualified enabled revision;
a no-effect image cannot be a visual baseline for an enabled effect. Until that
revision exists, report absolute enabled cost plus independent correctness without
inventing a speedup ratio. Ordinary scenes still compare against current master
and the established cumulative anchor.

Three serial B/C, C/B, B/C pairs; one completed cold source pass before each trial;
30 warmup and 540 measured frames; authored horizon >=10s; 60-Hz target at the
declared physical backing size. Report all three arithmetic means, dispersion,
raw timestamps and failures. Predeclare retries only for named infrastructure
invalidity; preserve invalid attempts and rerun the whole cohort, never the best
trial. No quality changes, source changes or profilers mid-cohort. The existing
literal physical FPS gates, where applicable, are not relaxed by the new targets.

The explicit enabled goals are p95 gaps <=20ms, p99 <=33.4ms, >25ms gap fraction
<=.02, input-to-present p95 <=50ms, extra scratch <=64MiB desktop/32MiB mobile.
Input latency needs real presentation evidence: CPU submit timing alone is not a
pass. Collect 60 scripted interactions per device, one per second after warmup,
with three complete repeats and raw input/presentation correspondence. GPU time is
null/unsupported without timestamp capability, never fabricated as zero. Cold
compilation and first-use latency are reported separately, not hidden in warmup.

Intensity-only edits must compile zero pipelines, rebuild zero source geometry,
dirty zero unrelated objects, and allocate zero new scratch targets after warming
fixed topology. Measure 1/100 active glows among 600 static objects; count passes,
upload ranges/bytes, CPU preparation/runtime, resident/peak/in-flight memory and
1000 attach/remove cycles. Radius/view/topology changes may legitimately resize
local targets; retain bounded reuse and dependency-local invalidation. No effect
must mean no effect-only passes, targets, global scan or time updates. Actual
measurements and failed attempts will be recorded on the issue/PR, not retroactively
filled into the false qualification flags here.

## Reuse inventory and narrowly missing prerequisites

This is an inspection handoff, not a proposed framework replacement.

| Existing owner / inspected hook | Reuse; finite missing work |
| --- | --- |
| `noon-core/src/lib.rs`, `semantic_store`, architecture identity contract | Same generational semantic identity and ordered ownership; add attachment role/owner and typed schema parameters, no new allocator |
| `noon-core/src/animation/timeline.rs` (`Property`, `ValueKind`, `RateFunction`) | Existing `Appearance` is a scalar, **not** an effect map. Add attachment/parameter channel addressing; preserve all current property meanings and shared time maps |
| `noon-core/src/animation/mod.rs` and its composition/family modules | Same declared composition, conflicts and family timing; add effect target capture/correspondence, not effect-specific playback |
| `noon/src/lib.rs`, `examples/shared_authoring.rs`, `live_program.rs` | Same Scene/live operations, target editor, `DeclaredAnimation`, `LiveContinuation`, completion and effective queries; add thin typed methods above |
| `web/python/examples/live_affine_completion.py` | Same effective continuation semantics and Rust-backed Python wrappers; add optional Noon-native effects exports, no Python state machine |
| Architecture publication / `noon-core/src/publication.rs` ownership | Prepare resources/validation before the existing coherent barrier, extend true dirty dependency closure; resource readiness must not partially publish an attachment |
| Existing `noon-compile` / `noon-runtime` owners | Lower typed attachment slots, capture at activation, own intensity leases, wake/seek and retire via normal runtime; add no shader clock |
| `noon-render-wgpu` retained preparation and presentation/inset ownership | Add minimal local mask/filter/composition work and parameter-only dirtiness. Inset capture is a mechanism, not already a general filter system |
| `noon-render-wgpu/src/cairo_color.wgsl` | Preserve encoded/quantized no-effect compatibility; do not globally substitute linear/HDR blend |
| `noon-render-wgpu/Cargo.toml`, existing native/web hosts | Same renderer on all targets, queried capabilities and device generations; no exporter/browser-specific effect implementation |
| `.github/workflows/pr-fast.yml`, `ci.yml`, `scripts/check.sh`, #1653 | Reuse existing discovery/local gates and fixed repeated Product protocol; no new benchmark framework |

**Sequencing clarification:** the frozen Gaussian model cannot be approximated by
an arbitrary analytic distance halo merely to avoid offscreen work. M1 may need a
minimal circle/rectangle mask plus blur/composite path. Generalizing that same
local path to arbitrary sources, nested groups and pooled pass dependencies stays
with M3. This is a small prerequisite pulled forward, not the full M3 pass planner
or a new renderer. The first implementation must report the concrete cost.

M0 does not add crates, runtime modules, shipping dependencies, API exports or
architecture changes. Full mixed Rust/Python executable parity, production error
injection, GPU/operator comparisons and physical qualification are explicit later
gates, not claims inferred from the independent reference tests.
