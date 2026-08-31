use serde::Serialize;

use crate::{SemanticTransform2_5D, SemanticVec3, Transform2D};

const QUATERNION_EPSILON: f64 = 1.0e-12;
const PROJECTION_EPSILON: f64 = 1.0e-12;

/// Canonical unit quaternion used by retained 3D world transforms.
///
/// `q` and `-q` represent the same rotation. Construction normalizes the value
/// and chooses one deterministic sign so serialization and resource identity do
/// not depend on which equivalent quaternion a frontend happened to provide.
/// Deserialization is intentionally absent until a validated wire constructor is
/// added; unchecked wire data must not bypass canonicalization.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct SemanticQuaternion {
    w: f64,
    x: f64,
    y: f64,
    z: f64,
}

impl SemanticQuaternion {
    pub const IDENTITY: Self = Self {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub fn normalized(w: f64, x: f64, y: f64, z: f64) -> Result<Self, ThreeDSemanticError> {
        if !w.is_finite() || !x.is_finite() || !y.is_finite() || !z.is_finite() {
            return Err(ThreeDSemanticError::NonFiniteQuaternion { w, x, y, z });
        }
        let norm = (w * w + x * x + y * y + z * z).sqrt();
        if !norm.is_finite() || norm <= QUATERNION_EPSILON {
            return Err(ThreeDSemanticError::DegenerateQuaternion);
        }

        let mut value = Self {
            w: w / norm,
            x: x / norm,
            y: y / norm,
            z: z / norm,
        };
        if quaternion_needs_sign_flip(value) {
            value.w = -value.w;
            value.x = -value.x;
            value.y = -value.y;
            value.z = -value.z;
        }
        Ok(value)
    }

    pub fn from_z_rotation(angle: f64) -> Result<Self, ThreeDSemanticError> {
        if !angle.is_finite() {
            return Err(ThreeDSemanticError::NonFiniteAngle(angle));
        }
        let half = angle * 0.5;
        Self::normalized(half.cos(), 0.0, 0.0, half.sin())
    }

    pub const fn w(self) -> f64 {
        self.w
    }

    pub const fn x(self) -> f64 {
        self.x
    }

    pub const fn y(self) -> f64 {
        self.y
    }

    pub const fn z(self) -> f64 {
        self.z
    }

    pub fn rotate_vector(self, value: SemanticVec3) -> Result<SemanticVec3, ThreeDSemanticError> {
        validate_vec3(value)?;
        let axis = SemanticVec3::new(self.x, self.y, self.z);
        let first = cross(axis, value);
        let second = cross(axis, first);
        Ok(add3(
            value,
            add3(scale3(first, 2.0 * self.w), scale3(second, 2.0)),
        ))
    }
}

impl Default for SemanticQuaternion {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// Renderer-independent affine 3D transform.
///
/// The order is scale, then rotate, then translate, matching existing
/// `Transform2D::transform_point` semantics for the zero-z compatibility path.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct WorldTransform {
    translation: SemanticVec3,
    rotation: SemanticQuaternion,
    scale: SemanticVec3,
}

impl WorldTransform {
    pub const IDENTITY: Self = Self {
        translation: SemanticVec3::ZERO,
        rotation: SemanticQuaternion::IDENTITY,
        scale: SemanticVec3::new(1.0, 1.0, 1.0),
    };

    pub fn new(
        translation: SemanticVec3,
        rotation: SemanticQuaternion,
        scale: SemanticVec3,
    ) -> Result<Self, ThreeDSemanticError> {
        validate_vec3(translation)?;
        validate_vec3(scale)?;
        let rotation =
            SemanticQuaternion::normalized(rotation.w, rotation.x, rotation.y, rotation.z)?;
        Ok(Self {
            translation,
            rotation,
            scale,
        })
    }

    /// Lossless semantic lift of the existing 2D transform contract.
    pub fn from_transform_2d(value: Transform2D) -> Result<Self, ThreeDSemanticError> {
        Self::new(
            SemanticVec3::new(
                f64::from(value.translation.x),
                f64::from(value.translation.y),
                0.0,
            ),
            SemanticQuaternion::from_z_rotation(f64::from(value.rotation))?,
            SemanticVec3::new(f64::from(value.scale.x), f64::from(value.scale.y), 1.0),
        )
    }

    /// Lossless semantic lift of the current f64 2.5D authoring transform.
    pub fn from_transform_2_5d(value: SemanticTransform2_5D) -> Result<Self, ThreeDSemanticError> {
        Self::new(
            value.translation,
            SemanticQuaternion::from_z_rotation(value.rotation_z)?,
            value.scale,
        )
    }

    pub const fn translation(self) -> SemanticVec3 {
        self.translation
    }

    pub const fn rotation(self) -> SemanticQuaternion {
        self.rotation
    }

    pub const fn scale(self) -> SemanticVec3 {
        self.scale
    }

    pub fn transform_point(self, point: SemanticVec3) -> Result<SemanticVec3, ThreeDSemanticError> {
        validate_vec3(point)?;
        let scaled = component_mul3(point, self.scale);
        let rotated = self.rotation.rotate_vector(scaled)?;
        Ok(add3(rotated, self.translation))
    }
}

impl Default for WorldTransform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// ManimCE v0.21-compatible retained 3D camera state for the first parity tranche.
///
/// Manim's Cairo 3D camera exposes `phi`, `theta`, `gamma`, focal distance,
/// frame center, and zoom. Keeping those observable parameters authoritative
/// avoids prematurely baking a renderer matrix convention into scene semantics.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Camera3DState {
    frame_center: SemanticVec3,
    phi: f64,
    theta: f64,
    gamma: f64,
    focal_distance: f64,
    zoom: f64,
    exponential_projection: bool,
}

impl Camera3DState {
    pub fn new(
        frame_center: SemanticVec3,
        phi: f64,
        theta: f64,
        gamma: f64,
        focal_distance: f64,
        zoom: f64,
        exponential_projection: bool,
    ) -> Result<Self, ThreeDSemanticError> {
        validate_vec3(frame_center)?;
        for angle in [phi, theta, gamma] {
            if !angle.is_finite() {
                return Err(ThreeDSemanticError::NonFiniteAngle(angle));
            }
        }
        if !focal_distance.is_finite() || focal_distance <= 0.0 {
            return Err(ThreeDSemanticError::InvalidFocalDistance(focal_distance));
        }
        if !zoom.is_finite() {
            return Err(ThreeDSemanticError::InvalidZoom(zoom));
        }
        Ok(Self {
            frame_center,
            phi,
            theta,
            gamma,
            focal_distance,
            zoom,
            exponential_projection,
        })
    }

    pub const fn frame_center(self) -> SemanticVec3 {
        self.frame_center
    }

    pub const fn phi(self) -> f64 {
        self.phi
    }

    pub const fn theta(self) -> f64 {
        self.theta
    }

    pub const fn gamma(self) -> f64 {
        self.gamma
    }

    pub const fn focal_distance(self) -> f64 {
        self.focal_distance
    }

    pub const fn zoom(self) -> f64 {
        self.zoom
    }

    pub const fn exponential_projection(self) -> bool {
        self.exponential_projection
    }

    /// Apply ManimCE v0.21's `generate_rotation_matrix` convention.
    ///
    /// For column vectors this is `Rz(gamma) * Rx(-phi) *
    /// Rz(-theta - pi/2) * (point - frame_center)`.
    pub fn world_to_camera(self, point: SemanticVec3) -> Result<SemanticVec3, ThreeDSemanticError> {
        validate_vec3(point)?;
        let centered = sub3(point, self.frame_center);
        let azimuth = rotate_z(centered, -self.theta - std::f64::consts::FRAC_PI_2);
        let polar = rotate_x(azimuth, -self.phi);
        Ok(rotate_z(polar, self.gamma))
    }

    /// Apply the observable ManimCE v0.21 perspective projection.
    ///
    /// X/Y are scaled by focal-distance perspective and zoom while camera-space
    /// Z is retained. The exact singular focal plane is rejected explicitly;
    /// points behind it follow Manim's finite `1e6` fallback.
    pub fn project_point(self, point: SemanticVec3) -> Result<SemanticVec3, ThreeDSemanticError> {
        let camera = self.world_to_camera(point)?;
        let denominator = self.focal_distance - camera.z;
        let factor = if self.exponential_projection {
            if camera.z < 0.0 {
                self.focal_distance / denominator
            } else {
                (camera.z / self.focal_distance).exp()
            }
        } else if denominator < 0.0 {
            1.0e6
        } else if denominator.abs() <= PROJECTION_EPSILON {
            return Err(ThreeDSemanticError::ProjectionSingularity {
                camera_z: camera.z,
                focal_distance: self.focal_distance,
            });
        } else {
            self.focal_distance / denominator
        };
        let projected = SemanticVec3::new(
            camera.x * factor * self.zoom,
            camera.y * factor * self.zoom,
            camera.z,
        );
        validate_vec3(projected)?;
        Ok(projected)
    }
}

impl Default for Camera3DState {
    fn default() -> Self {
        // ManimCE v0.21 ThreeDCamera defaults: phi=0, theta=-90deg,
        // gamma=0, focal_distance=20, zoom=1, frame_center=ORIGIN.
        Self {
            frame_center: SemanticVec3::ZERO,
            phi: 0.0,
            theta: -std::f64::consts::FRAC_PI_2,
            gamma: 0.0,
            focal_distance: 20.0,
            zoom: 1.0,
            exponential_projection: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Bounds3D64 {
    pub min: SemanticVec3,
    pub max: SemanticVec3,
}

impl Bounds3D64 {
    pub const fn point(value: SemanticVec3) -> Self {
        Self {
            min: value,
            max: value,
        }
    }

    pub fn include(&mut self, value: SemanticVec3) {
        self.min.x = self.min.x.min(value.x);
        self.min.y = self.min.y.min(value.y);
        self.min.z = self.min.z.min(value.z);
        self.max.x = self.max.x.max(value.x);
        self.max.y = self.max.y.max(value.y);
        self.max.z = self.max.z.max(value.z);
    }
}

/// Immutable indexed-triangle resource content for retained 3D rendering.
///
/// Like the canonical transform/camera types, this is serializable but not
/// directly deserializable yet. A future wire adapter must call `new` so finite
/// values, topology, and derived bounds are revalidated on ingress.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MeshResource {
    positions: Vec<SemanticVec3>,
    normals: Vec<SemanticVec3>,
    indices: Vec<u32>,
    local_bounds: Bounds3D64,
}

impl MeshResource {
    pub fn new(
        positions: Vec<SemanticVec3>,
        normals: Vec<SemanticVec3>,
        indices: Vec<u32>,
    ) -> Result<Self, MeshResourceError> {
        if positions.is_empty() {
            return Err(MeshResourceError::EmptyPositions);
        }
        if normals.len() != positions.len() {
            return Err(MeshResourceError::NormalCountMismatch {
                positions: positions.len(),
                normals: normals.len(),
            });
        }
        if !indices.len().is_multiple_of(3) {
            return Err(MeshResourceError::IndexCountNotTriangles(indices.len()));
        }

        for (index, position) in positions.iter().copied().enumerate() {
            if !position.is_finite() {
                return Err(MeshResourceError::NonFinitePosition { index, position });
            }
        }
        for (index, normal) in normals.iter().copied().enumerate() {
            if !normal.is_finite() {
                return Err(MeshResourceError::NonFiniteNormal { index, normal });
            }
        }
        for (offset, vertex) in indices.iter().copied().enumerate() {
            if vertex as usize >= positions.len() {
                return Err(MeshResourceError::IndexOutOfBounds {
                    offset,
                    vertex,
                    vertex_count: positions.len(),
                });
            }
        }

        let mut local_bounds = Bounds3D64::point(positions[0]);
        for position in positions.iter().copied().skip(1) {
            local_bounds.include(position);
        }
        Ok(Self {
            positions,
            normals,
            indices,
            local_bounds,
        })
    }

    pub fn positions(&self) -> &[SemanticVec3] {
        &self.positions
    }

    pub fn normals(&self) -> &[SemanticVec3] {
        &self.normals
    }

    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    pub const fn local_bounds(&self) -> Bounds3D64 {
        self.local_bounds
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ThreeDSemanticError {
    NonFiniteVector(SemanticVec3),
    NonFiniteQuaternion { w: f64, x: f64, y: f64, z: f64 },
    DegenerateQuaternion,
    NonFiniteAngle(f64),
    InvalidFocalDistance(f64),
    InvalidZoom(f64),
    ProjectionSingularity { camera_z: f64, focal_distance: f64 },
}

impl std::fmt::Display for ThreeDSemanticError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::NonFiniteVector(value) => write!(
                formatter,
                "3D semantic vector must be finite: ({}, {}, {})",
                value.x, value.y, value.z
            ),
            Self::NonFiniteQuaternion { w, x, y, z } => write!(
                formatter,
                "3D semantic quaternion must be finite: ({w}, {x}, {y}, {z})"
            ),
            Self::DegenerateQuaternion => formatter.write_str("3D semantic quaternion is zero"),
            Self::NonFiniteAngle(value) => {
                write!(formatter, "3D camera angle must be finite: {value}")
            }
            Self::InvalidFocalDistance(value) => write!(
                formatter,
                "3D focal distance must be finite and positive: {value}"
            ),
            Self::InvalidZoom(value) => write!(formatter, "3D zoom must be finite: {value}"),
            Self::ProjectionSingularity {
                camera_z,
                focal_distance,
            } => write!(
                formatter,
                "3D point lies on focal plane: camera_z={camera_z}, focal_distance={focal_distance}"
            ),
        }
    }
}

impl std::error::Error for ThreeDSemanticError {}

#[derive(Clone, Debug, PartialEq)]
pub enum MeshResourceError {
    EmptyPositions,
    NormalCountMismatch {
        positions: usize,
        normals: usize,
    },
    IndexCountNotTriangles(usize),
    NonFinitePosition {
        index: usize,
        position: SemanticVec3,
    },
    NonFiniteNormal {
        index: usize,
        normal: SemanticVec3,
    },
    IndexOutOfBounds {
        offset: usize,
        vertex: u32,
        vertex_count: usize,
    },
}

impl std::fmt::Display for MeshResourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyPositions => {
                formatter.write_str("mesh resource requires at least one position")
            }
            Self::NormalCountMismatch { positions, normals } => write!(
                formatter,
                "mesh normal count {normals} does not match position count {positions}"
            ),
            Self::IndexCountNotTriangles(count) => {
                write!(
                    formatter,
                    "mesh index count must be divisible by three: {count}"
                )
            }
            Self::NonFinitePosition { index, position } => write!(
                formatter,
                "mesh position {index} is non-finite: ({}, {}, {})",
                position.x, position.y, position.z
            ),
            Self::NonFiniteNormal { index, normal } => write!(
                formatter,
                "mesh normal {index} is non-finite: ({}, {}, {})",
                normal.x, normal.y, normal.z
            ),
            Self::IndexOutOfBounds {
                offset,
                vertex,
                vertex_count,
            } => write!(
                formatter,
                "mesh index {offset} references vertex {vertex}, but vertex count is {vertex_count}"
            ),
        }
    }
}

impl std::error::Error for MeshResourceError {}

fn quaternion_needs_sign_flip(value: SemanticQuaternion) -> bool {
    value.w < 0.0
        || (value.w == 0.0
            && (value.x < 0.0
                || (value.x == 0.0 && (value.y < 0.0 || (value.y == 0.0 && value.z < 0.0)))))
}

fn validate_vec3(value: SemanticVec3) -> Result<(), ThreeDSemanticError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(ThreeDSemanticError::NonFiniteVector(value))
    }
}

fn add3(lhs: SemanticVec3, rhs: SemanticVec3) -> SemanticVec3 {
    SemanticVec3::new(lhs.x + rhs.x, lhs.y + rhs.y, lhs.z + rhs.z)
}

fn sub3(lhs: SemanticVec3, rhs: SemanticVec3) -> SemanticVec3 {
    SemanticVec3::new(lhs.x - rhs.x, lhs.y - rhs.y, lhs.z - rhs.z)
}

fn scale3(value: SemanticVec3, factor: f64) -> SemanticVec3 {
    SemanticVec3::new(value.x * factor, value.y * factor, value.z * factor)
}

fn component_mul3(lhs: SemanticVec3, rhs: SemanticVec3) -> SemanticVec3 {
    SemanticVec3::new(lhs.x * rhs.x, lhs.y * rhs.y, lhs.z * rhs.z)
}

fn cross(lhs: SemanticVec3, rhs: SemanticVec3) -> SemanticVec3 {
    SemanticVec3::new(
        lhs.y * rhs.z - lhs.z * rhs.y,
        lhs.z * rhs.x - lhs.x * rhs.z,
        lhs.x * rhs.y - lhs.y * rhs.x,
    )
}

fn rotate_x(value: SemanticVec3, angle: f64) -> SemanticVec3 {
    let (sin, cos) = angle.sin_cos();
    SemanticVec3::new(
        value.x,
        value.y * cos - value.z * sin,
        value.y * sin + value.z * cos,
    )
}

fn rotate_z(value: SemanticVec3, angle: f64) -> SemanticVec3 {
    let (sin, cos) = angle.sin_cos();
    SemanticVec3::new(
        value.x * cos - value.y * sin,
        value.x * sin + value.y * cos,
        value.z,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vec2;

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 1.0e-6,
            "expected {expected}, got {actual}"
        );
    }

    fn assert_vec3(actual: SemanticVec3, expected: SemanticVec3) {
        assert_close(actual.x, expected.x);
        assert_close(actual.y, expected.y);
        assert_close(actual.z, expected.z);
    }

    #[test]
    fn quaternion_normalization_has_canonical_sign() {
        let a = SemanticQuaternion::normalized(2.0, 0.0, 0.0, 2.0).unwrap();
        let b = SemanticQuaternion::normalized(-2.0, 0.0, 0.0, -2.0).unwrap();
        assert_eq!(a, b);
        assert_close(a.w(), std::f64::consts::FRAC_1_SQRT_2);
        assert_close(a.z(), std::f64::consts::FRAC_1_SQRT_2);
    }

    #[test]
    fn world_transform_preserves_existing_2d_transform_order() {
        let legacy = Transform2D {
            translation: Vec2::new(3.0, -2.0),
            rotation: std::f32::consts::FRAC_PI_2,
            scale: Vec2::new(2.0, 4.0),
        };
        let world = WorldTransform::from_transform_2d(legacy).unwrap();
        let point = Vec2::new(1.0, 0.5);
        let expected = legacy.transform_point(point);
        let actual = world
            .transform_point(SemanticVec3::new(
                f64::from(point.x),
                f64::from(point.y),
                0.0,
            ))
            .unwrap();
        assert_close(actual.x, f64::from(expected.x));
        assert_close(actual.y, f64::from(expected.y));
        assert_close(actual.z, 0.0);
    }

    #[test]
    fn default_camera_matches_manim_identity_orientation_and_perspective() {
        let camera = Camera3DState::default();
        let camera_point = camera
            .world_to_camera(SemanticVec3::new(1.0, 2.0, 3.0))
            .unwrap();
        assert_vec3(camera_point, SemanticVec3::new(1.0, 2.0, 3.0));

        let projected = camera
            .project_point(SemanticVec3::new(1.0, 2.0, 3.0))
            .unwrap();
        let factor = 20.0 / 17.0;
        assert_vec3(projected, SemanticVec3::new(factor, 2.0 * factor, 3.0));
    }

    #[test]
    fn camera_phi_uses_manim_right_axis_rotation_convention() {
        let camera = Camera3DState::new(
            SemanticVec3::ZERO,
            std::f64::consts::FRAC_PI_2,
            -std::f64::consts::FRAC_PI_2,
            0.0,
            20.0,
            1.0,
            false,
        )
        .unwrap();
        assert_vec3(
            camera
                .world_to_camera(SemanticVec3::new(0.0, 0.0, 1.0))
                .unwrap(),
            SemanticVec3::new(0.0, 1.0, 0.0),
        );
    }

    #[test]
    fn mesh_resource_validates_topology_and_computes_bounds() {
        let positions = vec![
            SemanticVec3::new(-2.0, 1.0, 3.0),
            SemanticVec3::new(4.0, -1.0, 2.0),
            SemanticVec3::new(0.0, 5.0, -3.0),
        ];
        let normals = vec![SemanticVec3::new(0.0, 0.0, 1.0); 3];
        let mesh = MeshResource::new(positions.clone(), normals, vec![0, 1, 2]).unwrap();
        assert_eq!(mesh.positions(), positions);
        assert_eq!(mesh.indices(), &[0, 1, 2]);
        assert_eq!(
            mesh.local_bounds(),
            Bounds3D64 {
                min: SemanticVec3::new(-2.0, -1.0, -3.0),
                max: SemanticVec3::new(4.0, 5.0, 3.0),
            }
        );
    }

    #[test]
    fn mesh_resource_rejects_bad_indices_before_renderer_installation() {
        let positions = vec![SemanticVec3::ZERO; 3];
        let normals = vec![SemanticVec3::new(0.0, 0.0, 1.0); 3];
        assert!(matches!(
            MeshResource::new(positions, normals, vec![0, 1, 3]),
            Err(MeshResourceError::IndexOutOfBounds { vertex: 3, .. })
        ));
    }
}
