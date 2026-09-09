//! Derived slot identity shared by genuine cross-worker retained transports.
use noon_runtime::ExecutionSlotId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransportSlotId {
    pub slot: u32,
    pub generation: u32,
}

impl From<ExecutionSlotId> for TransportSlotId {
    fn from(value: ExecutionSlotId) -> Self {
        Self {
            slot: value.slot(),
            generation: value.generation(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportApplyOutcome {
    Applied,
    DroppedStale,
}
