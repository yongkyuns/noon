//! Pure composition of inspection navigation with the current execution camera.
//!
//! This value stores an adjustment, never a second camera or a captured scene.
//! The interactive session must own its lifetime and revision; hosts only consume
//! the resolved camera. Input admission, gesture cancellation, presentation
//! receipts and renderer delivery are deliberately not implemented by this value.

use noon_core::{Camera2DState, Vec2};

/// Constant-size, camera-relative inspection adjustment.
///
/// The effective view center is `camera.center + offset * camera.height`; its
/// height is `camera.height * scale`. Thus authored camera movement and inspection
/// navigation compose instead of racing to overwrite one camera transform.
/// Offsets use camera-height units on both axes, not viewport-width units.
///
/// This is an inert numerical value. Computing a view changes no authored state,
/// runtime publication, animation time or resource. Adapters must not use it as a
/// renderer-only override: rendering and picking need the same session-resolved
/// view, with ordinary view-revision/receipt invalidation. See issue #1714.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InspectionView2D {
    offset: [f64; 2],
    scale: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InspectionViewError {
    InvalidCamera,
    InvalidAnchor,
    InvalidFactor,
    UnrepresentableCamera,
}

impl std::fmt::Display for InspectionViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidCamera => "inspection requires a finite camera with positive height",
            Self::InvalidAnchor => "inspection anchor must be a finite scene position",
            Self::InvalidFactor => "inspection zoom factor must be finite and positive",
            Self::UnrepresentableCamera => "inspection camera is outside the render domain",
        })
    }
}

impl std::error::Error for InspectionViewError {}

impl Default for InspectionView2D {
    fn default() -> Self {
        Self {
            offset: [0.0, 0.0],
            scale: 1.0,
        }
    }
}

impl InspectionView2D {
    /// Relative view-height limits, not absolute authored camera limits.
    pub const MIN_SCALE: f64 = 1.0 / 64.0;
    pub const MAX_SCALE: f64 = 64.0;

    pub const fn relative_scale(self) -> f64 {
        self.scale
    }

    /// Resolve against the current effective camera, never a retained old camera.
    ///
    /// Invalid later authored camera states fail explicitly. The computation does
    /// not silently clamp authored camera motion or substitute a default camera.
    pub fn resolve(self, camera: Camera2DState) -> Result<Camera2DState, InspectionViewError> {
        validate_camera(camera)?;
        let height = f64::from(camera.height);
        representable_camera(
            f64::from(camera.center.x) + self.offset[0] * height,
            f64::from(camera.center.y) + self.offset[1] * height,
            height * self.scale,
        )
    }

    /// Prepare a new zoom adjustment about an occurrence-local scene anchor.
    ///
    /// `anchor` must come from the existing projection of the admitted displayed
    /// view. This operation does not project raw platform coordinates or validate
    /// their freshness. A factor below one zooms in; above one zooms out.
    ///
    /// The bounded scale uses the actual resulting height when anchoring, including
    /// f32 rounding. Returning a new value leaves the old one intact on failure;
    /// publication and gesture invalidation belong to the enclosing session.
    pub fn zoom_about(
        self,
        camera: Camera2DState,
        anchor: Vec2,
        factor: f64,
    ) -> Result<Self, InspectionViewError> {
        let current = self.resolve(camera)?;
        if !anchor.x.is_finite() || !anchor.y.is_finite() {
            return Err(InspectionViewError::InvalidAnchor);
        }
        if !factor.is_finite() || factor <= 0.0 {
            return Err(InspectionViewError::InvalidFactor);
        }
        // Overflow/underflow of this product is harmless: both operands were
        // validated positive and finite, and clamping precedes all view arithmetic.
        let scale = (self.scale * factor).clamp(Self::MIN_SCALE, Self::MAX_SCALE);
        if scale == self.scale {
            return Ok(self);
        }
        let height = f64::from(camera.height);
        let next_height = (height * scale) as f32;
        if !next_height.is_finite() || next_height <= 0.0 {
            return Err(InspectionViewError::UnrepresentableCamera);
        }
        let ratio = f64::from(next_height) / f64::from(current.height);
        let center = [
            f64::from(anchor.x) + (f64::from(current.center.x) - f64::from(anchor.x)) * ratio,
            f64::from(anchor.y) + (f64::from(current.center.y) - f64::from(anchor.y)) * ratio,
        ];
        let next = Self {
            offset: [
                (center[0] - f64::from(camera.center.x)) / height,
                (center[1] - f64::from(camera.center.y)) / height,
            ],
            scale,
        };
        next.resolve(camera)?;
        Ok(next)
    }
}

fn validate_camera(camera: Camera2DState) -> Result<(), InspectionViewError> {
    if !camera.center.x.is_finite()
        || !camera.center.y.is_finite()
        || !camera.height.is_finite()
        || camera.height <= 0.0
    {
        return Err(InspectionViewError::InvalidCamera);
    }
    Ok(())
}

fn representable_camera(x: f64, y: f64, height: f64) -> Result<Camera2DState, InspectionViewError> {
    let camera = Camera2DState {
        center: Vec2::new(x as f32, y as f32),
        height: height as f32,
    };
    validate_camera(camera).map_err(|_| InspectionViewError::UnrepresentableCamera)?;
    Ok(camera)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AnimationOptions, IndicateOptions, RateFunction, Scene};

    fn camera() -> Camera2DState {
        Camera2DState {
            center: Vec2::new(2.0, -1.0),
            height: 8.0,
        }
    }

    // Independent scene-to-normalized-view oracle, not the production formula.
    fn normalized(camera: Camera2DState, point: Vec2) -> (f64, f64) {
        (
            (f64::from(point.x) - f64::from(camera.center.x)) / f64::from(camera.height),
            (f64::from(point.y) - f64::from(camera.center.y)) / f64::from(camera.height),
        )
    }

    fn assert_close(actual: (f64, f64), expected: (f64, f64)) {
        assert!((actual.0 - expected.0).abs() < 1e-5, "{actual:?} != {expected:?}");
        assert!((actual.1 - expected.1).abs() < 1e-5, "{actual:?} != {expected:?}");
    }

    #[test]
    fn identity_resolves_exactly_without_retaining_a_camera() {
        let view = InspectionView2D::default();
        assert_eq!(view.relative_scale(), 1.0);
        for base in [camera(), Camera2DState::default()] {
            assert_eq!(view.resolve(base).unwrap(), base);
        }
        assert!(std::mem::size_of::<InspectionView2D>() <= 3 * std::mem::size_of::<f64>());
    }

    #[test]
    fn both_zoom_directions_keep_the_anchor_at_its_screen_position() {
        for anchor in [Vec2::ZERO, Vec2::new(-4.0, 3.0), Vec2::new(8.0, -6.0)] {
            for factor in [0.125, 0.73, 1.4, 8.0] {
                let base = camera();
                let view = InspectionView2D::default().zoom_about(base, anchor, factor).unwrap();
                let resolved = view.resolve(base).unwrap();
                assert_close(normalized(resolved, anchor), normalized(base, anchor));
                assert_eq!(resolved.height > base.height, factor > 1.0);
            }
        }
    }

    #[test]
    fn center_zoom_does_not_pan_and_unit_factor_is_exactly_inert() {
        let base = camera();
        let view = InspectionView2D::default().zoom_about(base, base.center, 0.5).unwrap();
        assert_eq!(view.resolve(base).unwrap().center, base.center);
        assert_eq!(view.zoom_about(base, Vec2::new(9.0, 3.0), 1.0).unwrap(), view);
    }

    #[test]
    fn reciprocal_factors_restore_the_original_view() {
        let base = camera();
        let initial = InspectionView2D::default();
        let zoomed = initial.zoom_about(base, Vec2::new(-4.0, 3.0), 0.5).unwrap();
        let restored = zoomed.zoom_about(base, Vec2::new(-4.0, 3.0), 2.0).unwrap();
        assert_eq!(restored, initial);
        assert_eq!(restored.resolve(base).unwrap(), base);
    }

    #[test]
    fn clamped_factors_keep_the_anchor_and_saturated_updates_are_inert() {
        let base = camera();
        let anchor = Vec2::new(-4.0, 3.0);
        for (factor, expected) in [
            (f64::MAX, InspectionView2D::MAX_SCALE),
            (f64::MIN_POSITIVE, InspectionView2D::MIN_SCALE),
        ] {
            let view = InspectionView2D::default().zoom_about(base, anchor, factor).unwrap();
            assert_eq!(view.relative_scale(), expected);
            assert_close(normalized(view.resolve(base).unwrap(), anchor), normalized(base, anchor));
            assert_eq!(view.zoom_about(base, anchor, factor).unwrap(), view);
        }
    }

    #[test]
    fn adjustment_follows_authored_camera_translation_and_height() {
        let base = camera();
        let view = InspectionView2D::default().zoom_about(base, Vec2::new(-2.0, 3.0), 0.5).unwrap();
        assert_eq!(view.resolve(base).unwrap().center, Vec2::new(0.0, 1.0));
        let moved = Camera2DState { center: Vec2::new(10.0, 20.0), height: 16.0 };
        assert_eq!(view.resolve(moved).unwrap(), Camera2DState {
            center: Vec2::new(6.0, 24.0), height: 8.0,
        });
        assert_eq!(InspectionView2D::default().resolve(moved).unwrap(), moved);
    }

    #[test]
    fn invalid_factors_and_anchors_leave_the_input_value_unchanged() {
        let view = InspectionView2D::default();
        for factor in [0.0, -0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(view.zoom_about(camera(), Vec2::ZERO, factor), Err(InspectionViewError::InvalidFactor));
        }
        for anchor in [Vec2::new(f32::NAN, 0.0), Vec2::new(0.0, f32::INFINITY)] {
            assert_eq!(view.zoom_about(camera(), anchor, 1.0), Err(InspectionViewError::InvalidAnchor));
        }
        assert_eq!(view, InspectionView2D::default());
    }

    #[test]
    fn invalid_base_camera_never_falls_back_to_a_default() {
        let view = InspectionView2D::default();
        for height in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let base = Camera2DState { height, ..camera() };
            assert_eq!(view.resolve(base), Err(InspectionViewError::InvalidCamera));
            assert_eq!(view.zoom_about(base, Vec2::ZERO, 1.0), Err(InspectionViewError::InvalidCamera));
        }
        let base = Camera2DState { center: Vec2::new(f32::NAN, 0.0), ..camera() };
        assert_eq!(view.resolve(base), Err(InspectionViewError::InvalidCamera));
    }

    #[test]
    fn unrepresentable_height_or_center_is_rejected_before_any_commit() {
        let view = InspectionView2D::default();
        let tiny = Camera2DState { height: f32::from_bits(1), ..camera() };
        assert_eq!(view.zoom_about(tiny, tiny.center, 0.5), Err(InspectionViewError::UnrepresentableCamera));
        let huge = Camera2DState { height: f32::MAX, ..camera() };
        assert_eq!(view.zoom_about(huge, huge.center, 2.0), Err(InspectionViewError::UnrepresentableCamera));
        assert_eq!(view.zoom_about(camera(), Vec2::new(f32::MAX, 0.0), 4.0), Err(InspectionViewError::UnrepresentableCamera));
    }

    #[test]
    fn later_authored_height_overflow_does_not_clamp_the_authored_camera() {
        let base = camera();
        let view = InspectionView2D::default().zoom_about(base, base.center, 2.0).unwrap();
        let later = Camera2DState { height: f32::MAX, ..base };
        assert_eq!(view.resolve(later), Err(InspectionViewError::UnrepresentableCamera));
        assert_eq!(later.height, f32::MAX);
    }

    #[test]
    fn navigation_math_during_real_indicate_preserves_publication_and_restoration() {
        let mut scene = Scene::new();
        let frame = scene.camera_frame().unwrap();
        let circle = scene.circle(1.0).unwrap();
        scene.add(&circle).unwrap();
        let mut session = scene.execution_session().unwrap();
        let original = scene.live(&mut session).effective(&circle).unwrap();
        let authored_camera = frame.state().unwrap();
        let segment = scene.live(&mut session).declare_and_activate_indicate(
            &circle, IndicateOptions::default(), AnimationOptions::new().run_time(1.0),
        ).unwrap();
        scene.live(&mut session).advance_segment_to(segment, 0.5).unwrap();
        let midpoint = scene.live(&mut session).effective(&circle).unwrap();
        assert!(midpoint.transform.scale.x > original.transform.scale.x);
        assert_ne!(midpoint.style, original.style);
        let publication = session.publication_context();
        let snapshot = session.frame().clone();
        let base = session.camera().unwrap();
        let view = InspectionView2D::default().zoom_about(base, Vec2::new(2.0, 1.0), 0.5).unwrap();
        assert_eq!(view.resolve(base).unwrap().height, base.height * 0.5);
        assert_eq!(session.publication_context(), publication);
        assert_eq!(session.frame(), &snapshot);
        assert_eq!(frame.state().unwrap(), authored_camera);
        scene.live(&mut session).advance_segment_to(segment, segment.end_time()).unwrap();
        scene.live(&mut session).complete_segment(segment).unwrap();
        let restored = scene.live(&mut session).effective(&circle).unwrap();
        assert_eq!(restored.transform, original.transform);
        assert_eq!(restored.style, original.style);
        assert_eq!(frame.state().unwrap(), authored_camera);
        assert_eq!(view.resolve(session.camera().unwrap()).unwrap().height, base.height * 0.5);
    }

    #[test]
    fn navigation_math_composes_with_a_real_camera_animation_and_its_completion() {
        let mut scene = Scene::new();
        let frame = scene.camera_frame().unwrap();
        let mut target = frame.target_editor().unwrap();
        target.set_translation(4.0, 2.0).unwrap();
        target.set_scale(2.0, 2.0).unwrap();
        let mut session = scene.execution_session().unwrap();
        let segment = scene.live(&mut session).declare_and_activate_transform_to(
            &frame, &target, AnimationOptions::new().run_time(1.0).rate_func(RateFunction::Linear),
        ).unwrap();
        let start = session.camera().unwrap();
        let view = InspectionView2D::default().zoom_about(start, Vec2::new(2.0, 1.0), 0.5).unwrap();
        for time in [0.25, 0.5, 1.0] {
            scene.live(&mut session).advance_segment_to(segment, time).unwrap();
            let base = session.camera().unwrap();
            let publication = session.publication_context();
            let resolved = view.resolve(base).unwrap();
            assert_eq!(resolved.height, base.height * 0.5);
            assert_close(
                (f64::from(resolved.center.x), f64::from(resolved.center.y)),
                (f64::from(base.center.x) + f64::from(base.height / start.height),
                 f64::from(base.center.y) + 0.5 * f64::from(base.height / start.height)),
            );
            assert_eq!(session.publication_context(), publication);
        }
        let resolved_endpoint = view.resolve(session.camera().unwrap()).unwrap();
        scene.live(&mut session).complete_segment(segment).unwrap();
        assert_eq!(view.resolve(session.camera().unwrap()).unwrap(), resolved_endpoint);
        assert_eq!(session.camera().unwrap().center, Vec2::new(4.0, 2.0));
    }
}
