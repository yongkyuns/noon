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

## Ports of Manim gallery scenes

- [A Galton board](../../playground.html?catalog=showcase&example=showcase-galton-board) reproduces Academa’s seeded 480-bead simulation and the normal curve over the final piles. [ManimGX source](https://manimgx.academa.ai/gallery/galton-board/)
- [Boolean operations on two ellipses](../../playground.html?catalog=showcase&example=showcase-vector-cutout) shows all four operations in the original nine-second sequence. [Manim Community source](https://docs.manim.community/en/stable/examples.html#booleanoperations)
- [Circles draw π](../../playground.html?catalog=showcase&example=showcase-fourier-epicycles) uses the original 101 terms, π outline, and pen-follow zoom. [ManimGX source](https://manimgx.academa.ai/gallery/fourier-pi/) · [3Blue1Brown Fourier lesson](https://www.3blue1brown.com/lessons/fourier-series)
- [Miura-ori](../../playground.html?catalog=showcase&example=showcase-miura-ori) uses the original 168-panel sheet, camera orbit, and dimension readouts. [ManimGX source](https://manimgx.academa.ai/gallery/miura-ori/) · [Schenk and Guest’s fold geometry](https://doi.org/10.1073/pnas.1217998110)
- [Pythagoras by rearrangement](../../playground.html?catalog=showcase&example=showcase-pythagoras) preserves the original colored sides, quarter turns, and three slides. [ManimGX source](https://manimgx.academa.ai/gallery/pythagoras/)

## Ports of JavaScript animation scenes

- [Circle area from one radius](../../playground.html?catalog=showcase&example=showcase-circle-area-signals) follows a changing radius through the circle, dimension arrows, and area readout. [Motion Canvas Signals lesson](https://motioncanvas.io/docs/signals/#complex-example) · [Pinned source](https://github.com/motion-canvas/motion-canvas/blob/7b91435c301d530351dcf5ebb91dd139c002e405/packages/examples/src/scenes/node-signal.tsx)
- [The world’s most valuable brands](../../playground.html?catalog=showcase&example=showcase-brand-value-race) follows the top 12 global brands from 2000 to 2019, with moving ranks, live values in millions of US dollars, and a rescaling axis. [Mike Bostock’s D3 notebook, revision 3087](https://observablehq.com/d/3ff9fa2c6593d814@3087)
- [Drip Fill Gauge](../../playground.html?catalog=showcase&example=showcase-drip-fill-gauge) ties a pixel-art carafe, bitmap percentage, and gauge bar to one progress value. [Pinned RenderComp source](https://github.com/RenderComp/free-remotion-templates/blob/f648980e528defb7594e24c6447055f0540f340b/src/components/pourover-drip-fill-gauge/PouroverDripFillGauge.tsx)
