//! Analytic camera motion in authored time, consumed by the existing timeline.

use crate::ManimCamera3DProfile;

/// Manim's unwrapped angular coordinate driven by one ambient occurrence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraRotationAxis {
    Phi,
    Theta,
    Gamma,
}

/// One immutable native angular driver interval. It owns no clock or scheduler.
/// `end` is exclusive for driver ownership; sampling at it returns the exact
/// endpoint for release. Closed occurrences are intentional authored seek history.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraAngularMotion {
    source: ManimCamera3DProfile,
    axis: CameraRotationAxis,
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
            axis,
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

    pub const fn source(self) -> ManimCamera3DProfile {
        self.source
    }
    pub const fn axis(self) -> CameraRotationAxis {
        self.axis
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

    pub fn close(self, end: f64) -> Option<Self> {
        if self.end.is_some() {
            return None;
        }
        Self::new(
            self.source,
            self.axis,
            self.rate,
            self.start,
            Some(end),
            self.near,
            self.far,
        )
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
        match self.axis {
            CameraRotationAxis::Phi => profile.phi += delta,
            CameraRotationAxis::Theta => profile.theta += delta,
            CameraRotationAxis::Gamma => profile.gamma += delta,
        }
        profile.camera(self.near, self.far)?;
        Some(profile)
    }
}

/// Validate camera-local occurrence order before semantic publication.
pub fn camera_motion_history_is_valid(motions: &[CameraAngularMotion]) -> bool {
    motions.iter().enumerate().all(|(index, motion)| {
        CameraAngularMotion::new(
            motion.source,
            motion.axis,
            motion.rate,
            motion.start,
            motion.end,
            motion.near,
            motion.far,
        )
        .is_some()
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
}
