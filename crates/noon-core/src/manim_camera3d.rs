//! Numeric conversion from ManimCE's finite `ThreeDCamera` profile to the
//! shared immutable camera contract. This is a constructor helper, not another
//! camera state or an Euler-angle runtime representation.

use crate::{SemanticCamera3D, SemanticProjection3D, SemanticRotation3D, SemanticVec3};
use serde::{Deserialize, Serialize};

/// The finite-perspective subset of ManimCE v0.21 `ThreeDCamera` parameters.
///
/// `phi`, `theta`, and `gamma` are radians. `frame_center` is the point the
/// camera's focal-distance offset is measured from. Manim's `zoom` and frame
/// height are converted to vertical field of view; viewport aspect remains a
/// render-time property of `SemanticCamera3D`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ManimCamera3DProfile {
    pub phi: f64,
    pub theta: f64,
    pub gamma: f64,
    pub focal_distance: f64,
    pub zoom: f64,
    pub frame_height: f64,
    pub frame_center: SemanticVec3,
}

impl ManimCamera3DProfile {
    /// Linearly interpolate Manim's unwrapped camera parameters. Angles are
    /// intentionally not reduced modulo a turn, so multi-turn authored motion
    /// keeps its direction and distance.
    pub fn interpolate(from: Self, to: Self, alpha: f64) -> Option<Self> {
        if !alpha.is_finite() {
            return None;
        }
        let lerp = |a: f64, b: f64| {
            if alpha == 0.0 {
                a
            } else if alpha == 1.0 {
                b
            } else if (0.0..=1.0).contains(&alpha) && a.is_sign_positive() != b.is_sign_positive() {
                a * (1.0 - alpha) + b * alpha
            } else {
                a + (b - a) * alpha
            }
        };
        let profile = Self {
            phi: lerp(from.phi, to.phi),
            theta: lerp(from.theta, to.theta),
            gamma: lerp(from.gamma, to.gamma),
            focal_distance: lerp(from.focal_distance, to.focal_distance),
            zoom: lerp(from.zoom, to.zoom),
            frame_height: lerp(from.frame_height, to.frame_height),
            frame_center: SemanticVec3::new(
                lerp(from.frame_center.x, to.frame_center.x),
                lerp(from.frame_center.y, to.frame_center.y),
                lerp(from.frame_center.z, to.frame_center.z),
            ),
        };
        [
            profile.phi,
            profile.theta,
            profile.gamma,
            profile.focal_distance,
            profile.zoom,
            profile.frame_height,
            profile.frame_center.x,
            profile.frame_center.y,
            profile.frame_center.z,
        ]
        .into_iter()
        .all(f64::is_finite)
        .then_some(profile)
    }

    /// Sample this camera-profile track without converting through a
    /// quaternion track. Invalid interpolated optics or clipping planes fail.
    pub fn sample_camera(
        from: Self,
        to: Self,
        alpha: f64,
        near: f64,
        far: f64,
    ) -> Option<SemanticCamera3D> {
        Self::interpolate(from, to, alpha)?.camera(near, far)
    }

    /// Build a shared semantic camera with explicit clipping planes.
    ///
    /// The mapping follows pinned ManimCE v0.21 `ThreeDCamera`:
    /// `R_view = Rz(gamma) Rx(-phi) Rz(-theta-π/2)` and
    /// `position = frame_center + inverse(R_view)·(0,0,focal_distance)`.
    /// Manim does not define these near/far planes, so callers must choose
    /// them for the renderer. Exponential projection and behind-camera point
    /// behavior are outside this finite-perspective compatibility helper.
    pub fn camera(self, near: f64, far: f64) -> Option<SemanticCamera3D> {
        if !self.phi.is_finite()
            || !self.theta.is_finite()
            || !self.gamma.is_finite()
            || !self.focal_distance.is_finite()
            || self.focal_distance <= 0.0
            || !self.zoom.is_finite()
            || self.zoom <= 0.0
            || !self.frame_height.is_finite()
            || self.frame_height <= 0.0
            || !self.frame_center.is_finite()
        {
            return None;
        }

        let rotation_z =
            |angle| SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 0.0, 1.0), angle);
        let rotation_x =
            |angle| SemanticRotation3D::from_axis_angle(SemanticVec3::new(1.0, 0.0, 0.0), angle);

        // The camera-local-to-world orientation is the inverse of Manim's
        // world-to-camera rotation matrix. Quaternion compose applies rhs first.
        let orientation = rotation_z(self.theta + std::f64::consts::FRAC_PI_2)?
            .compose(rotation_x(self.phi)?)?
            .compose(rotation_z(-self.gamma)?)?;
        let position =
            orientation.rotate_vector(SemanticVec3::new(0.0, 0.0, self.focal_distance))?;
        let position = SemanticVec3::new(
            self.frame_center.x + position.x,
            self.frame_center.y + position.y,
            self.frame_center.z + position.z,
        );
        let vertical_fov_radians =
            2.0 * (self.frame_height / (2.0 * self.focal_distance * self.zoom)).atan();
        SemanticCamera3D::new(
            position,
            orientation,
            SemanticProjection3D::Perspective {
                vertical_fov_radians,
                near,
                far,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SemanticWorldTransform3D;

    fn near(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1.0e-11,
            "{actual} != {expected}"
        );
    }

    fn manim_view_point(profile: ManimCamera3DProfile, point: SemanticVec3) -> SemanticVec3 {
        // Independent pointwise oracle for pinned ThreeDCamera's matrix
        // product. Its loop left-multiplies, yielding C * B * A; applying that
        // matrix to a column vector applies the Z, X, then Z rotations below.
        let mut x = point.x - profile.frame_center.x;
        let mut y = point.y - profile.frame_center.y;
        let mut z = point.z - profile.frame_center.z;

        let angle = -profile.theta - std::f64::consts::FRAC_PI_2;
        let (sine, cosine) = angle.sin_cos();
        (x, y) = (cosine * x - sine * y, sine * x + cosine * y);

        let angle = -profile.phi;
        let (sine, cosine) = angle.sin_cos();
        (y, z) = (cosine * y - sine * z, sine * y + cosine * z);

        let (sine, cosine) = profile.gamma.sin_cos();
        (x, y) = (cosine * x - sine * y, sine * x + cosine * y);

        SemanticVec3::new(x, y, z)
    }

    #[test]
    fn camera_pose_and_projection_match_pinned_manim_finite_formula() {
        let profile = ManimCamera3DProfile {
            phi: 0.63,
            theta: -1.17,
            gamma: 0.29,
            focal_distance: 17.0,
            zoom: 1.4,
            frame_height: 8.0,
            frame_center: SemanticVec3::new(1.25, -0.75, 2.0),
        };
        let near_plane = 0.1;
        let far_plane = 100.0;
        let aspect = 16.0 / 9.0;
        let camera = profile.camera(near_plane, far_plane).unwrap();
        let focal = camera
            .orientation
            .rotate_vector(SemanticVec3::new(0.0, 0.0, 1.0))
            .unwrap();
        near(
            camera.position.x,
            profile.frame_center.x + focal.x * profile.focal_distance,
        );
        near(
            camera.position.y,
            profile.frame_center.y + focal.y * profile.focal_distance,
        );
        near(
            camera.position.z,
            profile.frame_center.z + focal.z * profile.focal_distance,
        );

        for point in [
            SemanticVec3::new(2.0, -1.0, 4.0),
            SemanticVec3::new(-3.0, 2.5, -1.0),
        ] {
            let view = manim_view_point(profile, point);
            assert!(profile.focal_distance - view.z > 0.0);
            let factor = profile.focal_distance / (profile.focal_distance - view.z) * profile.zoom;
            let expected_x = view.x * factor * 2.0 / (profile.frame_height * aspect);
            let expected_y = view.y * factor * 2.0 / profile.frame_height;
            let actual = camera
                .project(SemanticWorldTransform3D::IDENTITY, point, aspect)
                .unwrap()
                .normalized_device_coordinates()
                .unwrap();
            near(actual.x, expected_x);
            near(actual.y, expected_y);
        }
    }

    #[test]
    fn camera_validation_rejects_nonfinite_or_unrepresentable_profiles() {
        let valid = ManimCamera3DProfile {
            phi: 0.0,
            theta: -std::f64::consts::FRAC_PI_2,
            gamma: 0.0,
            focal_distance: 20.0,
            zoom: 1.0,
            frame_height: 8.0,
            frame_center: SemanticVec3::ZERO,
        };
        assert!(valid.camera(0.1, 100.0).is_some());
        assert!(valid.camera(0.0, 100.0).is_none());
        assert!(valid.camera(0.1, 0.1).is_none());
        assert!(ManimCamera3DProfile {
            zoom: f64::INFINITY,
            ..valid
        }
        .camera(0.1, 100.0)
        .is_none());
        assert!(ManimCamera3DProfile {
            focal_distance: 0.0,
            ..valid
        }
        .camera(0.1, 100.0)
        .is_none());
        assert!(ManimCamera3DProfile {
            frame_height: f64::NAN,
            ..valid
        }
        .camera(0.1, 100.0)
        .is_none());
        assert!(ManimCamera3DProfile {
            frame_center: SemanticVec3::new(0.0, f64::INFINITY, 0.0),
            ..valid
        }
        .camera(0.1, 100.0)
        .is_none());
    }

    #[test]
    fn profile_track_interpolates_unwrapped_angles_and_samples_pose_and_lens_together() {
        let from = ManimCamera3DProfile {
            phi: 0.1,
            theta: -std::f64::consts::FRAC_PI_2,
            gamma: 0.0,
            focal_distance: 10.0,
            zoom: 1.0,
            frame_height: 8.0,
            frame_center: SemanticVec3::ZERO,
        };
        let to = ManimCamera3DProfile {
            phi: 0.7,
            theta: from.theta + 4.0 * std::f64::consts::PI,
            gamma: -0.4,
            focal_distance: 20.0,
            zoom: 3.0,
            frame_height: 10.0,
            frame_center: SemanticVec3::new(2.0, -4.0, 6.0),
        };
        let midpoint = ManimCamera3DProfile::interpolate(from, to, 0.5).unwrap();
        near(midpoint.theta, from.theta + 2.0 * std::f64::consts::PI);
        near(midpoint.focal_distance, 15.0);
        near(midpoint.zoom, 2.0);
        near(midpoint.frame_center.y, -2.0);

        let camera = ManimCamera3DProfile::sample_camera(from, to, 0.5, 0.1, 80.0).unwrap();
        assert_eq!(camera, midpoint.camera(0.1, 80.0).unwrap());
        assert!(ManimCamera3DProfile::interpolate(from, to, f64::NAN).is_none());
        assert!(ManimCamera3DProfile::sample_camera(from, to, 0.5, f64::NAN, 80.0).is_none());
        let extreme = ManimCamera3DProfile::interpolate(
            ManimCamera3DProfile {
                phi: -f64::MAX,
                ..from
            },
            ManimCamera3DProfile {
                phi: f64::MAX,
                ..to
            },
            0.5,
        )
        .unwrap();
        near(extreme.phi, 0.0);
    }
}
