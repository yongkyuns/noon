use serde::{Deserialize, Serialize};

/// Ordered phases for one evaluated frame.
///
/// The order is semantic, not an implementation suggestion. A backend may fuse
/// phases internally, but observable writes must be equivalent to this sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameExecutionPhase {
    Timeline,
    NativeDynamic,
    HostCallbacks,
    DerivedState,
    Render,
}

impl FrameExecutionPhase {
    pub const ORDERED: [Self; 5] = [
        Self::Timeline,
        Self::NativeDynamic,
        Self::HostCallbacks,
        Self::DerivedState,
        Self::Render,
    ];
}

/// Property-producing phases that may intentionally target the same property.
///
/// Noon uses ordered composition rather than rejecting the scene globally. The
/// later phase sees the value produced by the earlier phase and wins when both
/// phases write the same property in one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DynamicDriverPhase {
    Timeline,
    NativeDynamic,
    HostCallbacks,
}

impl DynamicDriverPhase {
    pub const ORDERED: [Self; 3] = [Self::Timeline, Self::NativeDynamic, Self::HostCallbacks];

    pub const fn precedence(self) -> u8 {
        match self {
            Self::Timeline => 0,
            Self::NativeDynamic => 1,
            Self::HostCallbacks => 2,
        }
    }

    pub const fn overrides(self, other: Self) -> bool {
        self.precedence() >= other.precedence()
    }
}

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
    fn execution_phase_order_is_explicit_and_stable() {
        assert_eq!(
            FrameExecutionPhase::ORDERED,
            [
                FrameExecutionPhase::Timeline,
                FrameExecutionPhase::NativeDynamic,
                FrameExecutionPhase::HostCallbacks,
                FrameExecutionPhase::DerivedState,
                FrameExecutionPhase::Render,
            ]
        );
    }

    #[test]
    fn later_dynamic_phase_overrides_earlier_phase() {
        assert!(DynamicDriverPhase::HostCallbacks.overrides(DynamicDriverPhase::NativeDynamic));
        assert!(DynamicDriverPhase::NativeDynamic.overrides(DynamicDriverPhase::Timeline));
        assert!(!DynamicDriverPhase::Timeline.overrides(DynamicDriverPhase::HostCallbacks));
    }

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
