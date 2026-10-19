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
