//! Analytic camera motion in authored time, consumed by the existing timeline.

use crate::ManimCamera3DProfile;

/// Maximum authored camera-motion intervals per camera. Closed intervals are seek
/// history; bounding them also bounds copy-on-publication metadata retention.
pub const MAX_CAMERA_MOTION_INTERVALS: usize = 256;

/// Manim's unwrapped angular coordinate driven by one ambient occurrence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraRotationAxis {
    Phi,
    Theta,
    Gamma,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum CameraMotionProfile {
    Angular(CameraRotationAxis),
    ThreeDIllusion { origin_phi: f64, origin_theta: f64 },
}

/// One immutable native authored camera-motion interval. It owns no clock or scheduler.
/// `end` is exclusive for driver ownership; sampling at it returns the exact
/// endpoint for release. Closed occurrences are intentional authored seek history.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraAngularMotion {
    source: ManimCamera3DProfile,
    profile: CameraMotionProfile,
    rate: f64,
    start: f64,
    end: Option<f64>,
    near: f64,
    far: f64,
}

impl CameraAngularMotion {
    pub fn new(
        source: ManimCamera3DProfile,
        axis: CameraRotationAxis,
        rate: f64,
        start: f64,
        end: Option<f64>,
        near: f64,
        far: f64,
    ) -> Option<Self> {
        let value = Self {
            source,
            profile: CameraMotionProfile::Angular(axis),
            rate,
            start,
            end,
            near,
            far,
        };
        if !rate.is_finite()
            || !start.is_finite()
            || start < 0.0
            || end.is_some_and(|end| !end.is_finite() || end < start)
            || source.camera(near, far).is_none()
            || end.is_some_and(|end| value.sample(end).is_none())
        {
            return None;
        }
        Some(value)
    }

    /// Build Manim's deterministic 3D-illusion camera interval. The two
    /// oscillations share phase `rate * (time - start)` and the captured
    /// origin defaults to the source profile's orientation.
    #[allow(clippy::too_many_arguments)]
    pub fn three_d_illusion(
        source: ManimCamera3DProfile,
        rate: f64,
        start: f64,
        end: Option<f64>,
        near: f64,
        far: f64,
        origin_phi: Option<f64>,
        origin_theta: Option<f64>,
    ) -> Option<Self> {
        let value = Self {
            source,
            profile: CameraMotionProfile::ThreeDIllusion {
                origin_phi: origin_phi.unwrap_or(source.phi),
                origin_theta: origin_theta.unwrap_or(source.theta),
            },
            rate,
            start,
            end,
            near,
            far,
        };
        let (origin_phi, origin_theta) = match value.profile {
            CameraMotionProfile::ThreeDIllusion {
                origin_phi,
                origin_theta,
            } => (origin_phi, origin_theta),
            CameraMotionProfile::Angular(_) => unreachable!(),
        };
        if !rate.is_finite()
            || !start.is_finite()
            || start < 0.0
            || !origin_phi.is_finite()
            || !origin_theta.is_finite()
            || end.is_some_and(|end| !end.is_finite() || end < start)
            || source.camera(near, far).is_none()
            || end.is_some_and(|end| value.sample(end).is_none())
        {
            return None;
        }
        Some(value)
    }

    pub const fn source(self) -> ManimCamera3DProfile {
        self.source
    }
    pub const fn axis(self) -> Option<CameraRotationAxis> {
        match self.profile {
            CameraMotionProfile::Angular(axis) => Some(axis),
            CameraMotionProfile::ThreeDIllusion { .. } => None,
        }
    }
    pub const fn rate(self) -> f64 {
        self.rate
    }
    pub const fn start(self) -> f64 {
        self.start
    }
    pub const fn end(self) -> Option<f64> {
        self.end
    }
    pub const fn clips(self) -> (f64, f64) {
        (self.near, self.far)
    }

    fn is_valid(self) -> bool {
        match self.profile {
            CameraMotionProfile::Angular(axis) => Self::new(
                self.source,
                axis,
                self.rate,
                self.start,
                self.end,
                self.near,
                self.far,
            )
            .is_some(),
            CameraMotionProfile::ThreeDIllusion {
                origin_phi,
                origin_theta,
            } => Self::three_d_illusion(
                self.source,
                self.rate,
                self.start,
                self.end,
                self.near,
                self.far,
                Some(origin_phi),
                Some(origin_theta),
            )
            .is_some(),
        }
    }

    pub fn close(self, end: f64) -> Option<Self> {
        if self.end.is_some() {
            return None;
        }
        match self.profile {
            CameraMotionProfile::Angular(axis) => Self::new(
                self.source,
                axis,
                self.rate,
                self.start,
                Some(end),
                self.near,
                self.far,
            ),
            CameraMotionProfile::ThreeDIllusion {
                origin_phi,
                origin_theta,
            } => Self::three_d_illusion(
                self.source,
                self.rate,
                self.start,
                Some(end),
                self.near,
                self.far,
                Some(origin_phi),
                Some(origin_theta),
            ),
        }
    }

    pub fn is_active_at(self, time: f64) -> bool {
        time.is_finite() && time >= self.start && self.end.is_none_or(|end| time < end)
    }

    /// Evaluate from the captured source rather than integrating frame `dt`.
    /// A numeric overflow is an explicit failure; it cannot publish a stale pose.
    pub fn sample(self, time: f64) -> Option<ManimCamera3DProfile> {
        if !time.is_finite() {
            return None;
        }
        let time = self.end.map_or(time, |end| time.min(end)).max(self.start);
        let delta = (time - self.start) * self.rate;
        let mut profile = self.source;
        match self.profile {
            CameraMotionProfile::Angular(CameraRotationAxis::Phi) => profile.phi += delta,
            CameraMotionProfile::Angular(CameraRotationAxis::Theta) => profile.theta += delta,
            CameraMotionProfile::Angular(CameraRotationAxis::Gamma) => profile.gamma += delta,
            CameraMotionProfile::ThreeDIllusion {
                origin_phi,
                origin_theta,
            } => {
                let phase = delta;
                profile.theta = origin_theta + 0.2 * phase.sin();
                profile.phi = origin_phi + 0.1 * phase.cos() - 0.1;
            }
        }
        profile.camera(self.near, self.far)?;
        Some(profile)
    }
}

/// Validate camera-local occurrence order before semantic publication.
pub fn camera_motion_history_is_valid(motions: &[CameraAngularMotion]) -> bool {
    motions.len() <= MAX_CAMERA_MOTION_INTERVALS
        && motions.iter().enumerate().all(|(index, motion)| {
            motion.is_valid()
                && (index == 0
                    || motions[index - 1]
                        .end
                        .is_some_and(|end| end <= motion.start))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SemanticVec3;

    fn profile() -> ManimCamera3DProfile {
        ManimCamera3DProfile {
            phi: 0.6,
            theta: -1.2,
            gamma: 0.0,
            focal_distance: 20.0,
            zoom: 1.0,
            frame_height: 8.0,
            frame_center: SemanticVec3::ZERO,
        }
    }

    #[test]
    fn angular_motion_uses_authored_time_and_exact_release_endpoint() {
        let motion = CameraAngularMotion::new(
            profile(),
            CameraRotationAxis::Theta,
            0.5,
            2.0,
            None,
            0.1,
            100.0,
        )
        .unwrap();
        assert_eq!(motion.sample(1.0), Some(profile()));
        assert!((motion.sample(4.0).unwrap().theta + 0.2).abs() < 1e-14);
        let closed = motion.close(4.0).unwrap();
        assert!(!closed.is_active_at(4.0));
        assert_eq!(closed.sample(100.0), closed.sample(4.0));
        assert!(closed.close(5.0).is_none());
    }

    #[test]
    fn illusion_motion_matches_pinned_manim_formula_and_phase() {
        let mut source = profile();
        source.phi = 75_f64.to_radians();
        source.theta = 30_f64.to_radians();
        let motion =
            CameraAngularMotion::three_d_illusion(source, 2.0, 4.0, None, 0.1, 100.0, None, None)
                .unwrap();

        assert_eq!(motion.sample(4.0), Some(source));
        let halfway = motion.sample(4.0 + std::f64::consts::PI / 4.0).unwrap();
        assert!((halfway.theta - (source.theta + 0.2)).abs() < 1e-14);
        assert!((halfway.phi - (source.phi - 0.1)).abs() < 1e-14);
        let stopped = motion.close(4.0 + std::f64::consts::PI / 2.0).unwrap();
        let endpoint = stopped.sample(stopped.end().unwrap()).unwrap();
        assert!((endpoint.theta - source.theta).abs() < 1e-14);
        assert!((endpoint.phi - (source.phi - 0.2)).abs() < 1e-14);
        assert_eq!(stopped.sample(100.0), Some(endpoint));

        let settled =
            CameraAngularMotion::three_d_illusion(source, 0.0, 0.0, None, 0.1, 100.0, None, None)
                .unwrap();
        assert_eq!(settled.sample(100.0), Some(source));
        assert!(CameraAngularMotion::three_d_illusion(
            source,
            1.0,
            0.0,
            None,
            0.1,
            100.0,
            Some(f64::NAN),
            None,
        )
        .is_none());
    }

    #[test]
    fn motion_history_rejects_overlap_open_predecessors_and_invalid_numbers() {
        let make = |start, end| {
            CameraAngularMotion::new(
                profile(),
                CameraRotationAxis::Gamma,
                0.2,
                start,
                end,
                0.1,
                100.0,
            )
            .unwrap()
        };
        assert!(camera_motion_history_is_valid(&[
            make(0.0, Some(1.0)),
            make(1.0, None)
        ]));
        assert!(!camera_motion_history_is_valid(&[
            make(0.0, None),
            make(1.0, None)
        ]));
        assert!(!camera_motion_history_is_valid(&[
            make(0.0, Some(2.0)),
            make(1.0, None)
        ]));
        assert!(CameraAngularMotion::new(
            profile(),
            CameraRotationAxis::Phi,
            f64::INFINITY,
            0.0,
            None,
            0.1,
            100.0
        )
        .is_none());
        assert!(CameraAngularMotion::new(
            profile(),
            CameraRotationAxis::Phi,
            f64::MAX,
            0.0,
            Some(2.0),
            0.1,
            100.0
        )
        .is_none());
    }

    #[test]
    fn intentional_ambient_seek_history_has_a_checked_size_limit() {
        let mut motions: Vec<_> = (0..MAX_CAMERA_MOTION_INTERVALS)
            .map(|index| {
                CameraAngularMotion::new(
                    profile(),
                    CameraRotationAxis::Theta,
                    0.0,
                    index as f64,
                    Some(index as f64 + 1.0),
                    0.1,
                    100.0,
                )
                .unwrap()
            })
            .collect();
        assert!(camera_motion_history_is_valid(&motions));
        motions.push(
            CameraAngularMotion::new(
                profile(),
                CameraRotationAxis::Theta,
                0.0,
                MAX_CAMERA_MOTION_INTERVALS as f64,
                None,
                0.1,
                100.0,
            )
            .unwrap(),
        );
        assert!(!camera_motion_history_is_valid(&motions));
    }
}
