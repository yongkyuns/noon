# Fourier and audio acceptance baselines

These are the complete sources investigated in #2358, not proposed APIs or a
claim that the combined animation and soundtrack are supported.

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
NumPy during preparation. Do not enroll the failing combined source as a passing
gallery example.

## Focused checks

From the repository root:

```sh
python3 -m unittest discover -s web/python -p 'test_fourier_audio_fixtures.py' -v
cargo test -p noon-runtime unsupported_audio_history_does_not_open_a_retention_scope
cargo test -p noon initial_audio_seek_remains_available_without_live_history_retention
```

The Python checks verify fixture identity/syntax, choreography durations,
absence of explicit frame callbacks, and numerical preparation using the actual
pure functions extracted from the Fourier fixture. Numerical checks require
NumPy and report explicit skips when it is unavailable. They do not substitute a
fake Noon runtime and do not establish renderer, worker, audio-output, or seeking
correctness.

The Rust checks exercise authoritative rejection, preserved publication/state,
PCM reuse, and continued finite seeking after rejection. Their presence is not
evidence that they passed; record execution results in the implementing PR.
The normal `scripts/check.sh` and exact-head CI gates remain required.

## Integration target

The minimal ordinary scene must eventually run unchanged with music. For the
full integration, add the preview's `self.audio(...)` call before the first play
in the Fourier scene, with duration 34.65, linear gain 0.5, fade-in 1.5 and fade-out
2.5 seconds. This is a future positive fixture until shared history/resource and
worker-output support is implemented and qualified under #2263 / #2211.

Keep the original sources as baselines; add improved variants rather than
rewriting a failing example to another authoring style. Implementation details,
remaining work, and acceptance ownership stay in #2358, not this fixture guide.
