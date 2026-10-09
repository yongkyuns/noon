//! Exact terminal MP4 sample duration without resampling the authored frame grid.
//!
//! The pinned FFmpeg H.264 writer uses one video track, version-0 timing boxes,
//! one constant STTS run, faststart MOOV before MDAT, and an 8-byte free atom
//! between them. Replace that padding with a second STTS run for the *last*
//! sample. The MDAT offset and every frame PTS stay byte-for-byte unchanged.
//! Reject unfamiliar layouts instead of publishing a video with unverified timing.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

use super::FrameRate;

const MAX_MOOV_BYTES: usize = 64 * 1024 * 1024;

fn invalid(reason: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason)
}

fn u32_at(bytes: &[u8], index: usize) -> io::Result<u32> {
    let slice = bytes
        .get(index..index.saturating_add(4))
        .ok_or_else(|| invalid("truncated MP4 integer"))?;
    Ok(u32::from_be_bytes(slice.try_into().expect("four bytes")))
}

fn set_u32(bytes: &mut [u8], index: usize, value: u32) -> io::Result<()> {
    let slice = bytes
        .get_mut(index..index.saturating_add(4))
        .ok_or_else(|| invalid("truncated MP4 integer"))?;
    slice.copy_from_slice(&value.to_be_bytes());
    Ok(())
}

fn u64_at(bytes: &[u8], index: usize) -> io::Result<u64> {
    let slice = bytes
        .get(index..index.saturating_add(8))
        .ok_or_else(|| invalid("truncated MP4 integer"))?;
    Ok(u64::from_be_bytes(slice.try_into().expect("eight bytes")))
}

#[derive(Clone, Copy, Debug)]
struct Atom {
    start: usize,
    end: usize,
    header: usize,
}

impl Atom {
    fn len(self) -> usize {
        self.end - self.start
    }

    fn payload(self) -> usize {
        self.start + self.header
    }
}

fn atom_at(bytes: &[u8], start: usize, end: usize) -> io::Result<([u8; 4], Atom)> {
    if end > bytes.len() {
        return Err(invalid("MP4 atom range exceeds buffer"));
    }
    let raw = bytes
        .get(start..start.saturating_add(8))
        .ok_or_else(|| invalid("truncated MP4 atom header"))?;
    let mut name = [0_u8; 4];
    name.copy_from_slice(&raw[4..8]);
    let word = u32::from_be_bytes(raw[..4].try_into().expect("four bytes"));
    let (size, header) = if word == 1 {
        (
            usize::try_from(u64_at(bytes, start + 8)?)
                .map_err(|_| invalid("oversized MP4 atom"))?,
            16,
        )
    } else if word >= 8 {
        (word as usize, 8)
    } else {
        return Err(invalid("unsupported MP4 atom size"));
    };
    if size < header || start.checked_add(size).is_none_or(|stop| stop > end) {
        return Err(invalid("malformed MP4 atom extent"));
    }
    Ok((
        name,
        Atom {
            start,
            end: start + size,
            header,
        },
    ))
}

fn child(bytes: &[u8], parent: Atom, wanted: [u8; 4]) -> io::Result<Atom> {
    let mut cursor = parent.payload();
    let mut found = None;
    while cursor < parent.end {
        let (kind, entry) = atom_at(bytes, cursor, parent.end)?;
        if kind == wanted {
            if found.replace(entry).is_some() {
                return Err(invalid("duplicated MP4 timing atom or track"));
            }
        }
        cursor = entry.end;
    }
    found.ok_or_else(|| invalid("required MP4 timing atom is absent"))
}

fn version_zero(bytes: &[u8], atom: Atom) -> io::Result<()> {
    if bytes.get(atom.payload()).copied() == Some(0) {
        Ok(())
    } else {
        Err(invalid("unsupported version-one MP4 timing atom"))
    }
}

fn disk_atom(file: &mut File, position: u64, length: u64) -> io::Result<([u8; 4], u64)> {
    if position.checked_add(8).is_none_or(|stop| stop > length) {
        return Err(invalid("truncated top-level MP4 atom"));
    }
    file.seek(SeekFrom::Start(position))?;
    let mut header = [0_u8; 16];
    file.read_exact(&mut header[..8])?;
    let size32 = u32::from_be_bytes(header[..4].try_into().expect("four bytes"));
    let mut name = [0_u8; 4];
    name.copy_from_slice(&header[4..8]);
    let size = if size32 == 1 {
        if position.checked_add(16).is_none_or(|stop| stop > length) {
            return Err(invalid("truncated extended MP4 atom"));
        }
        file.read_exact(&mut header[8..])?;
        u64::from_be_bytes(header[8..].try_into().expect("eight bytes"))
    } else if size32 >= 8 {
        u64::from(size32)
    } else {
        return Err(invalid("unsupported top-level MP4 atom size"));
    };
    let header_size = if size32 == 1 { 16 } else { 8 };
    if size < header_size || position.checked_add(size).is_none_or(|stop| stop > length) {
        return Err(invalid("malformed top-level MP4 atom extent"));
    }
    Ok((name, size))
}

fn integer_ticks(seconds: f64, timescale: u32) -> Option<u32> {
    let ticks = seconds * f64::from(timescale);
    let rounded = ticks.round();
    if !rounded.is_finite()
        || rounded < 0.0
        || rounded > f64::from(u32::MAX)
        || (ticks - rounded).abs() > 1.0e-6
    {
        return None;
    }
    Some(rounded as u32)
}

/// Return true only if the MP4 was shortened to the representable authored end.
/// A duration finer than its stream or movie timescale cannot be represented
/// exactly; leave the previously valid CFR output untouched in that case.
pub(super) fn finish_mp4_source_end(
    path: &Path,
    rate: FrameRate,
    frames: u64,
    authored_end: f64,
) -> io::Result<bool> {
    if !authored_end.is_finite() || authored_end < 0.0 || frames == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid authored movie endpoint",
        ));
    }
    let Some(media_ticks) = integer_ticks(authored_end, rate.numerator()) else {
        return Ok(false);
    };
    let frames_u32 = u32::try_from(frames)
        .map_err(|_| invalid("MP4 frame count exceeds version-zero sample table"))?;
    let last_start = u64::from(frames_u32 - 1) * u64::from(rate.denominator());
    let full_end = u64::from(frames_u32) * u64::from(rate.denominator());
    let full_end_u32 = u32::try_from(full_end)
        .map_err(|_| invalid("MP4 media duration exceeds version-zero field"))?;
    if u64::from(media_ticks) == full_end {
        return Ok(false);
    }
    if u64::from(media_ticks) <= last_start || u64::from(media_ticks) > full_end {
        return Err(invalid(
            "authored endpoint does not follow the last exported PTS",
        ));
    }
    let last_delta = u32::try_from(u64::from(media_ticks) - last_start)
        .map_err(|_| invalid("MP4 final sample duration overflow"))?;
    if frames_u32 < 2 {
        return Ok(false);
    }

    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let length = file.metadata()?.len();
    let (ftyp, ftyp_size) = disk_atom(&mut file, 0, length)?;
    if ftyp != *b"ftyp" {
        return Err(invalid("MP4 must begin with ftyp"));
    }
    let moov_offset = ftyp_size;
    let (moov_name, moov_size) = disk_atom(&mut file, moov_offset, length)?;
    if moov_name != *b"moov" || moov_size > MAX_MOOV_BYTES as u64 {
        return Err(invalid("MP4 must have a bounded faststart moov"));
    }
    let free_offset = moov_offset + moov_size;
    let (free_name, free_size) = disk_atom(&mut file, free_offset, length)?;
    if free_name != *b"free" || free_size != 8 {
        return Err(invalid("MP4 must reserve exactly 8 bytes after moov"));
    }
    let (mdat_name, _) = disk_atom(&mut file, free_offset + 8, length)?;
    if mdat_name != *b"mdat" {
        return Err(invalid("MP4 padding must immediately precede mdat"));
    }
    let mut moov = vec![0_u8; moov_size as usize];
    file.seek(SeekFrom::Start(moov_offset))?;
    file.read_exact(&mut moov)?;

    let (moov_type, root) = atom_at(&moov, 0, moov.len())?;
    if moov_type != *b"moov" || root.header != 8 || root.end != moov.len() {
        return Err(invalid("unsupported MP4 movie atom header"));
    }
    let mvhd = child(&moov, root, *b"mvhd")?;
    let trak = child(&moov, root, *b"trak")?;
    let tkhd = child(&moov, trak, *b"tkhd")?;
    let edts = child(&moov, trak, *b"edts")?;
    let elst = child(&moov, edts, *b"elst")?;
    let mdia = child(&moov, trak, *b"mdia")?;
    let mdhd = child(&moov, mdia, *b"mdhd")?;
    let minf = child(&moov, mdia, *b"minf")?;
    let stbl = child(&moov, minf, *b"stbl")?;
    let stts = child(&moov, stbl, *b"stts")?;
    for atom in [mvhd, tkhd, elst, mdhd, stts] {
        version_zero(&moov, atom)?;
    }
    let movie_scale = u32_at(&moov, mvhd.payload() + 12)?;
    if movie_scale == 0 || u32_at(&moov, mdhd.payload() + 12)? != rate.numerator() {
        return Err(invalid("MP4 track and authored time bases differ"));
    }
    let Some(movie_ticks) = integer_ticks(authored_end, movie_scale) else {
        return Ok(false);
    };
    if u32_at(&moov, elst.payload() + 4)? != 1
        || u32_at(&moov, elst.payload() + 12)? != 0
        || u32_at(&moov, stts.payload() + 4)? != 1
        || u32_at(&moov, stts.payload() + 8)? != frames_u32
        || u32_at(&moov, stts.payload() + 12)? != rate.denominator()
        || u32_at(&moov, mdhd.payload() + 16)? != full_end_u32
        || stts.len() != 24
    {
        return Err(invalid(
            "MP4 timing table does not match exact exported frame grid",
        ));
    }
    // All preflight errors above leave the on-disk file unchanged. From here,
    // modify only the atom metadata, then consume the eight-byte padding.
    set_u32(&mut moov, mvhd.payload() + 16, movie_ticks)?;
    set_u32(&mut moov, tkhd.payload() + 20, movie_ticks)?;
    set_u32(&mut moov, elst.payload() + 8, movie_ticks)?;
    set_u32(&mut moov, mdhd.payload() + 16, media_ticks)?;
    set_u32(&mut moov, stts.payload() + 4, 2)?;
    set_u32(&mut moov, stts.payload() + 8, frames_u32 - 1)?;
    let extra = [1_u32.to_be_bytes(), last_delta.to_be_bytes()].concat();
    moov.splice(stts.end..stts.end, extra);
    for atom in [stts, stbl, minf, mdia, trak, root] {
        let expanded = u32::try_from(atom.len() + 8)
            .map_err(|_| invalid("expanded MP4 atom exceeds version-zero length"))?;
        set_u32(&mut moov, atom.start, expanded)?;
    }
    if moov.len() as u64 != moov_size + 8 {
        return Err(invalid("MP4 final sample expansion was inconsistent"));
    }
    // Writes occupy the old moov PLUS old 8-byte free atom, not any MDAT byte.
    file.seek(SeekFrom::Start(moov_offset))?;
    file.write_all(&moov)?;
    file.flush()?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rational_endpoint_is_not_rounded_silently() {
        assert_eq!(integer_ticks(2.0, 30_000), Some(60_000));
        assert_eq!(integer_ticks(2.0, 60_000), Some(120_000));
        assert_eq!(integer_ticks(0.3, 24), None);
        assert_eq!(integer_ticks(f64::NAN, 30_000), None);
    }

    #[test]
    fn rejects_unparseable_atoms_before_any_edit() {
        assert!(atom_at(b"", 0, 0).is_err());
        assert!(atom_at(&[0, 0, 0, 4, b'm', b'o', b'o', b'v'], 0, 8).is_err());
    }
}
