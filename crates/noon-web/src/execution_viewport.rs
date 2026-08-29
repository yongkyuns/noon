use noon_core::Camera2DState;

use crate::{ExecutionVisibilityEnvelope, ExecutionVisibilityError, ScenePlayer};

#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionViewportError {
    InvalidCameraState,
    InvalidAspectRatio(f32),
    Visibility(ExecutionVisibilityError),
}

impl std::fmt::Display for ExecutionViewportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCameraState => formatter.write_str("invalid execution viewport camera state"),
            Self::InvalidAspectRatio(aspect_ratio) => write!(
                formatter,
                "invalid execution viewport aspect ratio {aspect_ratio}",
            ),
            Self::Visibility(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ExecutionViewportError {}

impl From<ExecutionVisibilityError> for ExecutionViewportError {
    fn from(value: ExecutionVisibilityError) -> Self {
        Self::Visibility(value)
    }
}

impl ScenePlayer {
    /// Query the authoritative retained spatial index for one camera viewport.
    ///
    /// `Camera2DState::height` remains the semantic world-space camera height;
    /// the render surface contributes only its width/height aspect ratio. This
    /// keeps world-space viewport derivation in Rust next to the retained query
    /// owner instead of duplicating camera math in browser workers.
    pub fn viewport_visibility_for_camera(
        &self,
        camera: Camera2DState,
        aspect_ratio: f32,
    ) -> Result<ExecutionVisibilityEnvelope, ExecutionViewportError> {
        if !camera.center.x.is_finite()
            || !camera.center.y.is_finite()
            || !camera.height.is_finite()
            || camera.height <= 0.0
        {
            return Err(ExecutionViewportError::InvalidCameraState);
        }
        if !aspect_ratio.is_finite() || aspect_ratio <= 0.0 {
            return Err(ExecutionViewportError::InvalidAspectRatio(aspect_ratio));
        }

        let half_height = camera.height * 0.5;
        let half_width = half_height * aspect_ratio;
        Ok(self.viewport_visibility(
            camera.center.x - half_width,
            camera.center.y - half_height,
            camera.center.x + half_width,
            camera.center.y + half_height,
        ))
    }

    pub fn viewport_visibility_for_camera_json(
        &self,
        camera: Camera2DState,
        aspect_ratio: f32,
    ) -> Result<String, ExecutionViewportError> {
        self.viewport_visibility_for_camera(camera, aspect_ratio)?
            .to_json()
            .map_err(ExecutionViewportError::from)
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{Camera2DState, GeometryRef, SceneDefinition, Transform2D, Vec2};
    use noon_ir::encode_scene;

    use super::*;

    fn spaced_scene() -> ScenePlayer {
        let mut scene = SceneDefinition::new();
        scene.add(GeometryRef::circle(0.1));

        let horizontal = scene.add(GeometryRef::circle(0.1));
        scene.object_mut(horizontal).unwrap().transform = Transform2D {
            translation: Vec2::new(6.0, 0.0),
            ..Transform2D::IDENTITY
        };

        let vertical = scene.add(GeometryRef::circle(0.1));
        scene.object_mut(vertical).unwrap().transform = Transform2D {
            translation: Vec2::new(0.0, 6.0),
            ..Transform2D::IDENTITY
        };

        ScenePlayer::from_scene_json(&encode_scene(&scene).unwrap()).unwrap()
    }

    #[test]
    fn surface_aspect_expands_only_camera_world_width() {
        let player = spaced_scene();
        let camera = Camera2DState {
            center: Vec2::ZERO,
            height: 8.0,
        };

        let square = player.viewport_visibility_for_camera(camera, 1.0).unwrap();
        assert_eq!(square.total_live, 3);
        assert_eq!(square.stats.results, 1);
        assert_eq!(square.slots.len(), 1);
        assert_eq!(square.slots[0].slot, 0);

        let wide = player.viewport_visibility_for_camera(camera, 2.0).unwrap();
        assert_eq!(wide.total_live, 3);
        assert_eq!(wide.stats.results, 2);
        assert_eq!(wide.slots.len(), 2);
        assert_eq!(wide.slots[0].slot, 0);
        assert_eq!(wide.slots[1].slot, 1);
        assert_eq!(wide.stats.full_scan_fallbacks, 0);
    }

    #[test]
    fn camera_translation_moves_the_retained_viewport_query() {
        let player = spaced_scene();
        let camera = Camera2DState {
            center: Vec2::new(6.0, 0.0),
            height: 4.0,
        };

        let visibility = player.viewport_visibility_for_camera(camera, 1.0).unwrap();
        assert_eq!(visibility.stats.results, 1);
        assert_eq!(visibility.slots[0].slot, 1);
    }

    #[test]
    fn invalid_camera_or_surface_aspect_is_rejected_before_query() {
        let player = spaced_scene();
        let camera = Camera2DState::default();

        assert!(matches!(
            player.viewport_visibility_for_camera(camera, 0.0),
            Err(ExecutionViewportError::InvalidAspectRatio(0.0))
        ));
        assert!(matches!(
            player.viewport_visibility_for_camera(camera, f32::NAN),
            Err(ExecutionViewportError::InvalidAspectRatio(value)) if value.is_nan()
        ));
        assert!(matches!(
            player.viewport_visibility_for_camera(
                Camera2DState {
                    center: Vec2::ZERO,
                    height: 0.0,
                },
                1.0,
            ),
            Err(ExecutionViewportError::InvalidCameraState)
        ));
    }

    #[test]
    fn camera_visibility_json_preserves_transport_metadata() {
        let player = spaced_scene();
        let json = player
            .viewport_visibility_for_camera_json(Camera2DState::default(), 1.0)
            .unwrap();
        let decoded = ExecutionVisibilityEnvelope::from_json(&json).unwrap();

        assert_eq!(decoded.time, player.frame().time);
        assert_eq!(decoded.layout_generation, player.layout_generation());
        assert_eq!(decoded.total_live, player.object_count());
        assert_eq!(decoded.stats.full_scan_fallbacks, 0);
    }
}
