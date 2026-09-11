//! Shared Manim-compatible TangentLine construction from retained path queries.
//!
//! Tangent sampling is a semantic authoring operation: the source path is observed
//! once in authored world space, two nearby path proportions are sampled, and the
//! resulting direction is normalized to the requested line length. The result is an
//! ordinary analytic Line candidate; no source dependency, renderer primitive, or
//! frontend-owned geometry is retained after construction.

use crate::{AuthoringError, ManimGeometryOptions, Mobject};

impl Mobject {
    /// Prepare an inert analytic Line matching ManimCE v0.21 `TangentLine`.
    ///
    /// `alpha +/- d_alpha` is clamped to `[0, 1]` before sampling the source's
    /// authored world-space path. The sampled chord is then scaled about its
    /// midpoint to `length`, exactly matching the constructor's Line-then-scale
    /// geometry for a nondegenerate sample. This query creates no semantic identity
    /// or immutable geometry resource; callers publish the returned options through
    /// the ordinary geometry authoring path after applying constructor style.
    pub fn manim_tangent_line_options(
        &self,
        alpha: f64,
        length: f64,
        d_alpha: f64,
    ) -> Result<ManimGeometryOptions, AuthoringError> {
        let query = self.path_query()?;
        let a1 = (alpha - d_alpha).clamp(0.0, 1.0);
        let a2 = (alpha + d_alpha).clamp(0.0, 1.0);
        let first = query.point_from_proportion(a1)?;
        let second = query.point_from_proportion(a2)?;

        let dx = second.0 - first.0;
        let dy = second.1 - first.1;
        let sample_length = dx.hypot(dy);
        if !sample_length.is_finite() || sample_length == 0.0 {
            return Err(AuthoringError::NonPositiveNumber {
                name: "TangentLine sampled chord length".to_owned(),
                value: sample_length,
            });
        }

        let midpoint = ((first.0 + second.0) * 0.5, (first.1 + second.1) * 0.5);
        let half = length * 0.5;
        let unit = (dx / sample_length, dy / sample_length);
        ManimGeometryOptions::line(
            midpoint.0 - unit.0 * half,
            midpoint.1 - unit.1 * half,
            midpoint.0 + unit.0 * half,
            midpoint.1 + unit.1 * half,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Scene;

    #[test]
    fn tangent_line_uses_clamped_path_samples_and_requested_length() {
        let scene = Scene::new();
        let circle = scene.circle(2.0).unwrap();
        let query = circle.path_query().unwrap();
        let sample_a = query.point_from_proportion(0.0).unwrap();
        let sample_b = query.point_from_proportion(1.0e-6).unwrap();
        let midpoint = (
            (sample_a.0 + sample_b.0) * 0.5,
            (sample_a.1 + sample_b.1) * 0.5,
        );

        let line = scene
            .geometry(
                circle
                    .manim_tangent_line_options(0.0, 4.0, 1.0e-6)
                    .unwrap(),
            )
            .unwrap();
        let endpoints = line.manim_line_endpoints().unwrap();
        let dx = endpoints.end.0 - endpoints.start.0;
        let dy = endpoints.end.1 - endpoints.start.1;
        assert!((dx.hypot(dy) - 4.0).abs() < 1.0e-5);
        assert!(((endpoints.start.0 + endpoints.end.0) * 0.5 - midpoint.0).abs() < 1.0e-6);
        assert!(((endpoints.start.1 + endpoints.end.1) * 0.5 - midpoint.1).abs() < 1.0e-6);
    }

    #[test]
    fn tangent_line_does_not_mutate_its_source() {
        let scene = Scene::new();
        let circle = scene.circle(2.0).unwrap();
        let before = circle.state().unwrap();
        let revision = scene.revision();

        let options = circle
            .manim_tangent_line_options(0.4, 3.0, 1.0e-5)
            .unwrap();
        assert_eq!(circle.state().unwrap(), before);
        assert_eq!(scene.revision(), revision);

        let _line = scene.geometry(options).unwrap();
        assert_eq!(circle.state().unwrap(), before);
    }

    #[test]
    fn tangent_line_rejects_degenerate_sampling_before_publication() {
        let scene = Scene::new();
        let circle = scene.circle(2.0).unwrap();
        let revision = scene.revision();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats();

        assert!(circle
            .manim_tangent_line_options(0.5, 1.0, 0.0)
            .is_err());
        assert_eq!(scene.revision(), revision);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .stats(),
            resources
        );
    }
}
