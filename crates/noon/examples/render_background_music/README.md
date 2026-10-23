# A Small Discovery

An original, quiet instrumental bed prepared for Noon's audio work in
[#2263](https://github.com/yongkyuns/noon/issues/2263). The 80-second stereo loop
uses sparse soft keys over slowly changing warm pads. It contains no recordings,
instrument samples, vocals, drums, downloaded music, or model-generated audio.
The score was composed for this example; it does not intentionally quote an
existing melody.

This example prepares a WAV asset. It does not add scene audio playback, an
audio device, a scheduler, or synchronized export. Those remain tracked in the
issue. Prepare the asset once and reuse it; do not run this generator on every
scene update or seek.

## Render and test

From the repository root:

```sh
cargo run -p noon --no-default-features --release --example render_background_music -- /tmp/a-small-discovery.wav
cargo test -p noon --no-default-features --example render_background_music
```

The example itself uses only Rust's standard library. It can also be rendered
without building Noon or downloading any crate dependencies:

```sh
rustc --edition=2021 -O crates/noon/examples/render_background_music/main.rs -o /tmp/render-background-music
/tmp/render-background-music /tmp/a-small-discovery.wav
rustc --edition=2021 --test -O crates/noon/examples/render_background_music/main.rs -o /tmp/background-music-tests
/tmp/background-music-tests
```

The optional second argument to the renderer selects a sample rate between
8,000 and 96,000 Hz. The default output is 48,000 Hz, PCM16 stereo WAV, exactly
80 seconds / 3,840,000 frames. At 48 kHz the generator uses a 30.72 MB float PCM
payload and writes a 15.36 MB WAV. Generated audio is kept out of Git; the source
score and synthesis are the reproducible asset definition.

The explicit score is sixteen bars at 48 BPM, centered on D major with added
ninths and suspended voicings. Additive oscillators, smooth envelopes, and three
short synthetic reflections produce every sample. There is no random source,
external service, or network request. Repeated renders on the same target and
sample rate are identical; floating-point transcendental functions can produce
small byte differences across platforms.

## Levels and looping

The source peak is scaled to 0.22 (about -13.15 dBFS). The initial 48 kHz render
measures about -26.4 LUFS integrated. A later scene mixer can start with a
separate gain of 0.5 for a quiet bed; narration can duck it further.

Note tails and reflections wrap into the beginning of the buffer. Loop the
complete WAV/PCM without dropping a frame or fading each cycle. The first and
last samples are consecutive waveform samples, not duplicate endpoints.

The loop begins with existing tails already sounding. Apply a short gain ramp
at an actual playback start, stop, or discontinuous seek to avoid a transient:
roughly 50–100 ms in and 150–250 ms out are starting points. This is output
transition handling, separate from authored timeline fades. Compressed listening
previews may include intro/outro fades and encoder padding; use the full WAV/PCM
as the canonical loop.

## Composition and audio dedication

**CC0 1.0 Universal (CC0-1.0)** applies specifically to the original musical
composition represented by `CHORDS` and `PHRASES` in `music.rs`, and to audio
rendered from this example. To the extent possible under law, the Noon
contributors waive copyright and related rights in that composition and audio
under the [CC0 1.0 dedication](https://creativecommons.org/publicdomain/zero/1.0/).
The [legal code](https://creativecommons.org/publicdomain/zero/1.0/legalcode)
defines the dedication. Attribution is welcome but not required.

The software implementation remains under the Noon crate's **MIT** license.
This notice does not change the license of other Noon code or assets.
