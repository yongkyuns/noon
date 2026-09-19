use crate::{Camera2DState, GeometryRef, Rect, Transform2D, Vec2};

impl Camera2DState {
    /// Derive axis-aligned world bounds for a render target aspect ratio.
    ///
    /// Camera height is the semantic vertical span. The horizontal span follows
    /// the render target aspect exactly, matching the renderer's camera contract.
    /// Returns `None` when either projected axis lacks finite, distinct f32 endpoints.
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
        // Widen before multiplying or halving: the final endpoints may be
        // representable even when a f32 intermediate product is not. Validate
        // after narrowing too, since translation can overflow an endpoint or
        // round both ends of a positive span to the same world coordinate.
        let half_height = f64::from(self.height) * 0.5;
        let half_width = half_height * f64::from(aspect);
        let center_x = f64::from(self.center.x);
        let center_y = f64::from(self.center.y);
        let min = Vec2::new(
            (center_x - half_width) as f32,
            (center_y - half_height) as f32,
        );
        let max = Vec2::new(
            (center_x + half_width) as f32,
            (center_y + half_height) as f32,
        );
        if !min.x.is_finite()
            || !min.y.is_finite()
            || !max.x.is_finite()
            || !max.y.is_finite()
            || min.x >= max.x
            || min.y >= max.y
        {
            return None;
        }
        Some(Rect::new(min, max))
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

    #[test]
    fn viewport_projection_avoids_intermediate_width_overflow() {
        let camera = Camera2DState {
            center: Vec2::ZERO,
            height: f32::MAX,
        };
        let bounds = camera.viewport_bounds(1.5).unwrap();
        let half_width = (f64::from(f32::MAX) * 0.75) as f32;
        assert_eq!(bounds.min, Vec2::new(-half_width, -f32::MAX * 0.5));
        assert_eq!(bounds.max, Vec2::new(half_width, f32::MAX * 0.5));
    }

    #[test]
    fn viewport_projection_rejects_overflow_after_translation() {
        for center in [
            Vec2::new(f32::MAX, 0.0),
            Vec2::new(-f32::MAX, 0.0),
            Vec2::new(0.0, f32::MAX),
            Vec2::new(0.0, -f32::MAX),
        ] {
            let camera = Camera2DState {
                center,
                height: f32::MAX,
            };
            assert!(camera.viewport_bounds(1.0).is_none(), "{center:?}");
        }
    }

    #[test]
    fn viewport_projection_rejects_unrepresentable_width() {
        let camera = Camera2DState {
            center: Vec2::ZERO,
            height: f32::MAX,
        };
        assert!(camera.viewport_bounds(f32::MAX).is_none());
    }

    #[test]
    fn viewport_projection_rejects_underflowed_height() {
        let camera = Camera2DState {
            center: Vec2::ZERO,
            height: f32::from_bits(1),
        };
        assert!(camera.viewport_bounds(1.0).is_none());
    }

    #[test]
    fn viewport_projection_rejects_underflowed_width() {
        let camera = Camera2DState {
            center: Vec2::ZERO,
            height: 1.0,
        };
        assert!(camera.viewport_bounds(f32::from_bits(1)).is_none());
    }

    #[test]
    fn viewport_projection_rejects_spans_lost_at_large_centers() {
        for center in [
            Vec2::new(16_777_216.0, 0.0),
            Vec2::new(-16_777_216.0, 0.0),
            Vec2::new(0.0, 16_777_216.0),
            Vec2::new(0.0, -16_777_216.0),
        ] {
            let camera = Camera2DState {
                center,
                height: 1.0,
            };
            assert!(camera.viewport_bounds(1.0).is_none(), "{center:?}");
        }
    }

    #[test]
    fn viewport_projection_accepts_representable_subnormal_endpoints() {
        let camera = Camera2DState {
            center: Vec2::ZERO,
            height: f32::MIN_POSITIVE,
        };
        let bounds = camera.viewport_bounds(0.5).unwrap();
        assert_eq!(bounds.min.x, -f32::MIN_POSITIVE * 0.25);
        assert_eq!(bounds.max.x, f32::MIN_POSITIVE * 0.25);
        assert_eq!(bounds.min.y, -f32::MIN_POSITIVE * 0.5);
        assert_eq!(bounds.max.y, f32::MIN_POSITIVE * 0.5);
    }

    #[test]
    fn viewport_projection_accepts_resolvable_spans_at_large_centers() {
        let center = Vec2::new(16_777_216.0, -16_777_216.0);
        let camera = Camera2DState {
            center,
            height: 4.0,
        };
        let bounds = camera.viewport_bounds(1.0).unwrap();
        assert_eq!(bounds.min, center - Vec2::new(2.0, 2.0));
        assert_eq!(bounds.max, center + Vec2::new(2.0, 2.0));
    }

    #[test]
    fn viewport_projection_rejects_invalid_camera_and_aspect_inputs() {
        for invalid in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(Camera2DState::default().viewport_bounds(invalid).is_none());
            let camera = Camera2DState {
                height: invalid,
                ..Camera2DState::default()
            };
            assert!(camera.viewport_bounds(1.0).is_none());
        }
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for center in [Vec2::new(invalid, 0.0), Vec2::new(0.0, invalid)] {
                let camera = Camera2DState {
                    center,
                    ..Camera2DState::default()
                };
                assert!(camera.viewport_bounds(1.0).is_none());
            }
        }
    }
}
