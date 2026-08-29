use std::collections::HashSet;

use noon_core::{Rect, Vec2};
use noon_runtime::SpatialQueryResult;
use serde::{Deserialize, Serialize};

use crate::{ExecutionFrameMirror, ScenePlayer, TransportSlotId};

pub const EXECUTION_VISIBILITY_CHANNEL: &str = "noon.execution.visibility";
pub const EXECUTION_VISIBILITY_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisibilityQueryStats {
    pub cells_visited: usize,
    pub candidates_tested: usize,
    pub results: usize,
    pub full_scan_fallbacks: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExecutionVisibilityEnvelope {
    pub channel: String,
    pub protocol_version: u32,
    pub time: f64,
    pub layout_generation: u64,
    pub total_live: usize,
    pub slots: Vec<TransportSlotId>,
    pub stats: VisibilityQueryStats,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionVisibilityError {
    Json(String),
    InvalidChannel(String),
    UnsupportedVersion(u32),
    InvalidTime,
    MissingFrame,
    StaleLayout {
        expected: u64,
        actual: u64,
    },
    FrameTimeMismatch {
        expected_bits: u64,
        actual_bits: u64,
    },
    LiveSetMismatch {
        expected: usize,
        actual: usize,
    },
    UnknownSlot(TransportSlotId),
    DuplicateSlot(TransportSlotId),
    InvalidResultCount {
        slots: usize,
        reported: usize,
    },
    ResultsExceedLiveSet {
        results: usize,
        total_live: usize,
    },
    CandidatesBelowResults {
        candidates: usize,
        results: usize,
    },
}

impl std::fmt::Display for ExecutionVisibilityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(message) => formatter.write_str(message),
            Self::InvalidChannel(channel) => {
                write!(formatter, "invalid execution visibility channel {channel:?}")
            }
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported execution visibility version {version}")
            }
            Self::InvalidTime => formatter.write_str("invalid execution visibility time"),
            Self::MissingFrame => {
                formatter.write_str("execution visibility requires a mirrored frame")
            }
            Self::StaleLayout { expected, actual } => write!(
                formatter,
                "stale execution visibility layout generation: expected {expected}, got {actual}",
            ),
            Self::FrameTimeMismatch {
                expected_bits,
                actual_bits,
            } => write!(
                formatter,
                "execution visibility frame time mismatch: expected {}, got {}",
                f64::from_bits(*expected_bits),
                f64::from_bits(*actual_bits),
            ),
            Self::LiveSetMismatch { expected, actual } => write!(
                formatter,
                "execution visibility live-set mismatch: expected {expected}, got {actual}",
            ),
            Self::UnknownSlot(slot) => write!(
                formatter,
                "execution visibility references unknown slot {}:{}",
                slot.slot, slot.generation,
            ),
            Self::DuplicateSlot(slot) => write!(
                formatter,
                "duplicate execution visibility slot {}:{}",
                slot.slot, slot.generation,
            ),
            Self::InvalidResultCount { slots, reported } => write!(
                formatter,
                "execution visibility reported {reported} results for {slots} slots",
            ),
            Self::ResultsExceedLiveSet {
                results,
                total_live,
            } => write!(
                formatter,
                "execution visibility returned {results} results from {total_live} live objects",
            ),
            Self::CandidatesBelowResults {
                candidates,
                results,
            } => write!(
                formatter,
                "execution visibility tested {candidates} candidates but returned {results} results",
            ),
        }
    }
}

impl std::error::Error for ExecutionVisibilityError {}

impl ExecutionVisibilityEnvelope {
    pub fn from_query(
        time: f64,
        layout_generation: u64,
        total_live: usize,
        query: SpatialQueryResult,
    ) -> Self {
        let stats = query.stats();
        Self {
            channel: EXECUTION_VISIBILITY_CHANNEL.to_owned(),
            protocol_version: EXECUTION_VISIBILITY_VERSION,
            time,
            layout_generation,
            total_live,
            slots: query
                .slots()
                .iter()
                .copied()
                .map(TransportSlotId::from)
                .collect(),
            stats: VisibilityQueryStats {
                cells_visited: stats.cells_visited,
                candidates_tested: stats.candidates_tested,
                results: stats.results,
                full_scan_fallbacks: stats.full_scan_fallbacks,
            },
        }
    }

    pub fn to_json(&self) -> Result<String, ExecutionVisibilityError> {
        self.validate()?;
        serde_json::to_string(self)
            .map_err(|error| ExecutionVisibilityError::Json(error.to_string()))
    }

    pub fn from_json(json: &str) -> Result<Self, ExecutionVisibilityError> {
        let envelope: Self = serde_json::from_str(json)
            .map_err(|error| ExecutionVisibilityError::Json(error.to_string()))?;
        envelope.validate()?;
        Ok(envelope)
    }

    pub fn validate_layout_generation(
        &self,
        expected: u64,
    ) -> Result<(), ExecutionVisibilityError> {
        if self.layout_generation == expected {
            Ok(())
        } else {
            Err(ExecutionVisibilityError::StaleLayout {
                expected,
                actual: self.layout_generation,
            })
        }
    }

    pub fn resolve_for_mirror(
        &self,
        mirror: &ExecutionFrameMirror,
    ) -> Result<Vec<usize>, ExecutionVisibilityError> {
        self.validate()?;
        self.validate_layout_generation(mirror.layout_generation())?;
        let frame = mirror
            .frame()
            .ok_or(ExecutionVisibilityError::MissingFrame)?;
        if self.time.to_bits() != frame.time.to_bits() {
            return Err(ExecutionVisibilityError::FrameTimeMismatch {
                expected_bits: frame.time.to_bits(),
                actual_bits: self.time.to_bits(),
            });
        }
        if self.total_live != mirror.live_object_count() {
            return Err(ExecutionVisibilityError::LiveSetMismatch {
                expected: mirror.live_object_count(),
                actual: self.total_live,
            });
        }

        self.slots
            .iter()
            .copied()
            .map(|slot| {
                mirror
                    .frame_index_for_slot(slot)
                    .ok_or(ExecutionVisibilityError::UnknownSlot(slot))
            })
            .collect()
    }

    pub fn validate(&self) -> Result<(), ExecutionVisibilityError> {
        if self.channel != EXECUTION_VISIBILITY_CHANNEL {
            return Err(ExecutionVisibilityError::InvalidChannel(
                self.channel.clone(),
            ));
        }
        if self.protocol_version != EXECUTION_VISIBILITY_VERSION {
            return Err(ExecutionVisibilityError::UnsupportedVersion(
                self.protocol_version,
            ));
        }
        if !self.time.is_finite() {
            return Err(ExecutionVisibilityError::InvalidTime);
        }
        if self.stats.results != self.slots.len() {
            return Err(ExecutionVisibilityError::InvalidResultCount {
                slots: self.slots.len(),
                reported: self.stats.results,
            });
        }
        if self.stats.results > self.total_live {
            return Err(ExecutionVisibilityError::ResultsExceedLiveSet {
                results: self.stats.results,
                total_live: self.total_live,
            });
        }
        if self.stats.candidates_tested < self.stats.results {
            return Err(ExecutionVisibilityError::CandidatesBelowResults {
                candidates: self.stats.candidates_tested,
                results: self.stats.results,
            });
        }
        let mut seen = HashSet::with_capacity(self.slots.len());
        for &slot in &self.slots {
            if !seen.insert(slot) {
                return Err(ExecutionVisibilityError::DuplicateSlot(slot));
            }
        }
        Ok(())
    }
}

impl ScenePlayer {
    pub fn viewport_visibility(
        &self,
        min_x: f32,
        min_y: f32,
        max_x: f32,
        max_y: f32,
    ) -> ExecutionVisibilityEnvelope {
        let bounds = Rect::new(Vec2::new(min_x, min_y), Vec2::new(max_x, max_y));
        ExecutionVisibilityEnvelope::from_query(
            self.frame().time,
            self.layout_generation(),
            self.object_count(),
            self.query_viewport(bounds),
        )
    }

    pub fn viewport_visibility_json(
        &self,
        min_x: f32,
        min_y: f32,
        max_x: f32,
        max_y: f32,
    ) -> Result<String, ExecutionVisibilityError> {
        self.viewport_visibility(min_x, min_y, max_x, max_y)
            .to_json()
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, SceneDefinition, Transform2D};
    use noon_ir::encode_scene;

    use super::*;
    use crate::EngineScenePlayer;

    fn overlapping_scene_json() -> String {
        let mut scene = SceneDefinition::new();
        scene.add(GeometryRef::circle(1.0));
        scene.add(GeometryRef::rectangle(1.0, 1.0));
        encode_scene(&scene).unwrap()
    }

    fn player_with_overlapping_objects() -> ScenePlayer {
        ScenePlayer::from_scene_json(&overlapping_scene_json()).unwrap()
    }

    fn mirror_with_overlapping_objects() -> ExecutionFrameMirror {
        let mut engine = EngineScenePlayer::new(&overlapping_scene_json(), 4.0, 17).unwrap();
        let mut mirror = ExecutionFrameMirror::default();
        let initial = engine.initial_delta_json().unwrap();
        mirror.apply_json(&initial).unwrap();
        mirror
    }

    #[test]
    fn visibility_envelope_preserves_retained_painter_order_and_metrics() {
        let mut player = player_with_overlapping_objects();
        player.seek(1.25).unwrap();

        let envelope = player.viewport_visibility(-0.25, -0.25, 0.25, 0.25);

        assert_eq!(envelope.time, 1.25);
        assert_eq!(envelope.layout_generation, 0);
        assert_eq!(envelope.total_live, 2);
        assert_eq!(envelope.slots.len(), 2);
        assert_eq!(envelope.slots[0].slot, 0);
        assert_eq!(envelope.slots[1].slot, 1);
        assert_eq!(envelope.stats.results, 2);
        assert_eq!(envelope.stats.full_scan_fallbacks, 0);
        envelope.validate().unwrap();
    }

    #[test]
    fn visibility_json_round_trip_keeps_generation_safe_identity() {
        let player = player_with_overlapping_objects();
        let json = player
            .viewport_visibility_json(-0.25, -0.25, 0.25, 0.25)
            .unwrap();
        let decoded = ExecutionVisibilityEnvelope::from_json(&json).unwrap();

        assert_eq!(
            decoded.slots[0],
            TransportSlotId {
                slot: 0,
                generation: 0
            }
        );
        assert_eq!(
            decoded.slots[1],
            TransportSlotId {
                slot: 1,
                generation: 0
            }
        );
        decoded.validate_layout_generation(0).unwrap();
        assert!(matches!(
            decoded.validate_layout_generation(1),
            Err(ExecutionVisibilityError::StaleLayout {
                expected: 1,
                actual: 0,
            })
        ));
    }

    #[test]
    fn visibility_resolves_to_ordered_mirror_rows_only_for_exact_frame() {
        let player = player_with_overlapping_objects();
        let mirror = mirror_with_overlapping_objects();
        let envelope = player.viewport_visibility(-0.25, -0.25, 0.25, 0.25);

        assert_eq!(envelope.resolve_for_mirror(&mirror).unwrap(), vec![0, 1]);
    }

    #[test]
    fn visibility_rejects_stale_frame_and_slot_identity() {
        let player = player_with_overlapping_objects();
        let mirror = mirror_with_overlapping_objects();
        let envelope = player.viewport_visibility(-0.25, -0.25, 0.25, 0.25);

        let mut stale_layout = envelope.clone();
        stale_layout.layout_generation += 1;
        assert!(matches!(
            stale_layout.resolve_for_mirror(&mirror),
            Err(ExecutionVisibilityError::StaleLayout { .. })
        ));

        let mut stale_time = envelope.clone();
        stale_time.time = 0.5;
        assert!(matches!(
            stale_time.resolve_for_mirror(&mirror),
            Err(ExecutionVisibilityError::FrameTimeMismatch { .. })
        ));

        let mut stale_slot = envelope.clone();
        stale_slot.slots[0].generation += 1;
        assert!(matches!(
            stale_slot.resolve_for_mirror(&mirror),
            Err(ExecutionVisibilityError::UnknownSlot(_))
        ));

        let mut stale_live_set = envelope;
        stale_live_set.total_live += 1;
        assert!(matches!(
            stale_live_set.resolve_for_mirror(&mirror),
            Err(ExecutionVisibilityError::LiveSetMismatch { .. })
        ));
    }

    #[test]
    fn large_offscreen_scene_reports_bounded_candidates_not_live_scan() {
        let mut scene = SceneDefinition::new();
        for index in 0..10_000 {
            let object = scene.add(GeometryRef::circle(0.1));
            scene.object_mut(object).unwrap().transform = Transform2D {
                translation: Vec2::new(index as f32 * 4.0, 0.0),
                ..Transform2D::IDENTITY
            };
        }
        let json = encode_scene(&scene).unwrap();
        let player = ScenePlayer::from_scene_json(&json).unwrap();

        let envelope = player.viewport_visibility(-0.5, -0.5, 0.5, 0.5);

        assert_eq!(envelope.total_live, 10_000);
        assert_eq!(envelope.stats.full_scan_fallbacks, 0);
        assert!(envelope.stats.results <= 1);
        assert!(envelope.stats.candidates_tested < 16);
    }

    #[test]
    fn malformed_visibility_metadata_is_rejected_transactionally() {
        let player = player_with_overlapping_objects();
        let mut envelope = player.viewport_visibility(-0.25, -0.25, 0.25, 0.25);
        envelope.stats.results += 1;

        assert!(matches!(
            envelope.validate(),
            Err(ExecutionVisibilityError::InvalidResultCount { .. })
        ));
    }
}
