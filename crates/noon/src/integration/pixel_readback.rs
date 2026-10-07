//! Checked storage and byte normalization for four-channel, eight-bit readback.
//!
//! This is a pixel payload utility, not a renderer or a GPU host. The caller owns
//! texture format selection, submission, mapping, synchronization and unmapping.
//! Only call it with completed, readable bytes from the intended publication.
//! No color transfer, alpha conversion, compositing or time evaluation occurs.

use std::{collections::TryReserveError, error::Error, fmt};

/// Channel order in the mapped source. Output is always RGBA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelChannelOrder {
    Rgba,
    Bgra,
}

/// Row order in the mapped source. Output is always top-to-bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelRowOrder {
    TopToBottom,
    BottomToTop,
}

/// One bounded, padded buffer layout. All sizes are validated before allocation.
///
/// `row_alignment` is supplied by the host (for example, its GPU copy alignment).
/// The source buffer contains `height` full padded rows, including the last row.
/// `max_buffer_bytes` bounds each buffer, not aggregate GPU/CPU/in-flight storage;
/// a host must separately bound the number of retained buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba8ReadbackLayout {
    width: u32,
    height: u32,
    bytes_per_row: u32,
    padded_bytes_per_row: u32,
    buffer_len: usize,
    packed_len: usize,
}

impl Rgba8ReadbackLayout {
    pub fn new(
        width: u32,
        height: u32,
        row_alignment: u32,
        max_buffer_bytes: u64,
    ) -> Result<Self, PixelReadbackError> {
        if width == 0 || height == 0 {
            return Err(PixelReadbackError::InvalidDimensions);
        }
        if !row_alignment.is_power_of_two() {
            return Err(PixelReadbackError::InvalidAlignment);
        }
        let bytes_per_row = width
            .checked_mul(4)
            .ok_or(PixelReadbackError::SizeOverflow)?;
        let padded_bytes_per_row = bytes_per_row
            .checked_add(row_alignment - 1)
            .ok_or(PixelReadbackError::SizeOverflow)?
            & !(row_alignment - 1);
        let buffer_bytes = u64::from(padded_bytes_per_row) * u64::from(height);
        if buffer_bytes > max_buffer_bytes {
            return Err(PixelReadbackError::BufferLimit {
                requested: buffer_bytes,
                limit: max_buffer_bytes,
            });
        }
        let buffer_len = usize::try_from(buffer_bytes)
            .ok()
            .filter(|&len| len <= isize::MAX as usize)
            .ok_or(PixelReadbackError::SizeOverflow)?;
        // The packed allocation cannot exceed the already checked padded one.
        let packed_len = usize::try_from(u64::from(bytes_per_row) * u64::from(height))
            .map_err(|_| PixelReadbackError::SizeOverflow)?;
        Ok(Self {
            width,
            height,
            bytes_per_row,
            padded_bytes_per_row,
            buffer_len,
            packed_len,
        })
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }

    pub const fn bytes_per_row(self) -> u32 {
        self.bytes_per_row
    }

    pub const fn padded_bytes_per_row(self) -> u32 {
        self.padded_bytes_per_row
    }

    pub const fn buffer_len(self) -> usize {
        self.buffer_len
    }

    pub const fn packed_len(self) -> usize {
        self.packed_len
    }

    /// Copy into caller-owned storage, suitable for a bounded reusable pool.
    ///
    /// Both lengths are checked before any destination byte is changed. Padding
    /// is never copied. Color and alpha bytes are preserved without interpretation:
    /// an UNORM format alone does not identify the renderer's color transfer.
    pub fn copy_rgba8_into(
        self,
        source: &[u8],
        channels: PixelChannelOrder,
        rows: PixelRowOrder,
        destination: &mut [u8],
    ) -> Result<(), PixelReadbackError> {
        self.validate_source(source)?;
        if destination.len() != self.packed_len {
            return Err(PixelReadbackError::DestinationLength {
                expected: self.packed_len,
                actual: destination.len(),
            });
        }
        let stride = self.padded_bytes_per_row as usize;
        let row_bytes = self.bytes_per_row as usize;
        for (y, output) in destination.chunks_exact_mut(row_bytes).enumerate() {
            let source_y = match rows {
                PixelRowOrder::TopToBottom => y,
                PixelRowOrder::BottomToTop => self.height as usize - 1 - y,
            };
            let start = source_y * stride;
            output.copy_from_slice(&source[start..start + row_bytes]);
            if channels == PixelChannelOrder::Bgra {
                for pixel in output.as_chunks_mut::<4>().0 {
                    pixel.swap(0, 2);
                }
            }
        }
        Ok(())
    }

    /// Produce an independent, tightly packed RGBA payload. The returned storage
    /// has no borrow of the mapped GPU buffer; the host may then unmap/reuse it.
    /// This allocation is fallible. Pool-based hosts can use `copy_rgba8_into`.
    pub fn copy_rgba8(
        self,
        source: &[u8],
        channels: PixelChannelOrder,
        rows: PixelRowOrder,
    ) -> Result<Vec<u8>, PixelReadbackError> {
        self.validate_source(source)?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(self.packed_len)
            .map_err(PixelReadbackError::Allocation)?;
        pixels.resize(self.packed_len, 0);
        self.copy_rgba8_into(source, channels, rows, &mut pixels)?;
        Ok(pixels)
    }

    fn validate_source(self, source: &[u8]) -> Result<(), PixelReadbackError> {
        if source.len() != self.buffer_len {
            return Err(PixelReadbackError::SourceLength {
                expected: self.buffer_len,
                actual: source.len(),
            });
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum PixelReadbackError {
    InvalidDimensions,
    InvalidAlignment,
    SizeOverflow,
    BufferLimit { requested: u64, limit: u64 },
    SourceLength { expected: usize, actual: usize },
    DestinationLength { expected: usize, actual: usize },
    Allocation(TryReserveError),
}

impl fmt::Display for PixelReadbackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDimensions => f.write_str("readback dimensions must be positive"),
            Self::InvalidAlignment => f.write_str("row alignment must be a positive power of two"),
            Self::SizeOverflow => {
                f.write_str("readback size exceeds the representable buffer size")
            }
            Self::BufferLimit { requested, limit } => {
                write!(
                    f,
                    "readback needs {requested} bytes, exceeding the {limit}-byte limit"
                )
            }
            Self::SourceLength { expected, actual } => {
                write!(f, "mapped source has {actual} bytes; expected {expected}")
            }
            Self::DestinationLength { expected, actual } => {
                write!(
                    f,
                    "packed destination has {actual} bytes; expected {expected}"
                )
            }
            Self::Allocation(error) => error.fmt(f),
        }
    }
}

impl Error for PixelReadbackError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Allocation(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
