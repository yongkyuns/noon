use crate::{Camera2DState, GeometryRef, Rect, Transform2D, Vec2};

impl Camera2DState {
    /// Derive axis-aligned world bounds for a render target aspect ratio.
    ///
    /// Camera height is the semantic vertical span. The horizontal span follows
    /// the render target aspect exactly, matching the renderer's camera contract.
    pub fn viewport_bounds(self, aspect: f32) -> Option<Rect> {
        if !self.center.x.is_finite()
            || !self.center.y.is_finite()
            || !self.height.is_finite()
            || self.height <= 0.0
            || !aspect.is_finite()
            || aspect <= 0.0
        {
            return None;
        }
        let half_extent = Vec2::new(self.height * aspect * 0.5, self.height * 0.5);
        if !half_extent.x.is_finite() || !half_extent.y.is_finite() {
            return None;
        }
        Some(Rect::new(
            self.center - half_extent,
            self.center + half_extent,
        ))
    }

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
    fn camera_state_derives_aspect_correct_world_bounds() {
        let camera = Camera2DState {
            center: Vec2::new(3.0, -1.0),
            height: 8.0,
        };
        let bounds = camera.viewport_bounds(2.0).unwrap();
        assert_eq!(bounds.min, Vec2::new(-5.0, -5.0));
        assert_eq!(bounds.max, Vec2::new(11.0, 3.0));
        assert!(camera.viewport_bounds(0.0).is_none());
        assert!(camera.viewport_bounds(f32::NAN).is_none());
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
