//! Immutable, explicitly sampled rigid motion. These values own no clock.

use std::{fmt, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{DVec2, SemanticVec3, Transform2D, Vec2};

/// Admission bound for one authored sample table, independent of scene size.
pub const MAX_SAMPLED_MOTION_KEYS: usize = 65_536;

/// Destination keys for a piecewise-linear center and unwrapped angular offset.
///
/// Phases are strictly increasing in `(0, 1]`, ending at exactly one. Phase zero
/// is deliberately absent: activation captures the effective center and pose.
/// Positions place that captured center as a fixed rigid anchor. They do not
/// recompute the bounding-box center of an asymmetric shape during rotation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "MotionInput")]
pub struct SampledMotionData {
    phases: Box<[f64]>,
    centers: Box<[SemanticVec3]>,
    rotation_offsets: Box<[f64]>,
}

#[derive(Deserialize)]
struct MotionInput {
    phases: Vec<f64>,
    centers: Vec<SemanticVec3>,
    rotation_offsets: Vec<f64>,
}

impl TryFrom<MotionInput> for SampledMotionData {
    type Error = SampledMotionError;
    fn try_from(value: MotionInput) -> Result<Self, Self::Error> {
        Self::new(value.phases, value.centers, value.rotation_offsets)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampledMotionError {
    InvalidCardinality,
    InvalidPhases,
    NonFinite,
    NonPlanar,
    UnrepresentablePose,
}

impl fmt::Display for SampledMotionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidCardinality => {
                "sampled motion requires equal nonempty arrays of at most 65536 destination keys"
            }
            Self::InvalidPhases => {
                "sampled motion phases must increase strictly within (0, 1] and end at exactly 1"
            }
            Self::NonFinite => "sampled motion positions and angular offsets must be finite",
            Self::NonPlanar => "sampled planar motion requires zero destination z coordinates",
            Self::UnrepresentablePose => {
                "sampled motion activation or output exceeds the finite planar transform range"
            }
        })
    }
}

impl std::error::Error for SampledMotionError {}

impl SampledMotionData {
    pub fn new(
        phases: Vec<f64>,
        centers: Vec<SemanticVec3>,
        rotation_offsets: Vec<f64>,
    ) -> Result<Self, SampledMotionError> {
        if phases.is_empty()
            || phases.len() > MAX_SAMPLED_MOTION_KEYS
            || centers.len() != phases.len()
            || rotation_offsets.len() != phases.len()
        {
            return Err(SampledMotionError::InvalidCardinality);
        }
        let mut previous = 0.0;
        for &phase in &phases {
            if !phase.is_finite() || phase <= previous || phase > 1.0 {
                return Err(SampledMotionError::InvalidPhases);
            }
            previous = phase;
        }
        if previous != 1.0 {
            return Err(SampledMotionError::InvalidPhases);
        }
        if centers
            .iter()
            .any(|point| ![point.x, point.y, point.z].into_iter().all(f64::is_finite))
            || rotation_offsets.iter().any(|angle| !angle.is_finite())
        {
            return Err(SampledMotionError::NonFinite);
        }
        Ok(Self {
            phases: phases.into_boxed_slice(),
            centers: centers.into_boxed_slice(),
            rotation_offsets: rotation_offsets.into_boxed_slice(),
        })
    }

    pub fn phases(&self) -> &[f64] {
        &self.phases
    }
    pub fn centers(&self) -> &[SemanticVec3] {
        &self.centers
    }
    pub fn rotation_offsets(&self) -> &[f64] {
        &self.rotation_offsets
    }
    pub fn len(&self) -> usize {
        self.phases.len()
    }
    pub fn is_empty(&self) -> bool {
        self.phases.is_empty()
    }

    /// Logical immutable allocation bytes, excluding allocator overhead.
    pub fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.len() * (2 * std::mem::size_of::<f64>() + std::mem::size_of::<SemanticVec3>())
    }

    fn sample(&self, phase: f64, source_center: Vec2) -> (f64, f64, f64) {
        let next = self
            .phases
            .partition_point(|&key| key < phase)
            .min(self.len() - 1);
        let (start, x, y, rotation) = if next == 0 {
            (
                0.0,
                f64::from(source_center.x),
                f64::from(source_center.y),
                0.0,
            )
        } else {
            let previous = self.centers[next - 1];
            (
                self.phases[next - 1],
                previous.x,
                previous.y,
                self.rotation_offsets[next - 1],
            )
        };
        let alpha = ((phase - start) / (self.phases[next] - start)).clamp(0.0, 1.0);
        let target = self.centers[next];
        let mix = |from: f64, to: f64| from * (1.0 - alpha) + to * alpha;
        (
            mix(x, target.x),
            mix(y, target.y),
            mix(rotation, self.rotation_offsets[next]),
        )
    }
}

/// One activation snapshot shared by the ordinary Position and Rotation tracks.
/// Geometry and scale remain owned by the existing execution row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "PlanInput")]
pub struct SampledMotionPlan {
    motion: Arc<SampledMotionData>,
    source_transform: Transform2D,
    source_center: Vec2,
}

#[derive(Deserialize)]
struct PlanInput {
    motion: Arc<SampledMotionData>,
    source_transform: Transform2D,
    source_center: Vec2,
}

impl TryFrom<PlanInput> for SampledMotionPlan {
    type Error = SampledMotionError;
    fn try_from(value: PlanInput) -> Result<Self, Self::Error> {
        Self::new(value.motion, value.source_transform, value.source_center)
    }
}

impl SampledMotionPlan {
    pub fn new(
        motion: Arc<SampledMotionData>,
        source_transform: Transform2D,
        source_center: Vec2,
    ) -> Result<Self, SampledMotionError> {
        if ![
            source_transform.translation.x,
            source_transform.translation.y,
            source_transform.rotation,
            source_transform.scale.x,
            source_transform.scale.y,
            source_center.x,
            source_center.y,
        ]
        .into_iter()
        .all(f32::is_finite)
        {
            return Err(SampledMotionError::UnrepresentablePose);
        }
        let offset_x = f64::from(source_center.x) - f64::from(source_transform.translation.x);
        let offset_y = f64::from(source_center.y) - f64::from(source_transform.translation.y);
        let radius = offset_x.hypot(offset_y);
        let limit = f64::from(f32::MAX);
        for (point, angle) in motion.centers().iter().zip(motion.rotation_offsets()) {
            if point.z != 0.0 {
                return Err(SampledMotionError::NonPlanar);
            }
            if point.x.abs() + radius > limit
                || point.y.abs() + radius > limit
                || (f64::from(source_transform.rotation) + angle).abs() > limit
            {
                return Err(SampledMotionError::UnrepresentablePose);
            }
        }
        if f64::from(source_center.x).abs() + radius > limit
            || f64::from(source_center.y).abs() + radius > limit
        {
            return Err(SampledMotionError::UnrepresentablePose);
        }
        Ok(Self {
            motion,
            source_transform,
            source_center,
        })
    }

    pub fn motion(&self) -> &Arc<SampledMotionData> {
        &self.motion
    }
    pub const fn source_transform(&self) -> Transform2D {
        self.source_transform
    }
    pub const fn source_center(&self) -> Vec2 {
        self.source_center
    }

    /// Evaluate the existing timeline's mapped phase; this does not advance time.
    /// Only the execution row lowers the shared f64 sample to render precision.
    pub fn sample(&self, phase: f64) -> (Vec2, f32) {
        let (translation, rotation) = self.sample_f64(phase);
        (
            Vec2::new(translation.x as f32, translation.y as f32),
            rotation as f32,
        )
    }

    /// Preserve authored destination precision for semantic completion. The
    /// captured source still has the precision of the effective execution row.
    pub fn sample_f64(&self, phase: f64) -> (DVec2, f64) {
        if phase <= 0.0 {
            return (
                self.source_transform.translation.into(),
                f64::from(self.source_transform.rotation),
            );
        }
        let (x, y, angle) = self
            .motion
            .sample(phase.clamp(0.0, 1.0), self.source_center);
        let (sin, cos) = angle.sin_cos();
        let dx = f64::from(self.source_center.x) - f64::from(self.source_transform.translation.x);
        let dy = f64::from(self.source_center.y) - f64::from(self.source_transform.translation.y);
        (
            DVec2::new(x - (cos * dx - sin * dy), y - (sin * dx + cos * dy)),
            f64::from(self.source_transform.rotation) + angle,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(phases: Vec<f64>) -> Result<SampledMotionData, SampledMotionError> {
        let n = phases.len();
        SampledMotionData::new(phases, vec![SemanticVec3::ZERO; n], vec![0.0; n])
    }

    #[test]
    fn sampled_motion_validates_complete_destination_tables() {
        for phases in [
            vec![],
            vec![0.0, 1.0],
            vec![0.5, 0.5, 1.0],
            vec![0.7, 0.4, 1.0],
            vec![0.5],
            vec![f64::NAN, 1.0],
            vec![1.1],
        ] {
            assert!(data(phases).is_err());
        }
        assert!(data(vec![1.0; MAX_SAMPLED_MOTION_KEYS + 1]).is_err());
        assert_eq!(
            SampledMotionData::new(vec![1.0], vec![], vec![0.0]),
            Err(SampledMotionError::InvalidCardinality)
        );
        assert_eq!(
            SampledMotionData::new(
                vec![1.0],
                vec![SemanticVec3::new(f64::INFINITY, 0.0, 0.0)],
                vec![0.0]
            ),
            Err(SampledMotionError::NonFinite)
        );
        assert_eq!(
            SampledMotionData::new(vec![1.0], vec![SemanticVec3::ZERO], vec![f64::NAN]),
            Err(SampledMotionError::NonFinite)
        );
        assert!(data(vec![0.1, 0.1000000001, 1.0]).is_ok());
    }

    #[test]
    fn sampled_motion_preserves_f64_nondyadic_keys_and_captured_zero() {
        let motion = Arc::new(
            SampledMotionData::new(
                vec![0.1, 0.1000000001, 1.0],
                vec![
                    SemanticVec3::new(1.0, 2.0, 0.0),
                    SemanticVec3::new(3.0, 4.0, 0.0),
                    SemanticVec3::new(9.0, 8.0, 0.0),
                ],
                vec![0.0, 2.0, 4.0],
            )
            .unwrap(),
        );
        let source = Transform2D {
            translation: Vec2::new(-3.0, -2.0),
            rotation: 0.25,
            scale: Vec2::new(2.0, -3.0),
        };
        let plan = SampledMotionPlan::new(motion, source, source.translation).unwrap();
        assert_eq!(plan.sample(0.0), (source.translation, source.rotation));
        assert_eq!(plan.sample(0.1), (Vec2::new(1.0, 2.0), 0.25));
        let (point, rotation) = plan.sample(0.10000000005);
        assert!((point.x - 2.0).abs() < 1e-5, "{point:?}");
        assert!((point.y - 3.0).abs() < 1e-5);
        assert!((rotation - 1.25).abs() < 1e-5);
        assert_eq!(plan.sample(1.0), (Vec2::new(9.0, 8.0), 4.25));
    }

    #[test]
    fn sampled_motion_moves_captured_anchor_with_signed_unwrapped_rotation() {
        let turns = -7.0 * std::f64::consts::TAU;
        let motion = Arc::new(
            SampledMotionData::new(
                vec![1.0],
                vec![SemanticVec3::new(10.0, -4.0, 0.0)],
                vec![turns],
            )
            .unwrap(),
        );
        let source = Transform2D {
            translation: Vec2::new(2.0, 3.0),
            rotation: 0.25,
            scale: Vec2::new(2.0, -3.0),
        };
        let center = Vec2::new(6.0, 8.0);
        let plan = SampledMotionPlan::new(motion, source, center).unwrap();
        for phase in [0.0, 0.1, 0.25, 0.5, 0.75, 1.0] {
            let (translation, rotation) = plan.sample(phase);
            let delta = turns * phase;
            let (sin, cos) = delta.sin_cos();
            let anchor_x = f64::from(translation.x) + cos * 4.0 - sin * 5.0;
            let anchor_y = f64::from(translation.y) + sin * 4.0 + cos * 5.0;
            assert!((anchor_x - (6.0 + 4.0 * phase)).abs() < 2e-6);
            assert!((anchor_y - (8.0 - 12.0 * phase)).abs() < 2e-6);
            assert!((f64::from(rotation) - (0.25 + delta)).abs() < 2e-6);
        }
        assert_eq!(plan.source_transform().scale, source.scale);
    }

    #[test]
    fn sampled_plan_rejects_nonplanar_and_unrepresentable_output() {
        let make = |point, angle| {
            Arc::new(SampledMotionData::new(vec![1.0], vec![point], vec![angle]).unwrap())
        };
        assert_eq!(
            SampledMotionPlan::new(
                make(SemanticVec3::new(0.0, 0.0, 1.0), 0.0),
                Transform2D::IDENTITY,
                Vec2::ZERO
            ),
            Err(SampledMotionError::NonPlanar)
        );
        assert!(SampledMotionPlan::new(
            make(SemanticVec3::new(f64::MAX, 0.0, 0.0), 0.0),
            Transform2D::IDENTITY,
            Vec2::ZERO
        )
        .is_err());
        assert!(SampledMotionPlan::new(
            make(SemanticVec3::ZERO, f64::MAX),
            Transform2D::IDENTITY,
            Vec2::ZERO
        )
        .is_err());
    }

    #[test]
    fn sampled_motion_options_preserve_precedence_and_reverse_without_arc_or_lag() {
        use crate::{
            resolve_sampled_motion_animation_options as resolve, AnimationDefaults,
            AnimationOptions, RateFunction,
        };
        let local = AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear)
            .reverse_rate_function(true);
        let value = resolve(
            AnimationDefaults::MANIM,
            local,
            AnimationOptions::new().run_time(3.0),
        )
        .unwrap();
        assert_eq!(value.run_time, 3.0);
        assert_eq!(value.rate_func, RateFunction::Linear);
        assert!(value.reverse_rate_function);
        for invalid in [
            AnimationOptions::new().path_arc(1e-14),
            AnimationOptions::new().lag_ratio(0.1),
            AnimationOptions::new().run_time(0.0),
        ] {
            assert!(resolve(AnimationDefaults::MANIM, local, invalid).is_err());
        }
    }
}
