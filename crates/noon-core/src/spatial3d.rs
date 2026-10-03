//! Renderer-independent numeric conventions for Phase D world and camera state.
//!
//! World axes are right-handed: +X right, +Y up, +Z toward the viewer. A camera
//! with identity orientation looks along -Z. Object points use translation *
//! rotation * scale; camera orientation maps camera-local axes into world axes.
//! Clip depth is 0 at the near plane and 1 at the far plane, matching WebGPU.
//! A WebGL backend may convert that clip convention at its rendering boundary.
//! Viewport aspect is derived at evaluation time, not authored camera state.
//! These values are a numeric substrate, not a separate scene or camera store.

use crate::{SemanticTransform2_5D, SemanticVec3};

fn add(a: SemanticVec3, b: SemanticVec3) -> SemanticVec3 {
    SemanticVec3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}

fn subtract(a: SemanticVec3, b: SemanticVec3) -> SemanticVec3 {
    SemanticVec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

fn cross(a: SemanticVec3, b: SemanticVec3) -> SemanticVec3 {
    SemanticVec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

/// Unit quaternion rotating a local vector into its parent's right-handed axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticRotation3D {
    w: f64,
    x: f64,
    y: f64,
    z: f64,
}

impl SemanticRotation3D {
    pub const IDENTITY: Self = Self {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// Reject a zero or non-finite axis or angle, then normalize the rotation.
    pub fn from_axis_angle(axis: SemanticVec3, radians: f64) -> Option<Self> {
        if !axis.is_finite() || !radians.is_finite() {
            return None;
        }
        let largest = axis.x.abs().max(axis.y.abs()).max(axis.z.abs());
        if largest == 0.0 {
            return None;
        }
        let axis = SemanticVec3::new(axis.x / largest, axis.y / largest, axis.z / largest);
        let length = axis.x.hypot(axis.y).hypot(axis.z);
        let (sine, cosine) = (radians * 0.5).sin_cos();
        Self::normalized(
            cosine,
            axis.x / length * sine,
            axis.y / length * sine,
            axis.z / length * sine,
        )
    }

    fn normalized(w: f64, x: f64, y: f64, z: f64) -> Option<Self> {
        let length = w.hypot(x).hypot(y).hypot(z);
        if length == 0.0 || !length.is_finite() {
            return None;
        }
        Some(Self {
            w: w / length,
            x: x / length,
            y: y / length,
            z: z / length,
        })
    }

    fn inverse(self) -> Self {
        Self {
            w: self.w,
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }

    fn rotate(self, value: SemanticVec3) -> Option<SemanticVec3> {
        if !value.is_finite() {
            return None;
        }
        let imaginary = SemanticVec3::new(self.x, self.y, self.z);
        let twice_cross = cross(imaginary, value);
        let twice_cross = SemanticVec3::new(
            twice_cross.x * 2.0,
            twice_cross.y * 2.0,
            twice_cross.z * 2.0,
        );
        let result = add(
            add(
                value,
                SemanticVec3::new(
                    self.w * twice_cross.x,
                    self.w * twice_cross.y,
                    self.w * twice_cross.z,
                ),
            ),
            cross(imaginary, twice_cross),
        );
        result.is_finite().then_some(result)
    }
}

/// High-precision object transform; no renderer precision or resource state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticWorldTransform3D {
    pub translation: SemanticVec3,
    pub rotation: SemanticRotation3D,
    pub scale: SemanticVec3,
}

impl SemanticWorldTransform3D {
    pub const IDENTITY: Self = Self {
        translation: SemanticVec3::ZERO,
        rotation: SemanticRotation3D::IDENTITY,
        scale: SemanticVec3::new(1.0, 1.0, 1.0),
    };

    pub fn new(
        translation: SemanticVec3,
        rotation: SemanticRotation3D,
        scale: SemanticVec3,
    ) -> Option<Self> {
        (translation.is_finite() && scale.is_finite()).then_some(Self {
            translation,
            rotation,
            scale,
        })
    }

    /// Lift existing 2.5D authored transforms without changing their XY order.
    pub fn from_2_5d(value: SemanticTransform2_5D) -> Option<Self> {
        Self::new(
            value.translation,
            SemanticRotation3D::from_axis_angle(
                SemanticVec3::new(0.0, 0.0, 1.0),
                value.rotation_z,
            )?,
            value.scale,
        )
    }

    pub fn transform_point(self, point: SemanticVec3) -> Option<SemanticVec3> {
        if !point.is_finite() {
            return None;
        }
        let scaled = SemanticVec3::new(
            point.x * self.scale.x,
            point.y * self.scale.y,
            point.z * self.scale.z,
        );
        let result = add(self.rotation.rotate(scaled)?, self.translation);
        result.is_finite().then_some(result)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SemanticProjection3D {
    Perspective {
        vertical_fov_radians: f64,
        near: f64,
        far: f64,
    },
    Orthographic {
        height: f64,
        near: f64,
        far: f64,
    },
}

impl SemanticProjection3D {
    /// Validate the authored declaration independently of a render target.
    pub fn is_valid(self) -> bool {
        self.coefficients(1.0).is_some()
    }

    // X/Y scales and Z scale/offset in right-handed view coordinates.
    fn coefficients(self, aspect: f64) -> Option<[f64; 4]> {
        if !aspect.is_finite() || aspect <= 0.0 {
            return None;
        }
        let (near, far) = match self {
            Self::Perspective { near, far, .. } | Self::Orthographic { near, far, .. } => {
                (near, far)
            }
        };
        if !near.is_finite() || near <= 0.0 || !far.is_finite() || far <= near {
            return None;
        }
        let depth_range = far - near;
        let coefficients = match self {
            Self::Perspective {
                vertical_fov_radians,
                ..
            } => {
                if !vertical_fov_radians.is_finite()
                    || vertical_fov_radians <= 0.0
                    || vertical_fov_radians >= std::f64::consts::PI
                {
                    return None;
                }
                let vertical_scale = 1.0 / (vertical_fov_radians * 0.5).tan();
                let depth_scale = -far / depth_range;
                [
                    vertical_scale / aspect,
                    vertical_scale,
                    depth_scale,
                    depth_scale * near,
                ]
            }
            Self::Orthographic { height, .. } => {
                if !height.is_finite() || height <= 0.0 {
                    return None;
                }
                let vertical_scale = 2.0 / height;
                [
                    vertical_scale / aspect,
                    vertical_scale,
                    -1.0 / depth_range,
                    -near / depth_range,
                ]
            }
        };
        (coefficients.iter().all(|value| value.is_finite())
            && coefficients[0] > 0.0
            && coefficients[1] > 0.0
            && coefficients[2] < 0.0
            && coefficients[3] < 0.0)
            .then_some(coefficients)
    }

    fn clip(self, view: SemanticVec3, aspect: f64) -> Option<SemanticClipPoint3D> {
        if !view.is_finite() {
            return None;
        }
        let [x_scale, y_scale, z_scale, z_offset] = self.coefficients(aspect)?;
        let result = SemanticClipPoint3D {
            x: view.x * x_scale,
            y: view.y * y_scale,
            z: view.z * z_scale + z_offset,
            w: match self {
                Self::Perspective { .. } => -view.z,
                Self::Orthographic { .. } => 1.0,
            },
        };
        result.is_finite().then_some(result)
    }
}

/// Camera orientation maps local +X/+Y/-Z into world right/up/forward.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticCamera3D {
    pub position: SemanticVec3,
    pub orientation: SemanticRotation3D,
    pub projection: SemanticProjection3D,
}

impl SemanticCamera3D {
    pub fn new(
        position: SemanticVec3,
        orientation: SemanticRotation3D,
        projection: SemanticProjection3D,
    ) -> Option<Self> {
        (position.is_finite() && projection.is_valid()).then_some(Self {
            position,
            orientation,
            projection,
        })
    }

    /// Numeric oracle: local object point -> world -> view -> homogeneous clip.
    ///
    /// Aspect is viewport width / height. Invalid or unrepresentable projection
    /// coefficients and non-finite intermediate/results return `None`.
    pub fn project(
        self,
        object: SemanticWorldTransform3D,
        local_point: SemanticVec3,
        viewport_aspect: f64,
    ) -> Option<SemanticClipPoint3D> {
        let world = object.transform_point(local_point)?;
        let view = self
            .orientation
            .inverse()
            .rotate(subtract(world, self.position))?;
        self.projection.clip(view, viewport_aspect)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticClipPoint3D {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

impl SemanticClipPoint3D {
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite() && self.w.is_finite()
    }

    pub fn normalized_device_coordinates(self) -> Option<SemanticVec3> {
        if !self.is_finite() || self.w <= 0.0 {
            return None;
        }
        let value = SemanticVec3::new(self.x / self.w, self.y / self.w, self.z / self.w);
        value.is_finite().then_some(value)
    }

    pub fn inside_frustum(self) -> bool {
        self.is_finite()
            && self.w > 0.0
            && self.x.abs() <= self.w
            && self.y.abs() <= self.w
            && (0.0..=self.w).contains(&self.z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1.0e-12,
            "{actual} != {expected}"
        );
    }

    #[test]
    fn lifted_2d_transform_preserves_scale_rotation_translation_order() {
        let old = SemanticTransform2_5D {
            translation: SemanticVec3::new(3.0, -4.0, 5.0),
            scale: SemanticVec3::new(2.0, 0.5, 7.0),
            rotation_z: std::f64::consts::FRAC_PI_2,
        };
        let point = SemanticVec3::new(2.0, -4.0, 3.0);
        let world = SemanticWorldTransform3D::from_2_5d(old)
            .unwrap()
            .transform_point(point)
            .unwrap();
        let xy = old.transform_xy(point.x, point.y);
        near(world.x, xy.0);
        near(world.y, xy.1);
        near(world.z, 26.0);
    }

    #[test]
    fn perspective_oracle_pins_handedness_near_far_and_frustum_edges() {
        let camera = SemanticCamera3D::new(
            SemanticVec3::new(0.0, 0.0, 10.0),
            SemanticRotation3D::IDENTITY,
            SemanticProjection3D::Perspective {
                vertical_fov_radians: std::f64::consts::FRAC_PI_2,
                near: 1.0,
                far: 11.0,
            },
        )
        .unwrap();
        let project = |point| {
            camera
                .project(SemanticWorldTransform3D::IDENTITY, point, 1.0)
                .unwrap()
        };
        let near_point = project(SemanticVec3::new(0.0, 0.0, 9.0));
        let far_point = project(SemanticVec3::new(0.0, 0.0, -1.0));
        near(near_point.z, 0.0);
        near(near_point.w, 1.0);
        near(far_point.normalized_device_coordinates().unwrap().z, 1.0);
        let right_edge = project(SemanticVec3::new(2.0, 0.0, 8.0));
        near(right_edge.normalized_device_coordinates().unwrap().x, 1.0);
        assert!(project(SemanticVec3::new(1.99, 0.0, 8.0)).inside_frustum());
        assert!(!project(SemanticVec3::new(3.0, 0.0, 8.0)).inside_frustum());
        assert!(!project(SemanticVec3::new(0.0, 0.0, 11.0)).inside_frustum());
    }

    #[test]
    fn camera_orientation_is_inverted_for_world_to_view() {
        let camera = SemanticCamera3D::new(
            SemanticVec3::ZERO,
            SemanticRotation3D::from_axis_angle(
                SemanticVec3::new(1.0, 0.0, 0.0),
                std::f64::consts::FRAC_PI_2,
            )
            .unwrap(),
            SemanticProjection3D::Orthographic {
                height: 4.0,
                near: 1.0,
                far: 11.0,
            },
        )
        .unwrap();
        let clip = camera
            .project(
                SemanticWorldTransform3D::IDENTITY,
                SemanticVec3::new(0.0, 5.0, 0.0),
                1.0,
            )
            .unwrap();
        near(clip.z, 0.4);
        assert!(clip.inside_frustum());
    }

    #[test]
    fn invalid_parameters_and_overflow_fail_without_nonfinite_projection() {
        assert!(SemanticRotation3D::from_axis_angle(SemanticVec3::ZERO, 1.0).is_none());
        assert!(
            SemanticRotation3D::from_axis_angle(SemanticVec3::new(f64::NAN, 0.0, 0.0), 1.0)
                .is_none()
        );
        assert!(SemanticWorldTransform3D::new(
            SemanticVec3::new(f64::INFINITY, 0.0, 0.0),
            SemanticRotation3D::IDENTITY,
            SemanticVec3::new(1.0, 1.0, 1.0)
        )
        .is_none());
        for projection in [
            SemanticProjection3D::Perspective {
                vertical_fov_radians: 0.0,
                near: 1.0,
                far: 2.0,
            },
            SemanticProjection3D::Perspective {
                vertical_fov_radians: 1.0,
                near: 0.0,
                far: 2.0,
            },
            SemanticProjection3D::Orthographic {
                height: 0.0,
                near: 1.0,
                far: 2.0,
            },
            SemanticProjection3D::Orthographic {
                height: 1.0,
                near: 2.0,
                far: 1.0,
            },
        ] {
            assert!(!projection.is_valid());
            assert!(SemanticCamera3D::new(
                SemanticVec3::ZERO,
                SemanticRotation3D::IDENTITY,
                projection
            )
            .is_none());
        }
        let camera = SemanticCamera3D::new(
            SemanticVec3::ZERO,
            SemanticRotation3D::IDENTITY,
            SemanticProjection3D::Perspective {
                vertical_fov_radians: 1.0,
                near: 1.0,
                far: 10.0,
            },
        )
        .unwrap();
        assert!(camera
            .project(
                SemanticWorldTransform3D::IDENTITY,
                SemanticVec3::new(f64::MAX, f64::MAX, 1.0),
                1.0,
            )
            .is_none());
    }

    #[test]
    fn orthographic_resize_changes_only_horizontal_projection() {
        let camera = SemanticCamera3D::new(
            SemanticVec3::ZERO,
            SemanticRotation3D::IDENTITY,
            SemanticProjection3D::Orthographic {
                height: 4.0,
                near: 1.0,
                far: 11.0,
            },
        )
        .unwrap();
        let point = SemanticVec3::new(2.0, 1.0, -6.0);
        let square = camera
            .project(SemanticWorldTransform3D::IDENTITY, point, 1.0)
            .unwrap();
        let wide = camera
            .project(SemanticWorldTransform3D::IDENTITY, point, 2.0)
            .unwrap();
        near(square.x, 1.0);
        near(wide.x, 0.5);
        near(wide.y, 0.5);
        near(wide.z, 0.5);
        near(wide.y, square.y);
        near(wide.z, square.z);
        for aspect in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::from_bits(1)] {
            assert!(camera
                .project(SemanticWorldTransform3D::IDENTITY, point, aspect)
                .is_none());
        }
        assert!(!camera
            .project(
                SemanticWorldTransform3D::IDENTITY,
                SemanticVec3::new(0.0, 0.0, 1.0),
                1.0
            )
            .unwrap()
            .inside_frustum());
    }

    #[test]
    fn perspective_can_represent_manim_focal_distance_and_frame_zoom() {
        // ManimCE v0.21 ThreeDCamera.project_points' ordinary finite projection:
        // x/y *= focal_distance / (focal_distance - z) * zoom.
        let focal_distance = 20.0;
        let frame_height = 8.0;
        let zoom = 1.5;
        let aspect = 16.0 / 9.0;
        let camera = SemanticCamera3D::new(
            SemanticVec3::new(0.0, 0.0, focal_distance),
            SemanticRotation3D::IDENTITY,
            SemanticProjection3D::Perspective {
                vertical_fov_radians: 2.0
                    * (frame_height / (2.0_f64 * focal_distance * zoom)).atan(),
                near: 1.0,
                far: 100.0,
            },
        )
        .unwrap();
        for point in [
            SemanticVec3::new(2.0, -1.0, 3.0),
            SemanticVec3::new(-4.0, 2.0, -5.0),
        ] {
            let ndc = camera
                .project(SemanticWorldTransform3D::IDENTITY, point, aspect)
                .unwrap()
                .normalized_device_coordinates()
                .unwrap();
            let factor = focal_distance / (focal_distance - point.z) * zoom;
            near(ndc.x, point.x * factor * 2.0 / (frame_height * aspect));
            near(ndc.y, point.y * factor * 2.0 / frame_height);
        }
    }

    #[test]
    fn high_precision_translation_and_extreme_axis_normalization_are_retained() {
        let rotation = SemanticRotation3D::from_axis_angle(
            SemanticVec3::new(f64::MAX, f64::MAX, 0.0),
            std::f64::consts::PI,
        )
        .unwrap();
        let rotated = rotation.rotate(SemanticVec3::new(1.0, 0.0, 0.0)).unwrap();
        near(rotated.x, 0.0);
        near(rotated.y, 1.0);
        near(rotated.z, 0.0);
        assert!(SemanticRotation3D::from_axis_angle(
            SemanticVec3::new(f64::from_bits(1), 0.0, 0.0),
            1.0
        )
        .is_some());
        let translation = SemanticVec3::new(1.0e12, -1.0e12, 1.0e12);
        let object = SemanticWorldTransform3D::new(
            translation,
            SemanticRotation3D::IDENTITY,
            SemanticVec3::new(1.0, 1.0, 1.0),
        )
        .unwrap();
        let camera = SemanticCamera3D::new(
            translation,
            SemanticRotation3D::IDENTITY,
            SemanticProjection3D::Orthographic {
                height: 4.0,
                near: 1.0,
                far: 11.0,
            },
        )
        .unwrap();
        let projected = camera
            .project(object, SemanticVec3::new(1.0, 0.0, -6.0), 1.0)
            .unwrap();
        near(projected.x, 0.5);
        near(projected.z, 0.5);
    }

    #[test]
    fn unrepresentable_projection_coefficients_are_rejected() {
        let tiny = f64::from_bits(1);
        for projection in [
            SemanticProjection3D::Perspective {
                vertical_fov_radians: tiny,
                near: 1.0,
                far: 2.0,
            },
            SemanticProjection3D::Orthographic {
                height: tiny,
                near: 1.0,
                far: 2.0,
            },
            SemanticProjection3D::Orthographic {
                height: 1.0,
                near: tiny,
                far: tiny * 2.0,
            },
        ] {
            assert!(!projection.is_valid());
        }
        let camera = SemanticCamera3D::new(
            SemanticVec3::ZERO,
            SemanticRotation3D::IDENTITY,
            SemanticProjection3D::Orthographic {
                height: f64::MAX,
                near: 1.0,
                far: 11.0,
            },
        )
        .unwrap();
        // Do not overflow x*2 before dividing by an equally large height.
        near(
            camera
                .project(
                    SemanticWorldTransform3D::IDENTITY,
                    SemanticVec3::new(f64::MAX, 0.0, -6.0),
                    1.0,
                )
                .unwrap()
                .x,
            2.0,
        );
    }
}
