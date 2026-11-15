# Teaching patterns

Choose a few references by the explanatory job. These are original analyses of
specific sources, not copied scene implementations or a separate support catalog.
Read the [educational brief](education.md) and query current Noon capabilities
before coding. External ManimGX/ManimGL source is not executable Noon evidence.

## Choose a pattern

| Explanatory job | Card | Noon starting point or current gap |
| --- | --- | --- |
| Relate a symbol to something visible | [Connect representations](#connect-representations) | Supported layout/text primitives; full algebra-to-area recipe is not provided here. |
| Track the same quantity through a change | [Preserve identity](#preserve-identity) | Existing coordinate-swap showcase and its recorded evidence. |
| Keep several views consistent | [Coordinate views](#coordinate-views) | Existing tracker/connector showcase; its callbacks are explicitly nonreplayable. |
| Explain an approach to a limiting value | [Inspect a limit](#inspect-a-limit) | Study only; a complete Noon secant-to-tangent lesson is not provided here. |
| Motivate a definition by comparing cases | [Compare before formalizing](#compare-before-formalizing) | Existing plotting source for mechanics; no complete variance lesson here. |
| Explain a system through one representative operation | [One operation to system](#one-operation-to-system) | Existing CUDA lesson, annotated in the educational guide. |

The local sources below are showcase examples, separate from the ready tutorial
records served by `noon_reference`. Read their current
[manifest](../../../web/python/examples/noon_showcase_manifest.json), source, and
evidence. A listed capability or past capture does not qualify a changed scene.
Use the [ready example table](examples.md) when selecting a `noon_reference` ID.

## Connect representations

**Use for:** viewers who know the notation but cannot explain what its terms mean.
Establish one concrete quantity, represent it in a diagram, and connect each
subsequent term to an explicit operation or piece of that diagram.

**Sequence:** identify terms and units; construct their visible counterparts;
change one relationship while preserving the others; show the corresponding
algebraic step; hold both representations for comparison; state where the diagram's
assumptions limit the general argument.

**Invariants and data:** use stable term meanings, labels, and colors. Compute
quantities from the same parameters. An area model with positive lengths must not
silently claim to represent arbitrary negative or complex quantities.

**Narration and checkpoints:** explain where a new term comes from while its
corresponding piece is visible. Inspect term movement interiors and the completed
equality. Ask the viewer to account for an added term without reading the formula.

**Study:** ManimGX `QuadraticFormula` and `Equation`,
[quadratic_formula.py](https://github.com/academa-labs/manimgx/blob/1fb4dd7327bf5ed7d0c40d022c05cb816455ae1d/examples/quadratic_formula.py).
The source uses named terms and consistent associations across equations and
geometry. License: MIT; study dependency: ManimGX and its text/math facilities.
This card includes no imported code or rendered Noon port.

**Noon use and limits:** query `Text`, `MathTex`, and the required geometry/layout
operations; use their current restrictions. Do not assume named-term matching or
every LaTeX expression works. A new algebra-to-area recipe still needs ordinary
Noon implementation and rendered checks. **Transfer task:** explain a weighted
average by relating weighted contributions to its numerator and denominator.

## Preserve identity

**Use for:** viewers who mistake a rearrangement or transformation for replacing
the original quantity. Establish the initial object and reference frame, apply
the change, then compare the result with a clearly marked earlier state.

**Sequence:** introduce a concrete example; name its components; transform those
same components; retain the correspondence; state the general rule. Keep labels,
component colors, and units stable. Make an intentional copy visibly different
from the original and inspect which object later statements modify.

**Narration and checkpoints:** identify the object before moving it. Pause at the
before/after states and inspect a transformation interior; the path can imply a
different operation from its endpoints. Ask which quantity stayed the same and
which changed.

**Study:** 3b1b `TrackBasisVectorsExample`,
[chapter3.py](https://github.com/3b1b/videos/blob/31f8ffe785e860477a6051fde983c02c7d165bc2/_2016/eola/chapter3.py).
It develops a concrete vector from a basis combination before expressing the
transformation algebraically. License: CC BY-NC-SA 4.0; dependencies include older
ManimGL conventions and custom scene classes. Study the argument, not its API.

**Noon example:** [Swap coordinates with a matrix](../../../web/python/examples/showcase_linear_algebra.py)
uses the computed swap matrix twice, with earlier directions retained as faded
vectors. The manifest declares deterministic retained replay and review targets
1.5, 4.5, and 8.0 seconds. Read its
[recorded capture evidence](../../../web/thumbnails/showcase/showcase-linear-algebra-evidence.json).
Query `LinearTransformationScene` and `ApplyMatrix`; their broad API classification
is partial. This narrow example is not a full basis-combination lesson. Do not
generalize its zero path arc to every transform. **Transfer task:** trace the same
record through a sorting pass while keeping its value distinct from its position.

## Coordinate views

**Use for:** viewers who need to connect a parameter, numerical value, diagram,
and graph. Derive the related displays from one model; change one input and show
its consequences together.

**Sequence:** establish the model and reference state; vary one parameter; hold a
representative state; test a meaningful boundary; compare the views' predictions.
Preserve units, scales, and object meaning. Distinguish computed values from
illustrative inputs. Avoid independently animated readouts that only look related.

**Narration and checkpoints:** state the changed input, direct attention to its
consequences, and inspect the boundary before generalizing. For a rotation, choose
a rotation-preserving path rather than interpolation that accidentally collapses
the space. Ask the viewer to predict a second view from the first.

**Study:** ManimGX `LinearMaps`,
[linear_maps.py](https://github.com/academa-labs/manimgx/blob/1fb4dd7327bf5ed7d0c40d022c05cb816455ae1d/examples/linear_maps.py).
One matrix coordinates the grid, basis, shape, entries, and determinant through
ordinary and singular cases. License: MIT; dependency: ManimGX tracker/update and
text facilities. No Noon port of that full sequence is included here.

**Noon example:** [Relationships that follow motion](../../../web/python/examples/showcase_reactive_relationships.py)
uses `ValueTracker` values to move two dots and keep a connector attached. Query
`ValueTracker`, `Dot`, and `Line`, then inspect the example's `add_updater` and
`match_points` use before adapting it. The manifest
explicitly declares **nonreplayable host callbacks**; inspect it through a fresh
Run rather than promising retained seek. Prefer an existing qualified Rust-owned
binding when it expresses the requested relationship. **Transfer task:** coordinate
a unit-circle point and its sine projection using a single angle model.

## Inspect a limit

**Use for:** viewers who can manipulate a limit expression but cannot explain the
finite observations approaching it. State a nonzero interval, show its observable
ratio, reduce the interval, then distinguish the limiting statement from dividing
by zero.

**Sequence:** place two labeled points; show rise, run, and secant; reduce the
interval with the base point held fixed; hold an inspectable small interval;
explain the limit; move to another base point only after establishing the rule.

**Invariants and data:** compute the finite quotient from the stated function;
preserve axis scales and the meaning of each point. Label a numerical approximation
as such. A finite-difference slope is not automatically an exact derivative.

**Narration and checkpoints:** allow time to inspect rise/run before changing both.
Review the smallest interval for disappearing labels, overlapping points, and a
misleading zero denominator. Ask why the argument does not substitute zero into
the quotient.

**Studies:** 3b1b `SecantLineToTangentLine`,
[chapter2.py](https://github.com/3b1b/videos/blob/31f8ffe785e860477a6051fde983c02c7d165bc2/_2017/eoc/chapter2.py)
(CC BY-NC-SA 4.0, legacy ManimGL/custom graph classes), and ManimGX `DerivativeSlope`,
[derivative.py](https://github.com/academa-labs/manimgx/blob/1fb4dd7327bf5ed7d0c40d022c05cb816455ae1d/examples/derivative.py)
(MIT, ManimGX redraw/tracker facilities). The latter uses numerical differences
and mutable trace state; it is not evidence of Noon deterministic seeking.

**Noon status:** complete lesson not implemented by this card. Query `Axes` and
the exact line/point/label operations; design repeatable sampled states or use a
qualified dependency pattern. Do not silently remove callbacks to make a borrowed
recipe appear supported. **Transfer task:** show average speed over a shrinking
time interval while preserving distance and time units.

## Compare before formalizing

**Use for:** viewers who know a formula but do not know why that statistic is
needed. Construct two cases whose shared summary hides an important difference,
make the difference visible, then introduce the new quantity.

**Sequence:** show comparable distributions; establish their common mean; vary
spread with fixed scales; connect distances to the definition; compare the result;
explain the units and why a further operation such as a square root is useful.

**Invariants and data:** compute summaries from the displayed values and weights.
Keep normalization, axes, sample/population convention, and units explicit. Equal
means do not imply equal spreads. Identify chosen illustrative distributions rather
than suggesting observed data.

**Narration and checkpoints:** ask what the shared mean fails to describe before
introducing variance. Hold both cases while highlighting distances and inspect
their correspondence to formula terms. Ask the viewer to predict which case has
larger spread and explain the reason.

**Study:** 3b1b `MeanAndStandardDeviation`,
[main.py](https://github.com/3b1b/videos/blob/31f8ffe785e860477a6051fde983c02c7d165bc2/_2023/clt/main.py).
License: CC BY-NC-SA 4.0; dependencies: ManimGL and custom distribution helpers.
Study its comparisons; independently verify the mathematics for the new lesson.

**Noon starting point:** [Functions and samples](../../../web/python/examples/showcase_coordinate_plotting.py)
provides ordinary `Axes`, `plot`, and `plot_samples` usage and labels its chosen
samples illustrative. Query the current plotting restrictions. It is not a
variance implementation or a measured-data example. **Transfer task:** compare two
latency distributions with the same mean and different tail behavior; state how
the chosen summary relates to the question before adding another metric.

## One operation to system

**Use for:** viewers overwhelmed by a compact equation or a large network. Follow
one representative item through a complete operation before expanding the number
of items. Retain the original item as a reference when the system becomes dense.

**Sequence:** locate one input; show its rule and result; repeat enough cases to
expose the relationship; introduce the shared constraint or interaction; change one
relevant condition; compare outcomes; state the general rule and its assumptions.

**Invariants and data:** preserve item identity, direction, and input/output
meaning. Compute values when numerical correctness is part of the explanation.
Show whether concurrent motion means actual concurrency or a schematic comparison.

**Narration and checkpoints:** explain the representative calculation before
expanding it. Inspect an interior transfer as well as its destination; check that
labels remain attached and the shared constraint stays visible. Ask the viewer to
predict one unseen case from the same rule.

**Study:** 3b1b `AttentionPatterns` and `DescribeAttentionEquation`,
[attention.py](https://github.com/3b1b/videos/blob/31f8ffe785e860477a6051fde983c02c7d165bc2/_2024/transformers/attention.py).
License: CC BY-NC-SA 4.0; dependencies: ManimGL, custom classes, and author assets.
Some numerical scores and dot sizes are random or chosen for illustration; they
are not measured model attention. Do not reuse them as experimental results.

**Noon example:** follow the [One Extra Column walkthrough](education.md#worked-example-one-extra-column)
and its ordinary maintained source. Query `Text`, `Rectangle`, `Line`, `FadeIn`,
`FadeOut`, and `Write`, then read the scene-specific helper and replay restrictions.
Its computed address model and schematic service timing have separate evidence.
**Transfer task:** explain a dependency graph by tracing one item before showing
multiple items waiting for a shared prerequisite.

## Reuse boundaries

The ManimGX source and skill are MIT licensed; preserve notices for copied or
substantially adapted material. These cards do not bundle its runtime, native
wheels, fonts, or assets. Its wheel distribution has separate dependencies and
license obligations.

The 3b1b video-source corpus is CC BY-NC-SA 4.0, distinct from the engine's license.
Use original Noon scenes and concise analyses with pinned source references. A
direct port, copied exposition, character, or asset needs its own reuse decision;
attribution alone does not remove the noncommercial and ShareAlike terms. Neither
repository's wait durations establish a verified narration track. Keep studied
source, implemented Noon behavior, rendered evidence, and learner results separate.
