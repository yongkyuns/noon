use super::{invalid, NarrationError, PreparedAudio, MAX_WAV_BYTES};

/// Read a complete RIFF file; streaming/incomplete headers are not accepted.
/// The output budget is checked before allocating decoded samples.
pub(super) fn decode(wav: &[u8], max_pcm_bytes: usize) -> Result<PreparedAudio, NarrationError> {
    if wav.len() > MAX_WAV_BYTES {
        return Err(invalid("WAV exceeds the preparation limit"));
    }
    if wav.len() < 12
        || &wav[..4] != b"RIFF"
        || &wav[8..12] != b"WAVE"
        || u64::from(u32_at(wav, 4)) + 8 != wav.len() as u64
    {
        return Err(invalid("invalid or truncated WAV container"));
    }
    let mut format = None;
    let mut samples = None;
    let mut offset = 12_usize;
    while offset < wav.len() {
        let begin = offset
            .checked_add(8)
            .filter(|end| *end <= wav.len())
            .ok_or_else(|| invalid("truncated WAV chunk header"))?;
        let size = u32_at(wav, offset + 4) as usize;
        let end = begin
            .checked_add(size)
            .filter(|end| *end <= wav.len())
            .ok_or_else(|| invalid("truncated WAV chunk"))?;
        let next = end
            .checked_add(size % 2)
            .filter(|end| *end <= wav.len())
            .ok_or_else(|| invalid("truncated WAV chunk padding"))?;
        match &wav[offset..offset + 4] {
            b"fmt " if format.is_some() => {
                return Err(invalid("WAV must contain one format chunk"));
            }
            b"fmt " => format = Some(&wav[begin..end]),
            b"data" if samples.is_some() => {
                return Err(invalid("WAV must contain one PCM data chunk"));
            }
            b"data" => samples = Some(&wav[begin..end]),
            _ => {}
        }
        offset = next;
    }
    let format = format.ok_or_else(|| invalid("WAV has no format chunk"))?;
    let samples = samples.ok_or_else(|| invalid("WAV has no PCM data chunk"))?;
    if format.len() < 16 || u16_at(format, 0) != 1 || u16_at(format, 14) != 16 {
        return Err(invalid("only uncompressed 16-bit PCM WAV is supported"));
    }
    let channels = u16_at(format, 2);
    let sample_rate = u32_at(format, 4);
    if !matches!(channels, 1 | 2) || !(8_000..=192_000).contains(&sample_rate) {
        return Err(invalid("WAV requires 8–192 kHz mono or stereo audio"));
    }
    let frame_bytes = channels * 2;
    if u16_at(format, 12) != frame_bytes
        || u32_at(format, 8) != sample_rate * u32::from(frame_bytes)
    {
        return Err(invalid(
            "WAV byte rate or block alignment does not match PCM format",
        ));
    }
    if samples.is_empty() || !samples.len().is_multiple_of(usize::from(frame_bytes)) {
        return Err(invalid("WAV must contain nonempty complete PCM frames"));
    }
    let output_size = samples
        .len()
        .checked_mul(2)
        .filter(|size| *size <= max_pcm_bytes)
        .ok_or_else(|| invalid("decoded PCM exceeds the preparation limit"))?;
    let mut pcm = Vec::with_capacity(output_size);
    for sample in samples.as_chunks::<2>().0 {
        let sample = f32::from(i16::from_le_bytes(*sample)) / 32768.0;
        pcm.extend_from_slice(&sample.to_le_bytes());
    }
    PreparedAudio::from_pcm(pcm, sample_rate, channels)
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("validated WAV field range"),
    )
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated WAV field range"),
    )
}
