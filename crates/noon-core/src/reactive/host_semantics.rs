use serde::{Deserialize, Serialize};

/// How much execution history a host callback needs for an exact seek.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostCallbackReplayClass {
    /// Output is a pure function of the coherent current-frame input state.
    Pure,
    /// The callback owns deterministic state that can be restored from an engine
    /// checkpoint and replayed forward.
    StatefulDeterministic,
    /// The callback may depend on opaque Python/JS state, I/O, randomness, wall
    /// time, or other state the engine cannot checkpoint safely.
    #[default]
    Opaque,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostSeekMode {
    Direct,
    ReplayFromCheckpoint,
    ReplayFromInitialization,
}

impl HostCallbackReplayClass {
    pub const fn seek_mode(self) -> HostSeekMode {
        match self {
            Self::Pure => HostSeekMode::Direct,
            Self::StatefulDeterministic => HostSeekMode::ReplayFromCheckpoint,
            Self::Opaque => HostSeekMode::ReplayFromInitialization,
        }
    }
}

/// Whether presentation is allowed to wait for arbitrary host-language code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostPlaybackMode {
    /// Presentation is deadline-driven. Missed callbacks may commit later, but
    /// they must not synchronously stall the presenter.
    #[default]
    Realtime,
    /// Deterministic/offline evaluation prioritizes exact same-frame host results
    /// and therefore waits for the callback phase to complete.
    DeterministicOffline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostFrameDisposition {
    PresentLatestCommitted,
    WaitForHostCommit,
}

impl HostPlaybackMode {
    pub const fn frame_disposition(self) -> HostFrameDisposition {
        match self {
            Self::Realtime => HostFrameDisposition::PresentLatestCommitted,
            Self::DeterministicOffline => HostFrameDisposition::WaitForHostCommit,
        }
    }
}

/// Read semantics available to an arbitrary host callback.
///
/// Snapshot-only execution remains useful for traced/declared callbacks, but it
/// is not sufficient for unrestricted Manim-style Python because a closure may
/// synchronously inspect any object/tracker/global reachable from Python.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostReadModel {
    DeclaredSnapshot,
    #[default]
    EngineLocalSemanticView,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_class_maps_to_exact_seek_requirement() {
        assert_eq!(
            HostCallbackReplayClass::Pure.seek_mode(),
            HostSeekMode::Direct
        );
        assert_eq!(
            HostCallbackReplayClass::StatefulDeterministic.seek_mode(),
            HostSeekMode::ReplayFromCheckpoint
        );
        assert_eq!(
            HostCallbackReplayClass::Opaque.seek_mode(),
            HostSeekMode::ReplayFromInitialization
        );
    }

    #[test]
    fn realtime_never_requires_presentation_to_wait_for_host() {
        assert_eq!(
            HostPlaybackMode::Realtime.frame_disposition(),
            HostFrameDisposition::PresentLatestCommitted
        );
        assert_eq!(
            HostPlaybackMode::DeterministicOffline.frame_disposition(),
            HostFrameDisposition::WaitForHostCommit
        );
    }
}
