# Examples

On a phone or tablet, tap **Play scene** to run an example. You can keep two
examples open together. Tap **Close** to close a scene and discard its edits.
With a mouse or trackpad, the scenes start when the page loads.

Switch between **View** and **Code** within a live scene. Edits run after a typing
pause. **Code** shows syntax errors and runs the scene again when you fix them.

## Why loop order changes cache misses

Follow one read as it fetches four neighboring values into a one-slot cache.
Then see why scanning across the rows reuses those values while scanning down
the columns keeps replacing them in this model. Change `RIGHT_ORDER` to `"row"`
in **Code** to see both loops finish with four misses.

<!-- noon:demo showcase-cache-locality -->

## Why a doubling curve becomes a straight line

Follow a point as the same multiplier moves it through increasingly tall jumps.
Then change the vertical scale and see the same data form a straight line.
Try setting `GROWTH_FACTOR` to `1.5` in **Code**. Which plot stays straight?

<!-- noon:demo showcase-doubling-curve -->

## Rectangles become an integral

Start with four rectangles that take their heights from the curve at their
left edges. Watch the width halve, the live sum approach the area, and the
terms move into the integral. Set `B` to `3.5` in **Code** to shorten the
interval. Reproduced from
[the ManimGX gallery scene](https://manimgx.academa.ai/gallery/riemann-sums/).

<!-- noon:demo showcase-riemann-sums -->

## Why water speeds up through a narrow channel

Follow a dye pulse into a constriction. Rotate the channel to compare two
cross-sections, then measure how much water passes each in one second. The
model assumes steady flow and constant depth. Set `THROAT_WIDTH` to `1.5` and
compare how far the narrower slice travels in the same time.

<!-- noon:demo showcase-channel-flow -->

## Why a neon sign glows beyond its edges

Take one stroke from a neon sign, separate the sharp core from the diffuse
light, then see what happens when the halos from two strokes overlap. Try
setting `HALO_RADIUS` to `8` and watch the gap between the tubes.

<!-- noon:demo showcase-neon-glow -->

## Explore further

The [full gallery](../../playground.html?catalog=showcase) contains additional
feature and compatibility demonstrations, including
[Your first scene](../../playground.html?catalog=showcase&example=showcase-first-scene),
[Interference is addition](../../playground.html?catalog=showcase&example=showcase-wave-interference),
and [Make it move](../../playground.html?catalog=showcase&example=showcase-editorial-cover).
