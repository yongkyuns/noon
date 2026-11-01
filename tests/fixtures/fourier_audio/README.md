# Fourier and audio acceptance baselines

These are the complete original sources investigated in #2358. The recorded
baselines below belong to their original deployment. Current ordinary Python
audio is qualified separately by `scripts/prepared-audio-qualification.mjs`.

| File | Execution profile | Recorded baseline |
| --- | --- | --- |
| `fourier_epicycles.py` | Ordinary `Scene.construct` / `play` / `wait`; prepared native motion | Three 7/21/61-term butterfly drawings, 34.65 seconds, no Python frame updater. |
| `music_preview.py` | Finite `declare_value` plus immutable audio | Static butterfly and a moving marker with the same soundtrack window. |
| `audio_ordinary_play.py` | Audio followed by ordinary `play` / `wait` | Minimal unsupported audio/history combination; must not silently discard audio or bypass history admission. |

`provenance.json` records the exact preserved source bytes and the original
runtime revision. The observations in #2358 belong to that revision, not to each
new checkout. The source comments are preserved historical context. The local
`.gitattributes` keeps fixture line endings stable across checkouts.

Copy one entire source into the playground and run it. The finite music preview
starts paused; use Play to start it. The music is Noon's built-in original CC0
score, so no external media asset or codec is needed. The Fourier fixture uses
NumPy during preparation. Gallery publication remains a separate decision from
this integration qualification.

## Focused checks

From the repository root:

```sh
python3 -m unittest discover -s web/python -p 'test_fourier_audio_fixtures.py' -v
cargo test -p noon --features prepared-prefix --test prepared_audio_source
cargo test -p noon-web --lib canonical_authoring_scene::prepared::tests::audio
node scripts/prepared-audio-qualification.mjs
```

The Python checks verify fixture identity/syntax, choreography durations,
absence of explicit frame callbacks, and numerical preparation using the actual
pure functions extracted from the Fourier fixture. Numerical checks require
NumPy and report explicit skips when it is unavailable. They do not substitute a
fake Noon runtime and do not establish renderer, worker, audio-output, or seeking
correctness.

The Rust checks exercise fixed-soundtrack headless preparation, shared PCM,
adopted availability, ordinary canonical source selection and preserved rejection
of unsupported realtime/callback profiles. The browser script runs the original
minimal source and the full Fourier source with only the audio declaration below
inserted. It uses normal worker admission, actual render workers and Web Audio
output observation, including pause, seek, buffering, cancellation and restart.
Its report records exact sources, package identities and browser configuration.
The checks' presence is not evidence that they passed; record execution results in
the implementing PR.
The normal `scripts/check.sh` and exact-head CI gates remain required.

## Integration target

The minimal ordinary scene must run unchanged with music. For the
full integration, add the preview's `self.audio(...)` call before the first play
in the Fourier scene, with duration 34.65, linear gain 0.5, fade-in 1.5 and fade-out
2.5 seconds. The qualification script makes this single insertion without changing
the original fixture bytes or replacing ordinary `play()` / `wait()` syntax.

Keep the original sources as baselines; add improved variants rather than
rewriting a failing example to another authoring style. Implementation details,
remaining work, and acceptance ownership stay in #2358, not this fixture guide.
