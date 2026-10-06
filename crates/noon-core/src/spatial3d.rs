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

/// Material policies for retained spatial meshes. `PointLit` uses one
/// backend-neutral cubic directional response. `CairoSurface` is reserved for
/// sampled Surface cells carrying their immutable Cairo control-point profile;
/// it does not claim Cairo behavior for arbitrary mesh topology.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SemanticSpatialMaterial {
    #[default]
    Unlit,
    PointLit,
    CairoSurface,
}

/// How world-authored geometry is composed with the active camera.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum SemanticSpatialCompositionDomain {
    /// Geometry follows its full world transform and camera projection.
    #[default]
    World,
    /// Project the effective anchor center while preserving world point offsets.
    FixedOrientation,
    /// Geometry is interpreted in camera-frame coordinates.
    FixedFrame,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticSpatialCompositionDomainError {
    CameraOrLightMustRemainWorld,
    AnchorRequiresFixedOrientation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticManimCameraProfileError {
    AmbientMotionOwnsCamera,
    RequiresCamera3D,
    InvalidProfile,
}

impl std::fmt::Display for SemanticManimCameraProfileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AmbientMotionOwnsCamera => {
                formatter.write_str("stop ambient camera rotation before another camera edit")
            }
            Self::RequiresCamera3D => {
                formatter.write_str("camera profile requires a Camera3D object")
            }
            Self::InvalidProfile => {
                formatter.write_str("camera profile or clipping planes are invalid")
            }
        }
    }
}

impl std::error::Error for SemanticManimCameraProfileError {}

/// Optional authored spatial metadata. Ordinary objects pay no per-row storage;
/// cameras and explicitly shaded or composed objects share this one immutable
/// allocation. Removing a shared anchor root clears the root reference on each
/// surviving dependent object; its `None` anchor then denotes self-anchoring.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticSpatialProperties {
    camera_projection: Option<SemanticProjection3D>,
    camera_profile: Option<crate::ManimCamera3DProfile>,
    camera_motions: Option<std::sync::Arc<[crate::CameraAngularMotion]>>,
    material: SemanticSpatialMaterial,
    composition_domain: SemanticSpatialCompositionDomain,
    anchor_family: Option<crate::SemanticNodeId>,
    surface_uv_cell: Option<[usize; 2]>,
}

impl SemanticSpatialProperties {
    pub(crate) const fn new(
        camera_projection: Option<SemanticProjection3D>,
        camera_profile: Option<crate::ManimCamera3DProfile>,
        material: SemanticSpatialMaterial,
        composition_domain: SemanticSpatialCompositionDomain,
        anchor_family: Option<crate::SemanticNodeId>,
    ) -> Self {
        Self {
            camera_projection,
            camera_profile,
            camera_motions: None,
            material,
            composition_domain,
            anchor_family,
            surface_uv_cell: None,
        }
    }

    pub const fn camera_projection(&self) -> Option<SemanticProjection3D> {
        self.camera_projection
    }

    /// Canonical unwrapped Manim camera coordinates, if this camera was
    /// declared or last moved through the profile representation.
    pub const fn camera_profile(&self) -> Option<crate::ManimCamera3DProfile> {
        self.camera_profile
    }

    pub const fn material(&self) -> SemanticSpatialMaterial {
        self.material
    }

    pub const fn composition_domain(&self) -> SemanticSpatialCompositionDomain {
        self.composition_domain
    }

    /// Shared object/family whose effective bounds center anchors this
    /// FixedOrientation composition. `None` uses the object's own center.
    pub const fn anchor_family(&self) -> Option<crate::SemanticNodeId> {
        self.anchor_family
    }

    pub const fn surface_uv_cell(&self) -> Option<[usize; 2]> {
        self.surface_uv_cell
    }

    pub(crate) fn with_surface_uv_cell(mut self, cell: Option<[usize; 2]>) -> Self {
        self.surface_uv_cell = cell;
        self
    }

    pub fn camera_motions(&self) -> &[crate::CameraAngularMotion] {
        self.camera_motions.as_deref().unwrap_or(&[])
    }

    pub(crate) fn camera_motions_arc(
        &self,
    ) -> Option<std::sync::Arc<[crate::CameraAngularMotion]>> {
        self.camera_motions.clone()
    }

    pub(crate) fn with_camera_motions(
        mut self,
        motions: Option<std::sync::Arc<[crate::CameraAngularMotion]>>,
    ) -> Self {
        self.camera_motions = motions;
        self
    }

    pub const fn is_default(&self) -> bool {
        self.camera_projection.is_none()
            && self.camera_profile.is_none()
            && self.camera_motions.is_none()
            && matches!(self.material, SemanticSpatialMaterial::Unlit)
            && matches!(
                self.composition_domain,
                SemanticSpatialCompositionDomain::World
            )
            && self.anchor_family.is_none()
            && self.surface_uv_cell.is_none()
    }
}

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

    /// Construct and normalize a quaternion from `(w, x, y, z)` components.
    /// Zero and non-finite quaternions are rejected.
    pub fn from_components(w: f64, x: f64, y: f64, z: f64) -> Option<Self> {
        Self::normalized(w, x, y, z)
    }

    /// Accept already-normalized finite components without changing their bits.
    /// Use only at typed boundaries that promise to preserve a validated unit
    /// quaternion exactly; ordinary authoring should use `from_components`.
    pub fn from_validated_components(w: f64, x: f64, y: f64, z: f64) -> Option<Self> {
        let value = Self { w, x, y, z };
        value.is_valid().then_some(value)
    }

    /// Return normalized quaternion components in `(w, x, y, z)` order.
    pub const fn components(self) -> [f64; 4] {
        [self.w, self.x, self.y, self.z]
    }

    /// Whether the components form a finite unit quaternion.
    pub fn is_valid(self) -> bool {
        let norm = self.w.hypot(self.x).hypot(self.y).hypot(self.z);
        norm.is_finite() && (norm - 1.0).abs() <= 1.0e-12
    }

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

    pub fn inverse(self) -> Self {
        Self {
            w: self.w,
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }

    pub fn rotate_vector(self, value: SemanticVec3) -> Option<SemanticVec3> {
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

    /// Compose rotations as `self * rhs`; `rhs` acts on a vector first.
    pub fn compose(self, rhs: Self) -> Option<Self> {
        Self::normalized(
            self.w * rhs.w - self.x * rhs.x - self.y * rhs.y - self.z * rhs.z,
            self.w * rhs.x + self.x * rhs.w + self.y * rhs.z - self.z * rhs.y,
            self.w * rhs.y - self.x * rhs.z + self.y * rhs.w + self.z * rhs.x,
            self.w * rhs.z + self.x * rhs.y - self.y * rhs.x + self.z * rhs.w,
        )
    }

    /// Interpolate along the shortest quaternion arc for `t` in `[0, 1]`.
    /// Exact endpoints are returned unchanged; near-parallel inputs use nlerp.
    pub fn interpolate(self, target: Self, t: f64) -> Option<Self> {
        if !t.is_finite() || !(0.0..=1.0).contains(&t) {
            return None;
        }
        if t == 0.0 {
            return Some(self);
        }
        if t == 1.0 {
            return Some(target);
        }
        let mut end = target;
        let mut dot = self.w * end.w + self.x * end.x + self.y * end.y + self.z * end.z;
        if dot < 0.0 {
            end = Self {
                w: -end.w,
                x: -end.x,
                y: -end.y,
                z: -end.z,
            };
            dot = -dot;
        }
        dot = dot.clamp(-1.0, 1.0);
        let (a, b) = if dot > 0.9995 {
            (1.0 - t, t)
        } else {
            let theta = dot.acos();
            let sine = theta.sin();
            (((1.0 - t) * theta).sin() / sine, (t * theta).sin() / sine)
        };
        Self::normalized(
            a * self.w + b * end.w,
            a * self.x + b * end.x,
            a * self.y + b * end.y,
            a * self.z + b * end.z,
        )
    }
}

/// Multiply column-major 4x4 matrices (`left * right`).
fn matrix_multiply(left: [f64; 16], right: [f64; 16]) -> Option<[f64; 16]> {
    let mut result = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            result[column * 4 + row] = (0..4)
                .map(|k| left[k * 4 + row] * right[column * 4 + k])
                .sum();
        }
    }
    result.iter().all(|v| v.is_finite()).then_some(result)
}

fn rotation_matrix(rotation: SemanticRotation3D) -> [f64; 16] {
    let [w, x, y, z] = rotation.components();
    [
        1. - 2. * (y * y + z * z),
        2. * (x * y + w * z),
        2. * (x * z - w * y),
        0.,
        2. * (x * y - w * z),
        1. - 2. * (x * x + z * z),
        2. * (y * z + w * x),
        0.,
        2. * (x * z + w * y),
        2. * (y * z - w * x),
        1. - 2. * (x * x + y * y),
        0.,
        0.,
        0.,
        0.,
        1.,
    ]
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

    /// Place a local +Z axial mesh along a finite nonzero direction, without
    /// changing its geometry. Apply the local Z offset before orientation.
    ///
    /// Orientation is a +Y tilt followed by a +Z azimuth, as in Manim's
    /// Cylinder/Cone constructors. This also fixes the mesh's radial seam;
    /// a shortest-arc rotation has a different roll. Direction magnitude does
    /// not change scale. Zero/non-finite directions or offsets are rejected.
    pub fn from_axial_direction(direction: SemanticVec3, offset: f64) -> Option<Self> {
        if !direction.is_finite() || !offset.is_finite() {
            return None;
        }
        let largest = direction
            .x
            .abs()
            .max(direction.y.abs())
            .max(direction.z.abs());
        if largest == 0.0 {
            return None;
        }
        let (x, y, z) = (
            direction.x / largest,
            direction.y / largest,
            direction.z / largest,
        );
        let radial = x.hypot(y);
        let azimuth = if radial == 0.0 { 0.0 } else { y.atan2(x) };
        let tilt =
            SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 1.0, 0.0), radial.atan2(z))?;
        let rotation =
            SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 0.0, 1.0), azimuth)?
                .compose(tilt)?;
        Self::new(
            rotation.rotate_vector(SemanticVec3::new(0.0, 0.0, offset))?,
            rotation,
            SemanticVec3::new(1.0, 1.0, 1.0),
        )
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
        let result = add(self.rotation.rotate_vector(scaled)?, self.translation);
        result.is_finite().then_some(result)
    }

    /// Interpolate translation/scale linearly and orientation on its shortest arc.
    pub fn interpolate(self, target: Self, t: f64) -> Option<Self> {
        if !t.is_finite() || !(0.0..=1.0).contains(&t) {
            return None;
        }
        if t == 0.0 {
            return Some(self);
        }
        if t == 1.0 {
            return Some(target);
        }
        let lerp = |a: f64, b: f64| (1.0 - t) * a + t * b;
        Self::new(
            SemanticVec3::new(
                lerp(self.translation.x, target.translation.x),
                lerp(self.translation.y, target.translation.y),
                lerp(self.translation.z, target.translation.z),
            ),
            self.rotation.interpolate(target.rotation, t)?,
            SemanticVec3::new(
                lerp(self.scale.x, target.scale.x),
                lerp(self.scale.y, target.scale.y),
                lerp(self.scale.z, target.scale.z),
            ),
        )
    }

    /// Column-major `T * R * S` matrix, suitable for WGSL uniform upload.
    pub fn world_matrix(self) -> Option<[f64; 16]> {
        let mut scale = [0.; 16];
        scale[0] = self.scale.x;
        scale[5] = self.scale.y;
        scale[10] = self.scale.z;
        scale[15] = 1.;
        let mut translation = [0.; 16];
        translation[0] = 1.;
        translation[5] = 1.;
        translation[10] = 1.;
        translation[15] = 1.;
        translation[12] = self.translation.x;
        translation[13] = self.translation.y;
        translation[14] = self.translation.z;
        matrix_multiply(
            matrix_multiply(translation, rotation_matrix(self.rotation))?,
            scale,
        )
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

    fn matrix(self, aspect: f64) -> Option<[f64; 16]> {
        let [sx, sy, sz, offset] = self.coefficients(aspect)?;
        let mut m = [0.; 16];
        m[0] = sx;
        m[5] = sy;
        m[10] = sz;
        match self {
            Self::Perspective { .. } => {
                m[11] = -1.;
                m[14] = offset;
            }
            Self::Orthographic { .. } => {
                m[14] = offset;
                m[15] = 1.;
            }
        }
        Some(m)
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
            .rotate_vector(subtract(world, self.position))?;
        self.projection.clip(view, viewport_aspect)
    }

    /// Column-major `P * inverse(R) * T(-position)` view-projection matrix.
    /// Aspect is viewport width / height; values are computed in f64.
    pub fn camera_matrix(self, viewport_aspect: f64) -> Option<[f64; 16]> {
        let mut translation = [0.; 16];
        translation[0] = 1.;
        translation[5] = 1.;
        translation[10] = 1.;
        translation[15] = 1.;
        translation[12] = -self.position.x;
        translation[13] = -self.position.y;
        translation[14] = -self.position.z;
        let view = matrix_multiply(rotation_matrix(self.orientation.inverse()), translation)?;
        matrix_multiply(self.projection.matrix(viewport_aspect)?, view)
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

    #[test]
    fn axial_constructor_pose_preserves_direction_anchors_and_manim_radial_seam() {
        for direction in [
            SemanticVec3::new(1.0, 0.0, 0.0),
            SemanticVec3::new(-1.0, 0.0, 0.0),
            SemanticVec3::new(0.0, 1.0, 0.0),
            SemanticVec3::new(0.0, -1.0, 0.0),
            SemanticVec3::new(0.0, 0.0, 1.0),
            SemanticVec3::new(0.0, 0.0, -1.0),
            SemanticVec3::new(1.0, 2.0, -3.0),
            SemanticVec3::new(-2.0, -1.0, 3.0),
            SemanticVec3::new(f64::MAX, f64::MAX, f64::MAX),
            SemanticVec3::new(f64::MIN_POSITIVE, 0.0, 0.0),
        ] {
            let pose = SemanticWorldTransform3D::from_axial_direction(direction, -2.0).unwrap();
            let largest = direction
                .x
                .abs()
                .max(direction.y.abs())
                .max(direction.z.abs());
            let scaled = SemanticVec3::new(
                direction.x / largest,
                direction.y / largest,
                direction.z / largest,
            );
            let length = scaled.x.hypot(scaled.y).hypot(scaled.z);
            let axis = pose
                .rotation
                .rotate_vector(SemanticVec3::new(0.0, 0.0, 1.0))
                .unwrap();
            near(axis.x, scaled.x / length);
            near(axis.y, scaled.y / length);
            near(axis.z, scaled.z / length);
            let apex = pose
                .transform_point(SemanticVec3::new(0.0, 0.0, 2.0))
                .unwrap();
            near(apex.x, 0.0);
            near(apex.y, 0.0);
            near(apex.z, 0.0);
            assert_eq!(pose.scale, SemanticVec3::new(1.0, 1.0, 1.0));
        }
        // Pinned Manim Y-tilt/Z-azimuth convention, rather than any rotation
        // that merely maps +Z to the direction (which would miss radial roll).
        let pose =
            SemanticWorldTransform3D::from_axial_direction(SemanticVec3::new(1.0, 1.0, 1.0), 0.0)
                .unwrap();
        let radial = pose
            .rotation
            .rotate_vector(SemanticVec3::new(1.0, 0.0, 0.0))
            .unwrap();
        near(radial.x, 1.0 / 6.0_f64.sqrt());
        near(radial.y, 1.0 / 6.0_f64.sqrt());
        near(radial.z, -(2.0_f64 / 3.0).sqrt());
        let tangent = pose
            .rotation
            .rotate_vector(SemanticVec3::new(0.0, 1.0, 0.0))
            .unwrap();
        near(tangent.x, -std::f64::consts::FRAC_1_SQRT_2);
        near(tangent.y, std::f64::consts::FRAC_1_SQRT_2);
        near(tangent.z, 0.0);
        assert_eq!(
            SemanticWorldTransform3D::from_axial_direction(SemanticVec3::new(0.0, 0.0, 1.0), 0.0),
            Some(SemanticWorldTransform3D::IDENTITY)
        );
    }

    #[test]
    fn axial_constructor_pose_rejects_invalid_directions_and_offsets() {
        for direction in [
            SemanticVec3::ZERO,
            SemanticVec3::new(f64::NAN, 1.0, 0.0),
            SemanticVec3::new(0.0, f64::INFINITY, 1.0),
        ] {
            assert!(SemanticWorldTransform3D::from_axial_direction(direction, 0.0).is_none());
        }
        for offset in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(SemanticWorldTransform3D::from_axial_direction(
                SemanticVec3::new(0.0, 0.0, 1.0),
                offset
            )
            .is_none());
        }
    }

    fn near(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1.0e-12,
            "{actual} != {expected}"
        );
    }

    fn matrix_point(matrix: [f64; 16], point: SemanticVec3) -> SemanticClipPoint3D {
        let v = [point.x, point.y, point.z, 1.0];
        let mut out = [0.0; 4];
        for row in 0..4 {
            out[row] = (0..4)
                .map(|column| matrix[column * 4 + row] * v[column])
                .sum();
        }
        SemanticClipPoint3D {
            x: out[0],
            y: out[1],
            z: out[2],
            w: out[3],
        }
    }

    #[test]
    fn quaternion_construction_composition_and_shortest_interpolation_are_pinned() {
        assert!(SemanticRotation3D::from_components(0.0, 0.0, 0.0, 0.0).is_none());
        assert!(SemanticRotation3D::from_components(f64::NAN, 0.0, 0.0, 0.0).is_none());
        let x = SemanticRotation3D::from_axis_angle(
            SemanticVec3::new(1., 0., 0.),
            std::f64::consts::FRAC_PI_2,
        )
        .unwrap();
        let y = SemanticRotation3D::from_axis_angle(
            SemanticVec3::new(0., 1., 0.),
            std::f64::consts::FRAC_PI_2,
        )
        .unwrap();
        let v = SemanticVec3::new(0., 0., 1.);
        let composed = x.compose(y).unwrap().rotate_vector(v).unwrap();
        let sequential = x.rotate_vector(y.rotate_vector(v).unwrap()).unwrap();
        near(composed.x, sequential.x);
        near(composed.y, sequential.y);
        near(composed.z, sequential.z);
        let reverse = y.compose(x).unwrap().rotate_vector(v).unwrap();
        assert!(
            (reverse.x - composed.x).abs()
                + (reverse.y - composed.y).abs()
                + (reverse.z - composed.z).abs()
                > 0.5
        );

        let a = SemanticRotation3D::IDENTITY;
        let half_turn = SemanticRotation3D::from_axis_angle(
            SemanticVec3::new(0., 0., 1.),
            std::f64::consts::PI,
        )
        .unwrap();
        let midpoint = a
            .interpolate(half_turn, 0.5)
            .unwrap()
            .rotate_vector(SemanticVec3::new(1., 0., 0.))
            .unwrap();
        near(midpoint.x, 0.);
        near(midpoint.y, 1.);
        assert_eq!(a.interpolate(half_turn, 0.0), Some(a));
        assert_eq!(a.interpolate(half_turn, 1.0), Some(half_turn));
        assert!(a.interpolate(half_turn, f64::NAN).is_none());
        assert!(a.interpolate(half_turn, 1.1).is_none());
    }

    #[test]
    fn world_and_camera_matrices_match_projection_oracle() {
        let object = SemanticWorldTransform3D::new(
            SemanticVec3::new(2., -1., 3.),
            SemanticRotation3D::from_axis_angle(SemanticVec3::new(0., 0., 1.), 0.4).unwrap(),
            SemanticVec3::new(2., 0.5, 1.5),
        )
        .unwrap();
        let camera = SemanticCamera3D::new(
            SemanticVec3::new(0.5, 1., 8.),
            SemanticRotation3D::from_axis_angle(SemanticVec3::new(1., 0., 0.), -0.2).unwrap(),
            SemanticProjection3D::Perspective {
                vertical_fov_radians: 1.1,
                near: 0.5,
                far: 40.,
            },
        )
        .unwrap();
        let point = SemanticVec3::new(0.2, -0.7, 0.3);
        let aspect = 16. / 9.;
        let world_point = matrix_point(object.world_matrix().unwrap(), point);
        let expected_world = object.transform_point(point).unwrap();
        near(world_point.x, expected_world.x);
        near(world_point.y, expected_world.y);
        near(world_point.z, expected_world.z);
        let clip_matrix = matrix_point(
            matrix_multiply(
                camera.camera_matrix(aspect).unwrap(),
                object.world_matrix().unwrap(),
            )
            .unwrap(),
            point,
        );
        let clip_oracle = camera.project(object, point, aspect).unwrap();
        near(clip_matrix.x, clip_oracle.x);
        near(clip_matrix.y, clip_oracle.y);
        near(clip_matrix.z, clip_oracle.z);
        near(clip_matrix.w, clip_oracle.w);

        let ortho = SemanticCamera3D::new(
            SemanticVec3::ZERO,
            SemanticRotation3D::IDENTITY,
            SemanticProjection3D::Orthographic {
                height: 4.,
                near: 1.,
                far: 11.,
            },
        )
        .unwrap();
        let ortho_matrix = matrix_point(
            ortho.camera_matrix(2.).unwrap(),
            SemanticVec3::new(2., 1., -6.),
        );
        let ortho_oracle = ortho
            .project(
                SemanticWorldTransform3D::IDENTITY,
                SemanticVec3::new(2., 1., -6.),
                2.,
            )
            .unwrap();
        near(ortho_matrix.x, ortho_oracle.x);
        near(ortho_matrix.y, ortho_oracle.y);
        near(ortho_matrix.z, ortho_oracle.z);
        near(ortho_matrix.w, ortho_oracle.w);
        assert!(camera.camera_matrix(0.).is_none());
    }

    #[test]
    fn world_interpolation_handles_extreme_finite_opposite_endpoints() {
        let from = SemanticWorldTransform3D::new(
            SemanticVec3::new(-1.0e308, 0.0, 0.0),
            SemanticRotation3D::IDENTITY,
            SemanticVec3::new(1.0, 1.0, 1.0),
        )
        .unwrap();
        let to = SemanticWorldTransform3D::new(
            SemanticVec3::new(1.0e308, 0.0, 0.0),
            SemanticRotation3D::IDENTITY,
            SemanticVec3::new(1.0, 1.0, 1.0),
        )
        .unwrap();
        assert_eq!(from.interpolate(to, 0.5).unwrap().translation.x, 0.0);
        assert_eq!(from.interpolate(to, 0.0), Some(from));
        assert_eq!(from.interpolate(to, 1.0), Some(to));
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
        let rotated = rotation
            .rotate_vector(SemanticVec3::new(1.0, 0.0, 0.0))
            .unwrap();
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
