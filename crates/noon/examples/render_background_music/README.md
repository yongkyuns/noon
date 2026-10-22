# A Small Discovery

An original, quiet instrumental bed prepared for Noon's audio work in
[#2263](https://github.com/yongkyuns/noon/issues/2263). The 80-second stereo loop
uses sparse soft keys over slowly changing warm pads. It contains no recordings,
instrument samples, vocals, drums, downloaded music, or model-generated audio.
The score was composed for this example; it does not intentionally quote an
existing melody.

This command prepares a WAV asset from the same shared score used by the
finite direct Rust/WASM background-audio example. The command itself opens no
audio device. Prepare the asset once and reuse it; do not run this generator on
every scene update or seek. Native device output, Python worker playback, and
synchronized export remain tracked in the issue.

## Render and test

From the repository root:

```sh
cargo run -p noon --no-default-features --release --example render_background_music -- /tmp/a-small-discovery.wav
cargo test -p noon --no-default-features --lib example_scenes::background_audio
```

The synthesizer lives in
`crates/noon/src/example_scenes/background_audio/music.rs` and uses only Rust’s
standard library. The CLI and direct scene builder import that one module, so
they share the original score and synthesis.

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
measures about -26.4 LUFS integrated. The direct scene example uses a separate
authored gain of 0.5 for a quiet bed, with a 1.5-second entrance and 2.5-second
exit fade over its 30-second scene interval.

Note tails and reflections wrap into the beginning of the buffer. Loop the
complete WAV/PCM without dropping a frame or fading each cycle. The first and
last samples are consecutive waveform samples, not duplicate endpoints.

The loop begins with existing tails already sounding. Apply a short gain ramp
at an actual playback start, stop, or discontinuous seek to avoid a transient:
roughly 50–100 ms in and 150–250 ms out are starting points. This is output
transition handling, separate from authored timeline fades. Compressed listening
previews may include intro/outro fades and encoder padding; use the full WAV/PCM
as the canonical loop.

## Declare audio in a Rust scene

Prepare one immutable PCM resource, then attach it before lowering the scene:

```rust
use noon::{AudioClipOptions, AudioResource, Scene, DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET};
use noon::example_scenes::background_audio::music;

let mut scene = Scene::new();
let pcm = music::render_music(24_000)?;
let resource = AudioResource::from_interleaved(
    pcm.sample_rate(),
    music::CHANNELS as u16,
    pcm.into_samples(),
    DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET,
)?;
let clip = scene.audio(resource, AudioClipOptions {
    start_time: 0.0,
    duration: Some(30.0),
    gain: 0.5,
    fade_in: 1.5,
    fade_out: 2.5,
    ..AudioClipOptions::default()
})?;
```

`start_time` is absolute authored time; the call does not advance `scene.time()`
or insert a wait. A looping clip uses `looped: true` with an explicit finite
duration. Source trim uses PCM frame indices, and fades span the whole clip.
Viewer mute and volume belong to playback controls and do not change authored
gain. The returned clip is an ordinary semantic identity owned by this scene’s
root scope.

The full `background_audio::scene()` builder returns a Scene and its declared
animation root. Its sections at 0, 10 and 20 seconds align with the moving point’s
three vertices. Native Rust can lower and inspect the same animation and audio;
the direct Rust/WASM example supplies browser audio output. Live clip edits,
Python worker output, native device output and movie audio muxing are explicitly
outside this initial execution profile.

## Composition and audio dedication

**CC0 1.0 Universal (CC0-1.0)** applies specifically to the original musical
composition represented by `CHORDS` and `PHRASES` in
`crates/noon/src/example_scenes/background_audio/music.rs`, and to audio
rendered from this example. To the extent possible under law, the Noon
contributors waive copyright and related rights in that composition and audio
under the [CC0 1.0 dedication](https://creativecommons.org/publicdomain/zero/1.0/).
The [legal code](https://creativecommons.org/publicdomain/zero/1.0/legalcode)
defines the dedication. Attribution is welcome but not required.

The software implementation remains under the Noon crate's **MIT** license.
This notice does not change the license of other Noon code or assets.
