//! Versioned image identity and sampling on the genuine worker wire.
use noon_core::{RasterImageResourceHandle, RasterImageResourceId, RasterImageSampling};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TransportImageResourceHandle {
    pub arena: u64,
    pub id: u64,
    pub version: u64,
}
impl From<RasterImageResourceHandle> for TransportImageResourceHandle {
    fn from(handle: RasterImageResourceHandle) -> Self {
        Self {
            arena: handle.arena,
            id: handle.id.get(),
            version: handle.version,
        }
    }
}
impl From<TransportImageResourceHandle> for RasterImageResourceHandle {
    fn from(handle: TransportImageResourceHandle) -> Self {
        Self {
            arena: handle.arena,
            id: RasterImageResourceId::new(handle.id),
            version: handle.version,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportImageSampling {
    Nearest,
    Linear,
    Bicubic,
}
impl From<RasterImageSampling> for TransportImageSampling {
    fn from(value: RasterImageSampling) -> Self {
        match value {
            RasterImageSampling::Nearest => Self::Nearest,
            RasterImageSampling::Linear => Self::Linear,
            RasterImageSampling::Bicubic => Self::Bicubic,
        }
    }
}
impl From<TransportImageSampling> for RasterImageSampling {
    fn from(value: TransportImageSampling) -> Self {
        match value {
            TransportImageSampling::Nearest => Self::Nearest,
            TransportImageSampling::Linear => Self::Linear,
            TransportImageSampling::Bicubic => Self::Bicubic,
        }
    }
}
