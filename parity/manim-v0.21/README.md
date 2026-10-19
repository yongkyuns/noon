# ManimCE v0.21 parity corpus

This directory contains the canonical source used by the Manim raster differential lane.

The `.py` source is written for real ManimCE. The Noon runner is allowed to change only the import from `manim` to `noon` and append the small scene-selection wrapper required by the browser authoring worker. Scene geometry, style, timing, waits, ordering, and animation calls must remain unchanged.

The initial scenes correspond to the Manim Community v0.21.0 quickstart tutorial. Manim Community is MIT licensed; upstream source and documentation are at https://docs.manim.community/en/v0.21.0/.

Do not use this directory for pedagogical Noon-specific adaptations. Those belong in the demo/tutorial corpus. A parity fixture is a compatibility oracle, not a showcase scene.

The full pinned directive inventory is extracted by `scripts/manim-reference-inventory.mjs` and locked under `compat/`. Public tutorial/reference enrollment remains in `web/python/examples/manim_tutorial_manifest.json`; the remaining directives have internal provenance/ownership records in `compat/manim-v0.21.0-reference-dispositions.json`. The coverage join rejects duplicate or stale identities and requires every directive to be represented. Pending `unqualified`, `blocked` and `deferred` records are not executable or parity-qualified claims. New exports require stale namespace blockers to be reconciled without automatically promoting behavior. Existing exact-camera qualification is retained separately in recorded case evidence until its canonical enrollment is reconciled.

`paint-fade` covers ordinary leaf fades and recursive nested-family fades with
shared descendants. Manim's recursive fade visits a leaf once per membership
path; shared Rust counts those paths and publishes each affected leaf once.
The paired native/direct-WASM/Python scene is `family_paint`, qualified by
`scripts/paint-fade-qualification.mjs`. Callback fades use the same Rust paint
operation over the current ordered overlay. The bounded profile accepts darkness
in `[0, 1]`; background strokes, mixed image families and non-recursive
family-container paint remain unqualified. Image fade resets original pixel
alpha in Manim and is explicitly rejected rather than inheriting vector alpha
multiplication. Unrepresentable recursive occurrence counts fail before mutation.

`Mobject.fade_to(color, alpha)` now interpolates the shared current RGB paint
toward the target and routes the result through the common color edit, preserving
fill and stroke opacity. Direct, live, recursive-family, and staged callback
paths share that implementation; repeated family aliases blend once per membership
path using bounded composition and one publication per leaf. Point-free geometry
and image objects remain unchanged. Focused
Rust and Python adapter tests cover RGB interpolation, opacity preservation,
aliases, and publication routing. The pinned-source `paint-fade` fixture blends
the current vector fill/stroke color after prior paint edits, as Manim's
`VMobject.get_color` does, and holds the result in a visible interval.
Its exact-head native/direct-WASM/Python raster comparison and ManimCE oracle are
pending. Native Text hierarchy differences, resource paint color queries and
broader object types remain unqualified.

`triangle-options` covers omitted defaults, radius/start-angle overrides and
inherited regular-polygon vertex queries. The Rust Triangle constructor delegates
to the same closed regular-polygon path as the Python wrapper; the separate fixture
preserves the polygon compound-fill/stroke regression unchanged. Wider inherited
methods/options remain owned by #2012.

Angle nominal-value overrides are semantic metadata: assigning `angle_value`
updates the Rust-owned value without rebuilding the authored arc/elbow path, and
live assignments use the normal publication transaction. Focused native/live and
Python adapter tests cover this behavior, and the paired constructor scene checks
the same assignments without changing its paths. Pending callback phases reject
nominal-value writes through the existing publication barrier; callback-local
Angle metadata overlays are not supported. This non-visual operation does not qualify
the still-open dot-child, non-planar, or wider constructor behavior in #2012.

Detached updater suspension now uses per-object flags in shared Rust node state.
The `updating_suspended` property writes only the root; `suspend_updating()` uses
Rust's unique family traversal, and `resume_updating()` preserves ManimCE's nested
immediate `update(dt=0)` callback order. The state survives object copies and
family copies/targets. A product Pyodide comparison against pinned ManimCE passes
153/153 semantic fixtures, including nested suspension and resume. Scene-owned
suspension remains unsupported until the canonical callback phase also suppresses
Rust-selected callbacks and animation effects; the API rejects before mutation.

Group/VGroup `add_to_back()` prepends unique direct members in first-argument order,
moves existing members to the front, and publishes the order change through one
shared Rust transaction in detached and live contexts. Product Pyodide fixtures
match the pinned ManimCE 0.21.0 order in both contexts. The canonical overlapping-
square family-order fixture passes the unchanged raster tolerance on WebGPU and
WebGL2: no pixels cross the difference threshold, bounds and duration match, and
mean absolute channel error is 0.001725 with maximum channel error 9. Generic
Mobject-owned child membership remains unqualified because Noon currently models
child collections as family nodes.

`Mobject.insert(index, mobject)` is supported for a new direct Group/VGroup member.
Its Python list index conversion, negative-index adjustment, and clamping are
preserved before a single Rust family-order transaction in detached and live
contexts. Repeating an identity already present in the same family remains
unsupported: Manim stores repeated list entries, while Noon semantic families
currently store one direct edge per identity. The adapter raises
`NotImplementedError` before mutation for that case. Pinned-source differential
fixtures cover detached and live insertion order and receiver identity. The product
Pyodide runner exercised both paths as part of 207 shared Rust/WASM/Python
observations; exact comparison with pinned ManimCE remains in hosted checks because
the reference package is unavailable locally.

`Mobject.invert(recursive=False)` now reverses the direct member order through one
shared Rust transaction. Recursive inversion propagates root-to-family path parity,
so a nested family shared by two parents is inverted twice and returns to its
original order, matching Manim's recursive calls without expanding shared paths.
Detached and live product fixtures cover this alias case; the overlapping-square
Manim Cairo raster fixture passes on both backends with no pixels crossing the
difference threshold, matching bounds and duration, and 0.001789 mean absolute
channel error (maximum channel error 9).

`Mobject.sort()` and `sort_submobjects()` apply Python key callbacks to the current
member wrappers and submit the resulting stable order through the shared Rust
family-reorder transaction. The paired product fixtures cover the default
x-coordinate key, explicit member keys, stable ties, detached/live calls, and
receiver identity. The overlapping-square Cairo raster fixture qualifies the
point-key painter-order change after live scene admission on WebGPU and WebGL2
with zero differing pixels, matching bounds and duration, 0.001725 mean
absolute channel error, and maximum channel error 9.

`Mobject.shuffle(recursive=False)` and `shuffle_submobjects()` use Python's
seeded `random.shuffle` sequence to choose permutations, then publish each family
order through the shared Rust transaction. Product fixtures cover detached/live
calls and receiver identity. `recursive=True` is explicitly unsupported until
Rust owns the repeated-alias family traversal plan. The seeded overlapping-square
Cairo fixture shuffles after live scene admission and passes on WebGPU and WebGL2
with zero differing pixels, matching bounds and duration, mean absolute channel
error 0.001725, and maximum channel error 9.

`Mobject.reverse_points()` and nested `Group.reverse_points()` reverse the
stored point sequence through Rust-owned leaf selection and one atomic path
resource transaction, preserving member transforms and family order. Direct and
live product comparisons match pinned ManimCE for the selected VMobject paths
(included in 173/173 semantic fixtures). The nested-family raster fixture passes
on WebGPU and WebGL2 with unchanged duration and no manifest-policy failures
(0.2008% differing pixels, mean absolute channel error 0.022041, maximum channel
error 67). Text/image point buffers remain unqualified because their resources
do not expose shared vector-point arrays. Pinned source:
https://github.com/ManimCommunity/manim/blob/v0.21.0/manim/mobject/mobject.py#L1549.

`Mobject.repeat(count)` repeats each selected vector leaf's raw point array in
one atomic Rust family transaction, preserving transforms and order. Direct and
live product probes match pinned ManimCE for nested VMobject families, including
point values, counts, and receiver identity (175/175 semantic fixtures). Zero
and negative counts preserve the inherited point-free no-op and point-bearing
`reduce()` error; Text/Image point arrays remain unqualified. The nested-family
raster fixture passes on WebGPU and WebGL2: duration delta 0, 0.2008% differing
pixels, MAE 0.022041, maximum channel error 67. Pinned source:
https://github.com/ManimCommunity/manim/blob/v0.21.0/manim/mobject/mobject.py#L1554.

`arrange_submobjects()` forwards its positional and keyword arguments to the
dynamic `arrange()` implementation, reusing the shared Rust family layout path.
Product comparisons cover detached and live Group calls and preserve receiver
identity.

`Mobject.space_out_submobjects(factor=1.5, **kwargs)` expands the family then
scales each direct child by the reciprocal factor through existing shared Rust
transforms. Detached and live nested-Group fixtures match pinned ManimCE in the
177/177 semantic suite. The Cairo raster fixture passes on WebGPU and WebGL2
with zero duration delta, 0.0739% differing pixels, MAE 0.009215, and maximum
channel error 42. Pinned source:
https://github.com/ManimCommunity/manim/blob/v0.21.0/manim/mobject/mobject.py#L1898.

The planar frame helpers `align_on_border()`, `shift_onto_screen()`, and
`is_off_screen()` use Rust-backed family bounds and placement with Noon’s default
16:9 frame dimensions. Direct and live probes match pinned ManimCE in the
179/179 semantic suite. Custom Manim frame-config overrides and staged callback
queries remain unqualified. The nested-family shift fixture passes on WebGPU and
WebGL2 with duration delta 0, 0.0627% differing pixels, MAE 0.008777, and
maximum channel error 51. Pinned source:
https://github.com/ManimCommunity/manim/blob/v0.21.0/manim/mobject/mobject.py#L1604-L1752.

`Mobject.apply_function_to_position(function)` maps the object's center, then
moves it through the shared Rust placement path; the submobject variant applies
that call to each direct child in order. Direct and live nested-family fixtures
match pinned ManimCE in the 181/181 suite. NumPy receives a float64 Point3D when
available; without NumPy, Noon passes an equivalent zero-Z tuple. The Cairo
raster fixture passes on WebGPU and WebGL2 with duration delta 0, 0.0735%
differing pixels, MAE 0.011022, and max channel error 47. Pinned source:
https://github.com/ManimCommunity/manim/blob/v0.21.0/manim/mobject/mobject.py#L1480.

`Mobject.put_start_and_end_on(start, end)` maps planar point-owning paths by
scaling about the original start, rotating through the signed planar angle, and
shifting to the requested start, with each geometry edit delegated to shared
Rust operations. Detached and scene-bound product probes match pinned ManimCE
for transformed lines and an exact-zero VMobject, including receiver identity
and Manim's warning-and-shift fallback (183/183 semantic fixtures). Nonzero-z
placement remains unsupported. The Cairo raster fixture passes on WebGPU and
WebGL2 with duration delta 0, 0.0766% differing pixels, MAE 0.010016, and
maximum channel error 44. Pinned source:
https://github.com/ManimCommunity/manim/blob/v0.21.0/manim/mobject/mobject.py#L1948.

`Mobject.reduce_across_dimension(reduce_func, dim)` invokes the supplied reducer
on each point-owning member's transformed point array from a recursive tree
produced by shared Rust semantics, then combines descendant results in pinned
order, including repeated family aliases. Python owns only the caller callback
and combines its results in the Rust-provided order without resolving Python
family membership. Empty families return `None`; planar z reductions use zero
coordinates. Detached and live nested-family probes match pinned ManimCE for
min/max, custom reducer inputs, alias multiplicity, and empty families (185/185
semantic fixtures). Staged callback queries reject before reading a stale
snapshot, and resource-backed Text/Image point arrays remain unqualified. Pinned
source:
https://github.com/ManimCommunity/manim/blob/v0.21.0/manim/mobject/mobject.py#L2168.

`Mobject.get_array_attrs()` reports `points`, and
`get_merged_array("points")` returns the Rust-owned recursive point array in
depth-first pre-order as a planar Nx2 array, retaining repeated aliases in
detached and live scenes. The paired product/Pinned-ManimCE suite passes 187/187
fixtures, including both direct and live nested-alias cases.
Other arbitrary array attributes remain unsupported until they have a shared
Rust-backed representation. Staged callback-phase queries reject before reading
a stale snapshot. Pinned sources:
https://github.com/ManimCommunity/manim/blob/v0.21.0/manim/mobject/mobject.py#L869 and
https://github.com/ManimCommunity/manim/blob/v0.21.0/manim/mobject/mobject.py#L2220-L2229.

`Mobject.next_to()` now has paired active-live observations for both a Mobject
target and a point target. The Mobject case checks family bounds, aligned-edge
placement, the coordinate mask, and receiver identity after both objects are
admitted to a live scene; the point case checks point placement, its mask, and
identity. Product Pyodide passes all 246 shared Rust/Python fixtures. The pinned
ManimCE comparison is enrolled in the hosted differential because ManimCE is not
installed in this checkout. Pinned source:
https://github.com/ManimCommunity/manim/blob/v0.21.0/manim/mobject/mobject.py#L1558-L1607.

`Mobject.surround()` now has an active-live differential fixture for its
replace-then-scale layout composition. It checks the returned receiver, target
center, matched dimension, and buffer-expanded size against the detached product
profile. Product Pyodide qualification is included in the current shared fixture
run. The pinned ManimCE comparison remains enrolled in hosted qualification.
Pinned source:
https://github.com/ManimCommunity/manim/blob/v0.21.0/manim/mobject/mobject.py#L1793-L1803.

`Mobject.replace()` now has active-live fixtures for both proportional matching
of the default width dimension and `stretch=True`. They check receiver identity,
target-center alignment, and the expected aspect-ratio behavior. The paired
ManimCE probes are included in the hosted differential. Pinned source:
https://github.com/ManimCommunity/manim/blob/v0.21.0/manim/mobject/mobject.py#L1777-L1792.

`VMobject(shade_in_3d=False)` and the equivalent `VGroup` constructor profile
are supported without adding Python-side shading state. `set_shade_in_3d(False)`
preserves receiver identity in detached and active-live product fixtures. The
actual Pyodide probe confirms that enabling 3D shading or `z_index_as_group`
rejects before mutation; these branches need shared spatial-renderer semantics.
The pinned ManimCE comparator is enrolled for hosted qualification because the
reference package is unavailable in this checkout. Pinned source:
https://github.com/ManimCommunity/manim/blob/v0.21.0/manim/mobject/types/vectorized_mobject.py#L119-L149 and #L735-L742.

`VMobject.generate_rgbas_array(color, opacity)` now builds Manim-compatible RGBA
rows from scalar or iterable colors and opacities using the pinned proportional
`make_even` pairing. The paired product fixture covers default black for `None`,
named and hex colors, numeric RGB(A) tuples, custom `get_hex()` values, repeated
single colors/opacities, and float64 array shape/dtype. Noon returns list rows
when NumPy is absent. This pure color conversion does not mirror object or scene
state. The product Pyodide differential now passes 256 shared fixtures; exact
ManimCE comparison is enrolled for hosted qualification. Pinned source:
https://github.com/ManimCommunity/manim/blob/v0.21.0/manim/mobject/types/vectorized_mobject.py#L210-L240 and
https://github.com/ManimCommunity/manim/blob/v0.21.0/manim/utils/color/core.py#L148-L208.

`VMobject.init_colors(propagate_colors=True)` now reapplies the family root's
fill, foreground stroke, and background stroke through shared Rust family paint operations. With
`propagate_colors=False`, each member keeps its existing Rust-owned style. The
detached and active-live product fixtures check root/child paint and receiver
identity; nonzero sheen and background images reject before mutation. Exact ManimCE comparison is included
in hosted qualification because the pinned package is unavailable locally.
Pinned source:
https://github.com/ManimCommunity/manim/blob/v0.21.0/manim/mobject/types/vectorized_mobject.py#L182-L209.

`VMobject.update_rgbas_array()` now routes scalar solid fill, foreground stroke,
and background stroke color
or opacity updates through root-local Rust paint setters. Color alpha remains
independent from the explicit opacity argument. Detached/live probes cover
color-only, opacity-only, no-op, alpha-bearing hex input, VGroup root-only
changes, and child-style preservation. Multi-color arrays, arbitrary
Python-owned RGBA arrays, unknown channels, and nonzero sheen remain explicitly
unsupported. Pinned source:
https://github.com/ManimCommunity/manim/blob/v0.21.0/manim/mobject/types/vectorized_mobject.py#L241-L265.


Solid background strokes now have shared Rust color, opacity, and width fields
and render beneath fill and foreground strokes. `set_stroke(background=True)`,
`set_background_stroke()`, and atomic `set_style()` apply to leaves and recursive
family roots/members; `family=False` on a VGroup updates only its point-free root.
Detached/live product probes exercise nested root and leaf paint, scalar background
RGBA updates, color propagation, and leaf opacity/fade. Native tests protect
rollback, primitive-to-path rendering, width-cache invalidation, GPU attribute
layout, and raster layer order. Staged callback background reads/writes,
nonzero sheen, multi-color per-object arrays, and background images remain
unsupported. Pinned ManimCE raster parity for this channel is still unqualified.

The shared runtime generic-transform regression also checks intermediate
background RGBA and width, exact endpoints, and equality of direct seek and
forward playback for point-owning paths. This is native runtime evidence;
pinned ManimCE timing and raster equivalence remains unqualified.

Curve-function generators retain shared Rust query ownership for every callable
already returned, including after a caller injects a generator exception or a
later curve-length query fails. Regression tests verify final-owner cleanup;
actual WASM/Pyodide probes cover escaped callables from both generator variants.
The pinned two-sided comparison for these new error-lifetime probes remains
pending. This does not qualify source-edit tracking or broader curve contracts.

Recursive `VGroup.set_fill()`, `set_stroke()`, and `set_opacity()` now update
nested non-rendered root paint as well as unique leaves through the shared Rust
transaction, including live publication. Opacity changes all enabled leaf paint
channels and each root's observable background alpha, preserving object opacity
and unrelated objects. Native regressions cover aliases, one revision per edit,
disabled leaf paint, and invalid-update rollback; the executable family-paint
pair asserts the root observations. All 258 release-WASM/Pyodide semantic
fixtures pass, including the detached/live setter profiles. The family-paint
pair produces identical nonblank pixels on WebGPU and WebGL2 (1,970 visible
pixels each); the rendered result was inspected. The local fast gate passes
3,449 library tests / 17 configured ignores; 93 family/paint tests and the native
example also pass. Pinned ManimCE qualification, recursive root fade, and staged
callback root updates remain open.
