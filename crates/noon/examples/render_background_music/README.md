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
every scene update or seek. Finite native scenes can also render this score
directly into an MP4 through `noon-export`; native device output, Python worker
playback and broader live export remain tracked in the issue.

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

The loop begins with existing tails already sounding. The direct browser adapter
uses a 40 ms output entrance and a release of up to 60 ms for ordinary pause or
seek, separately from the authored timeline fades. Viewer mute and volume use
20 ms ramps. These values smooth control changes without changing the source
offset, authored duration, or the loop boundary. Human listening review remains
open; numerical and browser checks do not establish final musical quality.

A new source starts at its exact playback anchor and advances silently while the
previous source releases; its output entrance follows that release. Rapid seeks
replace the still-muted source, so at most one current graph and one releasing
graph are retained. The native source-ended event cleans up the release, without
a polling timer or another scene clock. Pausing freezes picture time immediately
while the old device output finishes its bounded release. Hidden-page,
interruption, failure, retirement, and exact finite-completion cancellation
remain immediate.

Compressed listening previews may include intro/outro fades and encoder padding;
use the full WAV/PCM as the canonical loop.

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
three vertices. Native Rust can export the same animation and audio; the direct
Rust/WASM example supplies browser audio output. To create the 30-second movie
with its soundtrack in one encode:

```sh
cargo run --release -p noon-export --example render_audio_video -- scene-with-music.mp4
```

The finite native export uses the shared clip evaluator and mixer, preserving
authored gain/fades and crop phase. Viewer mute/volume and output transition
ramps do not change the soundtrack. See the [native export guide](../../../noon-export/README.md)
for admission, encoding and failure behavior. Live clip edits, Python worker
output, native device output and live/history movie audio remain outside this
finite execution profile.

## Put playback buttons in the scene

The direct background-audio example includes five ordinary filled rectangles
bound to shared Rust playback actions. For example:

```rust
use noon::PlaybackControl;

let mut button = scene.rectangle(1.7, 0.65)?;
button.set_fill(0.09, 0.17, 0.20, 1.0)?;
scene.add(&button)?;
scene.on_click_playback(&button, PlaybackControl::TogglePlayback)?;
```

`PreviousSection`, `NextSection`, `Restart` and `ToggleAudioMuted` use the same
declaration. The existing Rust picker recognizes a completed primary-button
click on the displayed object. The input publication returns a typed request;
the direct host applies it through its normal playback commands after input
commits. Dragging away, cancelling a gesture or clicking the background does
not activate a button. Previous/Next/Restart preserve the current play/pause
state, and mute changes the viewer's output without editing the authored gain.

The example keeps stroke-only icons inside each button and scene text captions
below it. The current picker supports analytic circle/rectangle fills and
deliberately reports unsupported topmost fills, including Text. A text label
placed over a button would therefore block that fill; this example does not
introduce a separate frontend hit-test rule. The equivalent HTML controls remain
available for keyboard use.

These controls currently run in the finite direct Rust/WASM Window profile.
The first Play requires an ordinary browser user gesture to enable audio.
Native host controls, Python/render-worker controls and live-history integration
remain owned by #2263. The rectangles themselves are authored visual content;
they are part of a rendered frame unless the author omits them from an export
scene. Their viewer input is not replayed as an authored animation.

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
