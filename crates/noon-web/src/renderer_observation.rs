use noon::{CallbackRendererDirtyClassification, CommittedCallbackRendererObservation};
use noon_core::{ObjectId, Style, Transform2D};
use serde::{Deserialize, Serialize};

use crate::TransportSlotId;

pub const RENDERER_OBSERVATION_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RendererDirtyClassification {
    All,
    Added,
    Updated,
    Removed,
    Unchanged,
}

impl From<CallbackRendererDirtyClassification> for RendererDirtyClassification {
    fn from(value: CallbackRendererDirtyClassification) -> Self {
        match value {
            CallbackRendererDirtyClassification::All => Self::All,
            CallbackRendererDirtyClassification::Added => Self::Added,
            CallbackRendererDirtyClassification::Updated => Self::Updated,
            CallbackRendererDirtyClassification::Removed => Self::Removed,
            CallbackRendererDirtyClassification::Unchanged => Self::Unchanged,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RendererObservationPublication {
    pub session: u32,
    pub sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RendererCommittedObjectObservation {
    pub runtime: String,
    pub callback_sequence: String,
    pub scene_revision: String,
    pub execution_revision: String,
    pub frame_epoch: String,
    pub semantic_slot: u32,
    pub semantic_generation: u32,
    pub object: ObjectId,
    pub frame_index: usize,
    pub time: f64,
    pub transform: Transform2D,
    pub style: Style,
    pub presence: bool,
    pub dirty: RendererDirtyClassification,
}

impl From<CommittedCallbackRendererObservation> for RendererCommittedObjectObservation {
    fn from(value: CommittedCallbackRendererObservation) -> Self {
        let token = value.token();
        let publication = value.publication();
        let target = value.target();
        Self {
            runtime: token.runtime().get().to_string(),
            callback_sequence: token.sequence().get().to_string(),
            scene_revision: publication.scene_revision().get().to_string(),
            execution_revision: publication.execution_revision().get().to_string(),
            frame_epoch: publication.frame_epoch().get().to_string(),
            semantic_slot: target.slot(),
            semantic_generation: target.generation(),
            object: value.execution_object(),
            frame_index: value.frame_index(),
            time: value.time(),
            transform: value.transform(),
            style: value.style(),
            presence: value.presence(),
            dirty: value.dirty().into(),
        }
    }
}

/// One opt-in target pinned to the exact retained transport publication that
/// carries its callback-published runtime row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RendererObservationRequest {
    pub schema_version: u32,
    pub publication: RendererObservationPublication,
    pub slot: TransportSlotId,
    pub committed: RendererCommittedObjectObservation,
}

impl RendererObservationRequest {
    pub fn from_callback_publication(
        transport_session: u32,
        transport_sequence: u64,
        committed: CommittedCallbackRendererObservation,
    ) -> Self {
        Self {
            schema_version: RENDERER_OBSERVATION_VERSION,
            publication: RendererObservationPublication {
                session: transport_session,
                sequence: transport_sequence,
            },
            slot: committed.execution_slot().into(),
            committed: committed.into(),
        }
    }
}

#[cfg(any(all(feature = "renderer", target_arch = "wasm32"), test))]
mod retained;
#[cfg(any(all(feature = "renderer", target_arch = "wasm32"), test))]
pub use retained::*;
