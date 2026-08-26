use crate::{Camera2DState, GeometryRef, Transform2D};

impl Camera2DState {
    /// Derive the renderer-facing viewport from an evaluated semantic camera frame.
    ///
    /// The initial moving-camera contract deliberately accepts only an unrotated
    /// rectangle. Translation and scale therefore flow through the ordinary object
    /// transform timeline, while unsupported camera rotation fails explicitly rather
    /// than being approximated differently by individual frontends.
    pub fn from_frame_object(geometry: &GeometryRef, transform: Transform2D) -> Option<Self> {
        let GeometryRef::Rectangle { size } = geometry else {
            return None;
        };
        if !transform.translation.x.is_finite()
            || !transform.translation.y.is_finite()
            || !transform.rotation.is_finite()
            || transform.rotation.abs() > 1.0e-6
            || !transform.scale.y.is_finite()
        {
            return None;
        }
        let height = size.y * transform.scale.y.abs();
        if !height.is_finite() || height <= 0.0 {
            return None;
        }
        Some(Self {
            center: transform.translation,
            height,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        Camera2DState, GeometryRef, Transform2D, Vec2, DEFAULT_FRAME_HEIGHT, DEFAULT_FRAME_WIDTH,
    };

    #[test]
    fn camera_state_is_derived_from_shared_frame_transform() {
        let geometry = GeometryRef::rectangle(DEFAULT_FRAME_WIDTH, DEFAULT_FRAME_HEIGHT);
        let moved = Transform2D {
            translation: Vec2::new(3.0, -2.0),
            scale: Vec2::new(0.5, 0.5),
            ..Transform2D::IDENTITY
        };
        let camera = Camera2DState::from_frame_object(&geometry, moved).unwrap();
        assert_eq!(camera.center, Vec2::new(3.0, -2.0));
        assert_eq!(camera.height, DEFAULT_FRAME_HEIGHT * 0.5);
    }

    #[test]
    fn camera_state_rejects_non_frame_and_rotated_geometry() {
        assert!(
            Camera2DState::from_frame_object(&GeometryRef::circle(1.0), Transform2D::IDENTITY,)
                .is_none()
        );
        assert!(Camera2DState::from_frame_object(
            &GeometryRef::rectangle(DEFAULT_FRAME_WIDTH, DEFAULT_FRAME_HEIGHT),
            Transform2D {
                rotation: 0.25,
                ..Transform2D::IDENTITY
            },
        )
        .is_none());
    }
}
