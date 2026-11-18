# Designing a lecture or tutorial

## Storyboard the explanation

Write one sentence a viewer should be able to say or one task they should be able
to perform afterward. Infer the audience, prerequisites, and viewing conditions
from the request and its destination. A silent embed on a phone needs enough
context in the canvas to stand on its own. For a narrated lesson, map the
spoken script to visual beats and reserve the canvas for the objects, symbols,
and relationships the viewer needs to follow.

For each beat, write the viewer's current question, what they see change, and
what they can infer afterward. Use that sequence to choose scene sections and
what to sample in preview. A causal explanation often develops like this.

1. **Pose a concrete puzzle.** Show the input, intended outcome, or two similar
   cases with a surprising difference. Give the viewer time to form a guess.
2. **Make the mechanism visible.** Start with one small, fully worked case.
   Introduce names when their referents are visible. Show the state before an
   action, the action itself, and the resulting state. Expose hidden state when
   the explanation depends on it.
3. **Change one condition.** Keep the same input and visual encoding in a
   comparison. Give viewers time to predict what the changed condition does.
   Walk through the point where outcomes first diverge at a readable pace.
4. **Resolve the opening question.** Tie the result to the visible mechanism.
   State any model assumption that the conclusion depends on. In a tutorial,
   give the viewer a small change to make and a result to check.

For example, two loops visit all 16 entries of a matrix. Where do their memory
requests differ? Move the same tiles into address order and keep their line
colors. Follow a cache miss as four tiles load into a single slot. Show a read
that hits the loaded line, then a read that replaces it. Give each loop its own
slot and compare the first few reads slowly. The final totals follow from which
line each loop needs next. The aligned layout and one-slot cache explain the
counts for this example.

## Carry meaning across beats

- Keep positions, labels, and colors stable for objects the audience must
  recognize. When changing representations, move the same objects or show a
  copy traveling so the viewer can follow the correspondence.
- Reveal consequences when their causes occur. In a trace, let the selection
  arrive before changing the state or counter. Show related changes together.
  Give a new concept time to register before introducing another one.
- Use labels, contrast, and spatial grouping to direct attention. Keep captions
  for questions, rules, and inferences that the picture cannot convey. Use a
  second cue such as a label or outline where color carries meaning.
- Hold on a prediction, a new representation, or a result long enough to read.
  Speed through repeated steps after the rule is clear. Leave the takeaway
  visible long enough to connect it to the opening question.

## Verify the teaching sequence

Use [Noon's preview workflow](../SKILL.md#qualified-local-preview-route) to
sample just before, during, and after the important changes. For each sampled
beat, check what the viewer would know at that point. Watch the full sequence
at playback speed to see whether the change and its explanation land together.
In an embed, also check the smallest intended viewport with its controls
visible and audio muted. Try the suggested edit if the lesson offers one.

A useful final check is to work through an unseen case using only the visible
rule. In the cache example, pause after reading address 0 and predict what will
happen on a read of address 2 or address 8. Then verify the displayed comparison
and totals follow the same rule.

## Further examples

- The linear transformations chapter moves from one vector to a grid and uses
  motion to expose what stays fixed. [3Blue1Brown scene source](https://github.com/3b1b/videos/blob/master/_2016/eola/chapter3.py)
- The derivatives chapter aligns two snapshots before building the ratio that
  compares them. [3Blue1Brown scene source](https://github.com/3b1b/videos/blob/master/_2017/eoc/chapter2.py)
- The ManimGL skills show how to outline a lesson, inspect selected beats, and
  iterate at a checkpoint. Apply these habits using Noon's preview controls.
  [ManimGL Video](https://github.com/do-gongil/manimgl-skill/blob/main/skills/manimgl-video/SKILL.md),
  [ManimGL Interactive Development](https://github.com/adithya-s-k/manim_skill/blob/main/skills/manimgl-best-practices/rules/interactive.md)
