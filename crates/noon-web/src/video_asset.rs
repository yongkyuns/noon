//! Immutable media descriptors owned by the browser's codec host.

use std::{io, sync::Arc};

pub(crate) mod readiness;

use noon_core::{VideoFrameIndex, VideoFrameSource, VideoFrameWindow};

#[derive(Clone, Debug)]
pub(crate) struct BrowserVideoSource {
    id: u32,
    dimensions: (u32, u32),
    index: VideoFrameIndex,
}

impl BrowserVideoSource {
    pub(crate) fn new(
        id: u32,
        width: u32,
        height: u32,
        timestamps: Vec<f64>,
        duration: f64,
    ) -> Result<Self, String> {
        if id == 0
            || width == 0
            || height == 0
            || width.max(height) > 1920
            || u64::from(width) * u64::from(height) > 1920 * 1080
            || timestamps.len() > 7200
            || duration > 60.0
        {
            return Err("invalid browser video source identifier, dimensions, or duration".into());
        }
        Ok(Self {
            id,
            dimensions: (width, height),
            index: VideoFrameIndex::new(timestamps, duration).map_err(|error| error.to_string())?,
        })
    }
}

impl VideoFrameSource for BrowserVideoSource {
    fn dimensions(&self) -> (u32, u32) {
        self.dimensions
    }

    fn frame_index(&self) -> &VideoFrameIndex {
        &self.index
    }

    fn external_asset_id(&self) -> Option<u32> {
        Some(self.id)
    }

    fn open_window(
        self: Arc<Self>,
        _: Box<dyn Fn() + Send>,
    ) -> io::Result<Box<dyn VideoFrameWindow>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "browser video frames are supplied asynchronously by the codec host",
        ))
    }
}
