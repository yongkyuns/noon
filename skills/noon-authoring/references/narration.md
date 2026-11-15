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
- [Integrate through the current scene profile](#integrate-through-the-current-scene-profile)

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

## Integrate through the current scene profile

Read the [audio example documentation](../../../web/python/examples/background_music_lesson.md)
and [export profiles](../../../crates/noon-export/README.md) in the current checkout.
The underlying ingress is `AudioSource.from_interleaved` / Rust
`AudioResource::from_interleaved`; the preparation tool does not change admission.
Use the same accepted PCM and measured timing for supported preview and export.
Keep TTS, file I/O, decoding, and alignment outside playback and renderer callbacks.

Prepared ordinary Python source still has narrower audio admission than a fully
declared finite scene. Native Python audio/export work is tracked separately in
[#2512](https://github.com/yongkyuns/noon/pull/2512); an open PR is not a shipped
capability. Query the current checkout before claiming support. Narration-block
helpers, general prepared-source admission, and a complete ordinary narrated
preview/export lesson remain under [#2263](https://github.com/yongkyuns/noon/issues/2263)
and [#2581](https://github.com/yongkyuns/noon/issues/2581).
