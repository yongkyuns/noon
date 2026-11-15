# Prepare narration assets

Use this workflow to prepare a recording or generated speech before authoring the
scene. It produces reusable media and review metadata. It does not add a speech
clock, execute a scene, or establish that the target preview/export host admits
the resulting audio.

## Contents

- [Write for the explanation](#write-for-the-explanation)
- [Import a recording](#import-a-recording)
- [Generate one passage](#generate-one-passage)
- [Review captions and phrase cues](#review-captions-and-phrase-cues)
- [Reuse accepted audio](#reuse-accepted-audio)
- [Attach narration and wait for cues](#attach-narration-and-wait-for-cues)
- [Render the existing lesson with speech](#render-the-existing-lesson-with-speech)
- [Supported scene profiles](#supported-scene-profiles)

## Write for the explanation

Write a coherent explanatory passage around a visible relationship. Identify what
the learner should be looking at when each consequential phrase is spoken. Allow
the visual to appear before it is discussed and retain a result long enough to
inspect it. Use silent holds when they help; do not stretch every animation to
fill a sentence or keep unrelated objects moving during speech.

Keep these inputs distinct:

- **Spoken text:** the words to say, with pronunciation-friendly wording where
  necessary. Spell out a technical expression if its notation would be misread.
- **Visible notation and captions:** the notation/text the learner should read.
  Preserve meaning even when spoken wording differs.
- **Phrase cues:** important offsets in the accepted audio, reviewed by listening.
  A measured duration alone cannot establish word alignment.

Generate or record a short coherent passage, then listen for pronunciation,
continuity, pace, and correspondence. A supplied transcript is not an automatically
verified transcript. Keep the accepted recording instead of depending on a seed
or a later request to reproduce the same voice performance.

## Import a recording

From the trusted Noon checkout, supply your WAV recording and UTF-8 spoken text:

```bash
python3 -B scripts/prepare_narration.py \
  --wav narration.wav \
  --spoken-text-file narration.txt \
  --cache-dir output/narration-cache
```

The bounded initial import accepts uncompressed 16-bit PCM WAV, mono or stereo,
at 8–192 kHz, within the 64 MiB decoded float-PCM limit. Unsupported formats,
truncated data, and empty audio fail explicitly. The tool measures duration from
the actual frame count and sample rate; it does not trim silence, normalize gain,
resample, or estimate duration from the text.

## Generate one passage

The optional provider is ElevenLabs' documented
[Create speech endpoint](https://elevenlabs.io/docs/api-reference/text-to-speech/convert).
Supply an explicit voice available to your account and model in a request file:

```json
{
  "provider": "elevenlabs",
  "voice_id": "YOUR_VOICE_ID",
  "model_id": "eleven_multilingual_v2",
  "text": "Each lane requests a different word. Their bank numbers are the same.",
  "voice_settings": {
    "stability": 0.5,
    "similarity_boost": 0.75,
    "style": 0.0,
    "use_speaker_boost": true,
    "speed": 1.0
  }
}
```

Configure `ELEVENLABS_API_KEY` in your local preparation environment, then run:

```bash
python3 -B scripts/prepare_narration.py \
  --request narration-request.json \
  --cache-dir output/narration-cache
```

Only an explicit provider request on a cache miss contacts the service. Verified
cache hits need no credentials or network. The adapter requests `wav_24000`,
retains the resulting media, and uses the same decoder as imported recordings.
It does not retry failed synthesis automatically. Keep credentials out of request
files, scene source, cached metadata, and review artifacts.

Use optional `previous_text` / `next_text` for generation continuity. Include
explicit dictionary versions when supplying `pronunciation_dictionary_locators`:
each entry uses `pronunciation_dictionary_id` and `version_id`. These inputs affect
generation and therefore participate in the request identity. `language_code`
is supported only where the selected model accepts it; the multilingual-v2 model
does not accept that parameter. This adapter implements a documented subset, not
every provider option or model-specific setting.

The checked-in tests use a controlled provider response. They verify request,
cache, and failure behavior without making a paid call. They do not establish
current service availability, voice quality, or correct pronunciation. Disclose
generated speech when presenting the resulting lesson.

## Review captions and phrase cues

Supply separate caption text and reviewed cue offsets when ready:

```bash
python3 -B scripts/prepare_narration.py \
  --wav narration.wav \
  --spoken-text-file narration.txt \
  --caption-text-file captions.txt \
  --cues cues.json \
  --cache-dir output/narration-cache
```

A cue file is a JSON list. The following illustrates its shape; replace the times
with offsets you actually reviewed within the accepted recording:

```json
[
  {
    "name": "show_shared_bank",
    "start_seconds": 1.2,
    "end_seconds": 2.1,
    "provenance": "manual"
  }
]
```

Names must be unique, listed by increasing start time, and intervals must lie
within the measured audio duration.
These are phrase intervals relative to the recording, not global scene times.
Manual timestamps are not forced alignment or provider timestamps. Revisit them
when the spoken audio changes. The first tool prepares caption text and cue
metadata; it does not generate timed subtitle files or apply them to a scene.

## Reuse accepted audio

The tool reports the prepared narration manifest. Its cache separates immutable
audio assets, synthesis request receipts, and narration metadata. Audio is stored
as little-endian interleaved float32 PCM with format, frame count, duration, and
content hashes. Reuse verifies stored content; corrupt or partial entries fail
instead of silently regenerating speech or replacing an accepted asset.

| Change | Reuse and review |
| --- | --- |
| Color, layout, emphasis, or a visual hold | Reuse accepted audio; update the visual plan and review the affected interval. |
| Caption text or a phrase cue | Reuse accepted audio; produce revised narration metadata. Check meaning and cue bounds. |
| TTS wording, voice, model, settings, pronunciation dictionary, or continuity context | Prepare a new synthesis request; review the new audio and dependent timing. |
| Imported recording | Reimport and verify its PCM. Changed samples or format produce a new audio identity. Editing its supplied transcript does not change what the recording says. |
| Placement of an unchanged passage | Reuse the asset; update placement through Noon's existing scene timing. |

Do not put scene source, visual cue assignments, or caption styling into the
synthesis key. Cache lookup guarantees reuse of accepted bytes, not deterministic
provider generation. Preserve unrelated accepted passages when revising one step.

## Attach narration and wait for cues

Import accepted bytes during preparation. The Rust `narration-import` feature
enables the versioned external manifest/PCM importer:

```rust
let passage = PreparedNarration::from_prepared(manifest_text, pcm_bytes)?;
let speech = scene.narrate(&passage)?;
scene.wait_for_narration_cue(&speech, "show_shared_bank")?;
// Author the explanatory motion using the ordinary Scene API.
scene.wait_for_narration(&speech)?;
```

`narrate` attaches one complete, untrimmed, non-looping recording at the current
authored cursor, at unit gain. It leaves that cursor unchanged. The returned
handle identifies the existing semantic audio clip; it is not a playback clock.
Wait helpers resolve its owning scene and live clip, then advance only to a
future cue start or recording end. An already-passed target is a no-op. A visual
operation may therefore outlast speech without being stretched or rewound.
Missing cues and foreign, stale, or changed clips fail before changing time.

The importer verifies exact v1 fields, bounded lengths, actual PCM format and
frame-derived duration, PCM SHA-256, audio identity, and manual cue bounds. It
does not open the manifest's path. The caller supplies complete bytes from the
recognized cache location. File access, network access, synthesis, and decoding
remain preparation work. A typed `PreparedNarration::new` constructor accepts
an existing `AudioResource`, text, and `NarrationCue::manual` values without
enabling JSON or an importer in a native/direct-WASM Rust application.

The Python proxy reaches those same Rust operations for a completed immutable
scene:

```python
passage = PreparedNarration.from_prepared(manifest_text, pcm_bytes)
result = Scene()
speech = result.narrate(passage)
result.wait_for_narration_cue(speech, "show_shared_bank")
# Use declare_value / other supported finite declarations for the visuals.
result.wait_for_narration(speech)
```

The bytes must already be available in the authoring context. This snippet does
not load local paths into a browser automatically. Python performs argument
coercion and forwards handles; Rust owns parsing, cue validation, clip admission,
and time. Caption text is accessible separately but is not automatically drawn
or converted into timed subtitles. Manual cue provenance remains manual.

Keep inspection gaps between passages on the existing authored cursor. For a
frozen final picture, use the existing export `final_hold_seconds` option: a
trailing cursor-only wait does not by itself extend an immutable timeline, and
the export hold remains silent after the recording ends.

## Render the existing lesson with speech

The maintained [Double the input example](../../../web/python/examples/background_music_lesson.md)
offers a narration variant of the same illustration. It takes three prepared
passages (positive input, negative input, zero), each with a manual `move` cue.
The Rust `scene_with_narration` and Python `completed_scene(narrations)` use the
same two-second point movements, remaining narration waits, and 0.75-second
inspection gaps between sections. Static numerical answer footers are hidden
in this variant so the prediction question has space to work.

For a reproducible synthetic-speech example, an optional preparation script uses
FFmpeg's installed Flite `slt` voice. It writes original spoken text, WAVs, manual
phrase cues, and generation provenance into a new output directory, then calls
the same preparation/cache tool:

```bash
python3 crates/noon-export/examples/narration/prepare.py output/narration-lesson
```

Use the three manifest paths it reports in positive/negative/zero order:

```bash
cargo run --locked --release -p noon-export --features narration-import \
  --example render_audio_video -- lesson.mp4 --narration \
  POSITIVE_MANIFEST NEGATIVE_MANIFEST ZERO_MANIFEST
```

Replace the uppercase placeholders with those printed paths. This uses Noon's
normal native renderer, shared audio mixer, and MP4 sink. The example selects a
0.75-second final frozen hold. FFmpeg, a supported GPU/software Vulkan adapter,
and the native Rust build are required; Flite is only required by the optional
fixture generator. It is explicitly generated speech, not a qualification of
voice quality, pronunciation, comprehension, or automatic word alignment.

## Supported scene profiles

Read the [audio example documentation](../../../web/python/examples/background_music_lesson.md)
and [export profiles](../../../crates/noon-export/README.md) in the current checkout.
Narration enters through the existing Rust `AudioResource` and `Scene::audio`
boundary. Use the same accepted PCM and measured timing for preview and export.
Keep TTS, file I/O, decoding, and alignment outside playback and renderer callbacks.

The first narration helpers require a fully declared finite scene before
execution. They do not broaden ordinary source preparation, running audio
mutation, `ScriptScene`/`BlockingScene`, or native CPython admission. Native
Python audio/export work is tracked separately in
[#2512](https://github.com/yongkyuns/noon/pull/2512); an open PR is not a shipped
capability. Query the current checkout and qualify the actual host before
claiming support. Broader prepared-source admission, automatic alignment, and
native Python narration remain under [#2263](https://github.com/yongkyuns/noon/issues/2263)
and [#2581](https://github.com/yongkyuns/noon/issues/2581).
