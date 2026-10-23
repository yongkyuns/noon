//! Finalization of the installed FFmpeg writer's bounded two-track profile.
//!
//! AAC media samples, their timing table and encoder-priming edit offset remain
//! unchanged. Only presentation duration is trimmed; the decoder still receives
//! the original packets needed for AAC's overlap and priming. The exact movie
//! end follows the video clock. The audio edit ends at the last complete sample
//! at or before it, so demuxer sample rounding cannot extend the movie.

use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;

use super::{
    atom_at, child, integer_ticks, invalid, read_movie, set_u32, u32_at, version_zero, Atom,
    FrameRate,
};

const AUDIO_RATE: u32 = 48_000;
const AAC_FRAME: u32 = 1_024;

/// Known numeric limits are checked before staging or starting the encoder.
pub(super) struct SoundtrackGrid {
    frames: u32,
    full_end: u32,
    movie_scale: u32,
    video_tick_multiplier: u32,
    audio_tick_multiplier: u32,
}

pub(super) fn validate_grid(
    rate: FrameRate,
    frames: u64,
    pcm_frames: u64,
) -> io::Result<SoundtrackGrid> {
    if frames == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "soundtrack MP4 requires a nonempty video grid",
        ));
    }
    let frames = u32::try_from(frames)
        .map_err(|_| invalid("MP4 frame count exceeds version-zero sample table"))?;
    let full_end = frames
        .checked_mul(rate.denominator())
        .ok_or_else(|| invalid("MP4 media duration exceeds version-zero field"))?;
    let expected_pcm =
        (u128::from(full_end) * u128::from(AUDIO_RATE)).div_ceil(u128::from(rate.numerator()));
    if u128::from(pcm_frames) != expected_pcm || pcm_frames == 0 {
        return Err(invalid(
            "soundtrack PCM must cover exactly the complete video grid",
        ));
    }
    pcm_frames
        .checked_add(u64::from(AAC_FRAME))
        .and_then(|duration| u32::try_from(duration).ok())
        .ok_or_else(|| invalid("AAC media duration exceeds version-zero field"))?;
    let mut gcd = rate.numerator();
    let mut remainder = AUDIO_RATE;
    while remainder != 0 {
        (gcd, remainder) = (remainder, gcd % remainder);
    }
    let movie_scale = (rate.numerator() / gcd)
        .checked_mul(AUDIO_RATE)
        .ok_or_else(|| invalid("common video/audio movie time base exceeds version-zero field"))?;
    let video_tick_multiplier = movie_scale / rate.numerator();
    let audio_tick_multiplier = movie_scale / AUDIO_RATE;
    full_end
        .checked_mul(video_tick_multiplier)
        .ok_or_else(|| invalid("common-clock movie duration exceeds version-zero field"))?;
    Ok(SoundtrackGrid {
        frames,
        full_end,
        movie_scale,
        video_tick_multiplier,
        audio_tick_multiplier,
    })
}

#[derive(Clone, Copy)]
struct Track {
    trak: Atom,
    tkhd: Atom,
    elst: Atom,
    mdia: Atom,
    mdhd: Atom,
    minf: Atom,
    stbl: Atom,
    stts: Atom,
}

fn payload_at_least(atom: Atom, length: usize) -> io::Result<()> {
    if atom.end - atom.payload() < length {
        Err(invalid("truncated MP4 timing atom payload"))
    } else {
        Ok(())
    }
}

impl Track {
    fn read(bytes: &[u8], trak: Atom) -> io::Result<([u8; 4], Self)> {
        let tkhd = child(bytes, trak, *b"tkhd")?;
        let edts = child(bytes, trak, *b"edts")?;
        let elst = child(bytes, edts, *b"elst")?;
        let mdia = child(bytes, trak, *b"mdia")?;
        let mdhd = child(bytes, mdia, *b"mdhd")?;
        let hdlr = child(bytes, mdia, *b"hdlr")?;
        let minf = child(bytes, mdia, *b"minf")?;
        let stbl = child(bytes, minf, *b"stbl")?;
        let stts = child(bytes, stbl, *b"stts")?;
        for atom in [trak, tkhd, edts, elst, mdia, mdhd, hdlr, minf, stbl, stts] {
            if atom.header != 8 {
                return Err(invalid("unsupported extended soundtrack timing atom"));
            }
        }
        for atom in [tkhd, elst, mdhd, stts] {
            version_zero(bytes, atom)?;
        }
        payload_at_least(tkhd, 24)?;
        payload_at_least(mdhd, 20)?;
        payload_at_least(hdlr, 12)?;
        payload_at_least(stts, 8)?;
        if elst.len() != 28
            || u32_at(bytes, elst.payload() + 4)? != 1
            || u32_at(bytes, elst.payload() + 16)? != 0x0001_0000
        {
            return Err(invalid(
                "soundtrack MP4 requires one unit-rate presentation edit",
            ));
        }
        let mut handler = [0; 4];
        handler.copy_from_slice(&bytes[hdlr.payload() + 8..hdlr.payload() + 12]);
        Ok((
            handler,
            Self {
                trak,
                tkhd,
                elst,
                mdia,
                mdhd,
                minf,
                stbl,
                stts,
            },
        ))
    }
}

fn tracks(bytes: &[u8], root: Atom) -> io::Result<(Track, Track)> {
    let mut video = None;
    let mut audio = None;
    let mut cursor = root.payload();
    while cursor < root.end {
        let (kind, atom) = atom_at(bytes, cursor, root.end)?;
        match &kind {
            b"trak" => {
                let (handler, track) = Track::read(bytes, atom)?;
                let target = match &handler {
                    b"vide" => &mut video,
                    b"soun" => &mut audio,
                    _ => return Err(invalid("unsupported soundtrack MP4 track handler")),
                };
                if target.replace(track).is_some() {
                    return Err(invalid(
                        "soundtrack MP4 requires exactly one video and one audio track",
                    ));
                }
            }
            // No fragments or additional movie-time structures are admitted.
            b"mvhd" | b"udta" => {}
            _ => return Err(invalid("unsupported soundtrack MP4 movie atom")),
        }
        cursor = atom.end;
    }
    Ok((
        video.ok_or_else(|| invalid("soundtrack MP4 video track is absent"))?,
        audio.ok_or_else(|| invalid("soundtrack MP4 audio track is absent"))?,
    ))
}

fn sample_entry(bytes: &[u8], track: Track) -> io::Result<([u8; 4], Atom)> {
    let stsd = child(bytes, track.stbl, *b"stsd")?;
    version_zero(bytes, stsd)?;
    payload_at_least(stsd, 8)?;
    if stsd.header != 8 || u32_at(bytes, stsd.payload() + 4)? != 1 {
        return Err(invalid(
            "soundtrack MP4 requires one codec description per track",
        ));
    }
    let (kind, entry) = atom_at(bytes, stsd.payload() + 8, stsd.end)?;
    if entry.header != 8 || entry.end != stsd.end {
        return Err(invalid("unsupported soundtrack MP4 codec description"));
    }
    Ok((kind, entry))
}

fn descriptor(bytes: &[u8]) -> io::Result<(u8, &[u8], &[u8])> {
    let tag = *bytes
        .first()
        .ok_or_else(|| invalid("missing AAC descriptor"))?;
    let mut length = 0_usize;
    for index in 1..=4 {
        let byte = *bytes
            .get(index)
            .ok_or_else(|| invalid("truncated AAC descriptor length"))?;
        length = (length << 7) | usize::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            let start = index + 1;
            let end = start
                .checked_add(length)
                .filter(|&end| end <= bytes.len())
                .ok_or_else(|| invalid("truncated AAC descriptor"))?;
            return Ok((tag, &bytes[start..end], &bytes[end..]));
        }
    }
    Err(invalid("oversized AAC descriptor length"))
}

fn validate_aac_description(bytes: &[u8], track: Track) -> io::Result<()> {
    let (kind, entry) = sample_entry(bytes, track)?;
    payload_at_least(entry, 28)?;
    let p = entry.payload();
    if kind != *b"mp4a"
        || bytes[p..p + 6] != [0; 6]
        || bytes[p + 6..p + 8] != [0, 1]
        || bytes[p + 8..p + 16] != [0; 8]
        || bytes[p + 16..p + 20] != [0, 2, 0, 16]
        || bytes[p + 20..p + 24] != [0; 4]
        || u32_at(bytes, p + 24)? != AUDIO_RATE << 16
    {
        return Err(invalid(
            "soundtrack MP4 requires 48 kHz stereo AAC sample entries",
        ));
    }
    let mut cursor = p + 28;
    let mut esds = None;
    while cursor < entry.end {
        let (kind, atom) = atom_at(bytes, cursor, entry.end)?;
        match &kind {
            b"esds" if esds.replace(atom).is_none() => {}
            b"btrt" => {}
            _ => return Err(invalid("unsupported AAC sample-entry extension")),
        }
        cursor = atom.end;
    }
    let esds = esds.ok_or_else(|| invalid("AAC decoder configuration is absent"))?;
    version_zero(bytes, esds)?;
    payload_at_least(esds, 4)?;
    if esds.header != 8 {
        return Err(invalid("unsupported extended AAC configuration atom"));
    }
    let (tag, es, trailing) = descriptor(&bytes[esds.payload() + 4..esds.end])?;
    if tag != 3 || !trailing.is_empty() || es.len() < 3 || es[2] != 0 {
        return Err(invalid("unsupported AAC elementary-stream descriptor"));
    }
    let (tag, decoder, sl) = descriptor(&es[3..])?;
    if tag != 4 || decoder.len() < 13 || decoder[0] != 0x40 || decoder[1] != 0x15 {
        return Err(invalid("soundtrack MP4 requires MPEG-4 AAC audio"));
    }
    let (tag, config, trailing) = descriptor(&decoder[13..])?;
    // AAC-LC, 48 kHz, stereo, 1024-sample frames, no SBR/PS. The installed
    // encoder may include the standard explicit SBR-absent sync extension.
    if tag != 5
        || !trailing.is_empty()
        || !matches!(config, [0x11, 0x90] | [0x11, 0x90, 0x56, 0xe5, 0x00])
    {
        return Err(invalid(
            "unsupported AAC object type, rate, channels or extensions",
        ));
    }
    let (tag, predefined, trailing) = descriptor(sl)?;
    if tag != 6 || predefined != [2] || !trailing.is_empty() {
        return Err(invalid("unsupported AAC synchronization descriptor"));
    }
    Ok(())
}

fn movie_ticks(numerator: u64, denominator: u32, scale: u32, ceil: bool) -> io::Result<u32> {
    let scaled = u128::from(numerator) * u128::from(scale);
    let denominator = u128::from(denominator);
    let ticks = if ceil {
        scaled.div_ceil(denominator)
    } else {
        scaled / denominator
    };
    u32::try_from(ticks).map_err(|_| invalid("soundtrack MP4 duration exceeds version-zero field"))
}

fn validate_video(
    bytes: &[u8],
    video: Track,
    rate: FrameRate,
    frames: u32,
    full_end: u32,
    movie_scale: u32,
) -> io::Result<()> {
    let expected_movie = movie_ticks(u64::from(full_end), rate.numerator(), movie_scale, true)?;
    if sample_entry(bytes, video)?.0 != *b"avc1"
        || u32_at(bytes, video.mdhd.payload() + 12)? != rate.numerator()
        || u32_at(bytes, video.mdhd.payload() + 16)? != full_end
        || u32_at(bytes, video.tkhd.payload() + 20)? != expected_movie
        || u32_at(bytes, video.elst.payload() + 8)? != expected_movie
        || u32_at(bytes, video.elst.payload() + 12)? != 0
        || video.stts.len() != 24
        || u32_at(bytes, video.stts.payload() + 4)? != 1
        || u32_at(bytes, video.stts.payload() + 8)? != frames
        || u32_at(bytes, video.stts.payload() + 12)? != rate.denominator()
    {
        return Err(invalid(
            "soundtrack video timing does not match the exported frame grid",
        ));
    }
    Ok(())
}

fn validate_audio(bytes: &[u8], audio: Track, pcm_frames: u64, movie_scale: u32) -> io::Result<()> {
    validate_aac_description(bytes, audio)?;
    let media_duration = pcm_frames
        .checked_add(u64::from(AAC_FRAME))
        .and_then(|duration| u32::try_from(duration).ok())
        .ok_or_else(|| invalid("AAC media duration exceeds version-zero field"))?;
    let packet_count = pcm_frames.div_ceil(u64::from(AAC_FRAME)) + 1;
    let full_packets = media_duration / AAC_FRAME;
    let remainder = media_duration % AAC_FRAME;
    let entries = if remainder == 0 { 1 } else { 2 };
    let stts = audio.stts;
    // FFmpeg rounds the media span and priming offset into movie ticks
    // separately. Depending on the remainder, their difference is the floor
    // or ceiling of the PCM span (including for a sub-millisecond input).
    // The exact media table below, not that rounded edit, proves PCM length.
    let original_edit = u32_at(bytes, audio.elst.payload() + 8)?;
    let minimum_edit = movie_ticks(pcm_frames, AUDIO_RATE, movie_scale, false)?;
    let maximum_edit = movie_ticks(pcm_frames, AUDIO_RATE, movie_scale, true)?;
    if u32_at(bytes, audio.mdhd.payload() + 12)? != AUDIO_RATE
        || u32_at(bytes, audio.mdhd.payload() + 16)? != media_duration
        || u32_at(bytes, audio.tkhd.payload() + 20)?
            != movie_ticks(pcm_frames, AUDIO_RATE, movie_scale, true)?
        || !(minimum_edit..=maximum_edit).contains(&original_edit)
        || u32_at(bytes, audio.elst.payload() + 12)? != AAC_FRAME
        || stts.len() != 16 + entries * 8
        || u32_at(bytes, stts.payload() + 4)? != entries as u32
        || u32_at(bytes, stts.payload() + 8)? != full_packets
        || u32_at(bytes, stts.payload() + 12)? != AAC_FRAME
        || (remainder != 0
            && (u32_at(bytes, stts.payload() + 16)? != 1
                || u32_at(bytes, stts.payload() + 20)? != remainder))
    {
        return Err(invalid(
            "AAC timing or priming does not match the staged PCM",
        ));
    }
    let stsz = child(bytes, audio.stbl, *b"stsz")?;
    version_zero(bytes, stsz)?;
    payload_at_least(stsz, 12)?;
    if u64::from(u32_at(bytes, stsz.payload() + 8)?) != packet_count {
        return Err(invalid("AAC packet count does not match the staged PCM"));
    }
    Ok(())
}

pub(super) fn finish(
    path: &Path,
    rate: FrameRate,
    frames: u64,
    source_end: Option<f64>,
    pcm_frames: u64,
) -> io::Result<bool> {
    if source_end.is_some_and(|end| !end.is_finite() || end < 0.0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid authored soundtrack endpoint",
        ));
    }
    let grid = validate_grid(rate, frames, pcm_frames)?;
    let frames = grid.frames;
    let full_end = grid.full_end;
    let end = source_end
        .and_then(|end| integer_ticks(end, rate.numerator()))
        .unwrap_or(full_end);
    let last_start = full_end - rate.denominator();
    if end <= last_start || end > full_end {
        return Err(invalid(
            "authored endpoint does not follow the last exported PTS",
        ));
    }
    // The common movie clock represents both this exact video endpoint and a
    // whole audio sample. Floor only the audio edit: rounding a fractional audio
    // endpoint upward would make demuxers report a movie longer than the video.
    let video_movie_end = end * grid.video_tick_multiplier;
    let audio_samples = u64::from(end) * u64::from(AUDIO_RATE) / u64::from(rate.numerator());
    let audio_movie_end = u32::try_from(audio_samples)
        .ok()
        .and_then(|samples| samples.checked_mul(grid.audio_tick_multiplier))
        .ok_or_else(|| invalid("audio presentation duration exceeds version-zero field"))?;
    let (mut file, offset, mut moov, root) = read_movie(path)?;
    let original_size = moov.len();
    let mvhd = child(&moov, root, *b"mvhd")?;
    version_zero(&moov, mvhd)?;
    payload_at_least(mvhd, 20)?;
    if mvhd.header != 8 {
        return Err(invalid("unsupported extended soundtrack movie header"));
    }
    let original_scale = u32_at(&moov, mvhd.payload() + 12)?;
    if original_scale == 0 {
        return Err(invalid("MP4 movie time base is zero"));
    }
    let (video, audio) = tracks(&moov, root)?;
    validate_video(&moov, video, rate, frames, full_end, original_scale)?;
    validate_audio(&moov, audio, pcm_frames, original_scale)?;
    let maximum_track_duration =
        u32_at(&moov, video.tkhd.payload() + 20)?.max(u32_at(&moov, audio.tkhd.payload() + 20)?);
    if u32_at(&moov, mvhd.payload() + 16)? != maximum_track_duration {
        return Err(invalid(
            "soundtrack MP4 movie duration differs from its tracks",
        ));
    }

    // Complete validation precedes any write. Both edits use movie ticks;
    // their media_time fields retain their independent video/AAC units.
    set_u32(&mut moov, mvhd.payload() + 12, grid.movie_scale)?;
    set_u32(&mut moov, mvhd.payload() + 16, video_movie_end)?;
    for (track, duration) in [(video, video_movie_end), (audio, audio_movie_end)] {
        set_u32(&mut moov, track.tkhd.payload() + 20, duration)?;
        set_u32(&mut moov, track.elst.payload() + 8, duration)?;
    }
    let shortened = end != full_end;
    if shortened {
        set_u32(&mut moov, video.mdhd.payload() + 16, end)?;
        if frames == 1 {
            set_u32(&mut moov, video.stts.payload() + 12, end)?;
        } else {
            set_u32(&mut moov, video.stts.payload() + 4, 2)?;
            set_u32(&mut moov, video.stts.payload() + 8, frames - 1)?;
            let extra = [1_u32.to_be_bytes(), (end - last_start).to_be_bytes()].concat();
            moov.splice(video.stts.end..video.stts.end, extra);
            for atom in [
                video.stts, video.stbl, video.minf, video.mdia, video.trak, root,
            ] {
                let expanded = u32::try_from(atom.len() + 8)
                    .map_err(|_| invalid("expanded MP4 atom exceeds version-zero length"))?;
                set_u32(&mut moov, atom.start, expanded)?;
            }
        }
    }
    let growth = usize::from(shortened && frames > 1) * 8;
    if moov.len() != original_size + growth {
        return Err(invalid("MP4 final sample expansion was inconsistent"));
    }
    // Consuming only the existing free atom keeps both tracks' chunk offsets
    // and every encoded packet byte unchanged, even if audio precedes video.
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(&moov)?;
    file.flush()?;
    Ok(shortened)
}

#[cfg(test)]
mod tests;
