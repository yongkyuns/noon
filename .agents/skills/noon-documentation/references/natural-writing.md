# Natural technical writing for Noon

## Plan the smallest useful explanation

Write a one-sentence reader contract before drafting: “A reader who knows ___ can
___ by the end of this page.” Use it to remove prerequisites that belong elsewhere
and details that do not help the task. Usually keep this contract in working notes;
the page itself can open naturally with the outcome.

Choose the page's main purpose:

| Purpose | Reader's question | Useful shape |
| --- | --- | --- |
| Tutorial | Can I make a first working scene? | Guided sequence, complete example, visible result |
| How-to | How do I export this scene? | Relevant setup, commands, output, failure recovery |
| Reference | What arguments does this method accept? | Exact API, defaults, units, constraints, source link |
| Explanation | Why does authored time work this way? | Model, small example, consequence, design link |

Keep a continuous first-use path and a task map for returning readers. Explain
scene concepts before engine internals; link to the architecture when that depth
becomes useful. Preserve the existing browser-first introduction and first-class
Rust and Python paths unless the user's task changes that teaching sequence.

## Write like a helpful engineer

Name the actor and action: “The native host opens the window.” Use familiar verbs,
short concrete nouns, and ordinary transitions. Address the reader as “you” when
helpful; do not force every sentence into an imperative. Define a technical term
once, then use it consistently. Vary sentence length so explanations sound natural.

Cut promotional adjectives, throat-clearing, repeated conclusions, and phrases
that announce the writing: “In this comprehensive guide,” “as mentioned above,”
“it is important to note.” Do not call an operation “simple,” “obvious,” or “just”
when the reader may be learning it. Keep cautions specific and near the action.

These examples illustrate edits; verify the underlying behavior before reusing:

| Before | After | Why |
| --- | --- | --- |
| Leverage Noon's powerful capabilities to seamlessly bring your ideas to life. | Open the first animation, change its color, and see the result in the browser. | Gives the reader an action and visible outcome. |
| It is important to note that animations can be orchestrated concurrently. | Pass both animations to one `play` call to run them together. Use separate calls to run them in sequence. | Names the actual operation and its effect. |
| Simply run the export command to obtain your output. | Run the command from the repository root. MP4 export requires FFmpeg and a supported native GPU backend. | Makes relevant setup visible. |
| The example works everywhere. | The native runner and export host use the same scene constructor. Follow each host's setup instructions before running it. | Bounds the promise to the evidence. |

Do not turn every paragraph into a list or every useful sentence into a callout.
Use one idea per paragraph, with the implication following the fact. Preserve
meaning and necessary qualifications when tightening prose. Retain user-requested
capitalization, titles, and spellings; do not silently normalize their voice.

## Make examples runnable and teachable

Prefer real checked-in scenes and their source inclusion. The current walkthroughs
use `<!-- noon:source web/python/examples/first_lesson.py -->` and
`<!-- noon:source crates/noon/src/example_scenes/first_lesson.rs -->`. Check
`scripts/site/build_docs.py` before adding or changing such directives.

For each command or example:

1. Establish the working directory, dependencies, interpreter/toolchain, and host.
2. Give complete imports and setup, or explicitly label the block as an excerpt.
3. Show the exact command without a shell prompt that breaks copy/paste.
4. Describe the observable result: scene appearance, file path, duration, or output.
5. Explain the new concept immediately after the result, then one useful variation.

Keep terminal output separate from input and label representative output. Explain
placeholders and units; never silently substitute a guessed package/version/API.
For destructive commands, explain the specific effect before execution. Include
an actionable error recovery step when a known prerequisite commonly fails.

Use Rust/Python pairs to teach the same behavior, while documenting real differences
in authoring and execution. Check current native, WASM, Python facade, and export
support individually. Generated Python references and compiler-produced Rustdoc
own signatures; prose should explain choices and effects rather than copy catalogs.

## Preserve source and information ownership

`docs/architecture.md` owns architecture and roadmap decisions. `docs/type-map.md`
maps current types; `REFERENCE.md` and owning modules explain implementation details.
Link to the specific section that supports a claim. Do not duplicate their state
in a new evergreen authority or import unmerged proposal language.

For a changed title, preserve the route and useful incoming fragments or update
all affected references deliberately. Add pages through `scripts/site/navigation.json`
so search, navigation, and the reading sequence stay coherent. Link text should
name the destination or task, not say “click here.”

## Edit in three passes

First, read as a newcomer: can you reach the result using only the stated setup?
Second, read as a maintainer: are code, claims, sources, and support boundaries true?
Third, read the rendered page aloud: remove clumsy joins, repeated concepts, dense
paragraphs, unexplained abbreviations, and headings that say little. Stop when the
reader's task is clear and verified; do not polish away precision.

Writing references: [Google's developer-documentation tone](https://developers.google.com/style/tone)
and [Diátaxis page purposes](https://diataxis.fr/start-here/). Apply these ideas in
Noon's existing editorial voice; the repository and user instructions govern.
