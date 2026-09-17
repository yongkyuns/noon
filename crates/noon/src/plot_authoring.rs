//! Static curves through the ordinary retained geometry constructor.
//!
//! These operations return ordinary Mobjects, not an Axes facade or a second
//! graph model. Function evaluation is preparation-time only. Ranges and sample
//! planning are inert inputs; the resulting authored content is the sampled path.
//! Persistent graph-query metadata and dynamic resampling are separate B5 work.

use crate::{AuthoringError, ManimGeometryOptions, Mobject, Scene};
use noon_geometry::{PlotPreparationError, PlotSamplingOptions, PlotSamplingPlan};

#[derive(Debug)]
pub enum PlotAuthoringError {
    Preparation(PlotPreparationError),
    Authoring(AuthoringError),
}

impl std::fmt::Display for PlotAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Preparation(error) => std::fmt::Display::fmt(error, formatter),
            Self::Authoring(error) => std::fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for PlotAuthoringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Preparation(error) => Some(error),
            Self::Authoring(error) => Some(error),
        }
    }
}

impl From<PlotPreparationError> for PlotAuthoringError {
    fn from(error: PlotPreparationError) -> Self {
        Self::Preparation(error)
    }
}

impl From<AuthoringError> for PlotAuthoringError {
    fn from(error: AuthoringError) -> Self {
        Self::Authoring(error)
    }
}

impl ManimGeometryOptions {
    /// Prepare a static 2D parametric curve with the ordinary path defaults.
    ///
    /// The callback is called once per planned parameter. Validation and path
    /// preparation finish before this request can allocate semantic identity.
    /// Apply color/stroke/transform setters to the returned inert request before
    /// passing it to `Scene::geometry` or `LiveSession::create_manim_geometry`.
    pub fn parametric_plot(
        sampling: &PlotSamplingOptions,
        function: impl FnMut(f64) -> [f64; 2],
        use_smoothing: bool,
    ) -> Result<Self, PlotAuthoringError> {
        let path = sampling.plan()?.evaluate(function, use_smoothing)?;
        Ok(Self::path(path)?)
    }

    /// Prepare y=f(x) in scene coordinates. Axes coordinate mapping is separate;
    /// this helper does not infer ranges or rescale the function for the viewport.
    pub fn function_plot(
        sampling: &PlotSamplingOptions,
        mut function: impl FnMut(f64) -> f64,
        use_smoothing: bool,
    ) -> Result<Self, PlotAuthoringError> {
        Self::parametric_plot(sampling, |x| [x, function(x)], use_smoothing)
    }

    /// Convert host-evaluated samples from one shared plan into a normal inert
    /// geometry request. Sampling/subpath decisions never move into the host.
    pub fn plot_samples(
        plan: &PlotSamplingPlan,
        points: &[[f64; 2]],
        use_smoothing: bool,
    ) -> Result<Self, PlotAuthoringError> {
        Ok(Self::path(plan.path_from_samples(points, use_smoothing)?)?)
    }

    /// Preserve supplied data order as a polyline without smoothing/resampling.
    pub fn sampled_plot(points: &[[f64; 2]]) -> Result<Self, PlotAuthoringError> {
        Ok(Self::path(noon_geometry::sampled_plot_path(points)?)?)
    }
}

impl Scene {
    /// Construct detached static geometry through the Scene-owned publication
    /// path. This works before and after execution bootstrap; live continuations
    /// can continue to use `LiveSession::create_manim_geometry` when they do not
    /// own the Scene control surface.
    pub fn parametric_plot(
        &mut self,
        sampling: &PlotSamplingOptions,
        function: impl FnMut(f64) -> [f64; 2],
        use_smoothing: bool,
    ) -> Result<Mobject, PlotAuthoringError> {
        Ok(self.geometry(ManimGeometryOptions::parametric_plot(
            sampling,
            function,
            use_smoothing,
        )?)?)
    }

    /// Construct a static y=f(x) path through the Scene-owned publication path.
    pub fn function_plot(
        &mut self,
        sampling: &PlotSamplingOptions,
        function: impl FnMut(f64) -> f64,
        use_smoothing: bool,
    ) -> Result<Mobject, PlotAuthoringError> {
        Ok(self.geometry(ManimGeometryOptions::function_plot(
            sampling,
            function,
            use_smoothing,
        )?)?)
    }

    /// Construct a static data polyline through the Scene-owned publication path.
    pub fn sampled_plot(&mut self, points: &[[f64; 2]]) -> Result<Mobject, PlotAuthoringError> {
        Ok(self.geometry(ManimGeometryOptions::sampled_plot(points)?)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn static_function_evaluates_only_during_preparation() {
        let mut scene = Scene::new();
        let calls = Cell::new(0);
        let sampling = PlotSamplingOptions::parametric(&[-1.0, 1.0, 0.5]).unwrap();
        let curve = scene
            .function_plot(
                &sampling,
                |x| {
                    calls.set(calls.get() + 1);
                    x * x
                },
                false,
            )
            .unwrap();
        assert_eq!(calls.get(), 5);
        assert_eq!(curve.path_query().unwrap().start().unwrap(), (-1.0, 1.0));
        assert_eq!(curve.path_query().unwrap().end().unwrap(), (1.0, 1.0));
        scene.add(&curve).unwrap();
        let session = scene.execution_session().unwrap();
        assert_eq!(session.frame().objects.len(), 1);
        assert_eq!(calls.get(), 5);
    }

    #[test]
    fn rejected_plot_preserves_scene_revision_and_resources() {
        let mut scene = Scene::new();
        let revision = scene.revision();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        let sampling = PlotSamplingOptions::parametric(&[0.0, 1.0, 0.5]).unwrap();
        let result = scene.parametric_plot(&sampling, |_| [f64::NAN, 0.0], false);
        assert!(matches!(
            result,
            Err(PlotAuthoringError::Preparation(
                PlotPreparationError::InvalidPoint { sample_index: 0 }
            ))
        ));
        assert_eq!(scene.revision(), revision);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            resources
        );
        assert!(scene.sampled_plot(&[[0.0, 0.0], [1.0, 1.0]]).is_ok());
    }

    #[test]
    fn affine_edits_retain_sampled_geometry_resource() {
        let mut scene = Scene::new();
        let mut curve = scene.sampled_plot(&[[0.0, 0.0], [1.0, 2.0]]).unwrap();
        let content = curve.state().unwrap().content;
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        curve.shift(3.0, -2.0).unwrap();
        assert_eq!(curve.state().unwrap().content, content);
        assert_eq!(curve.path_query().unwrap().start().unwrap(), (3.0, -2.0));
        assert_eq!(curve.path_query().unwrap().end().unwrap(), (4.0, 0.0));
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            resources
        );
    }

    #[test]
    fn native_and_host_sample_ingestion_produce_the_same_path() {
        let mut scene = Scene::new();
        let sampling = PlotSamplingOptions::parametric(&[0.0, 1.0, 0.25]).unwrap();
        let plan = sampling.plan().unwrap();
        let points: Vec<_> = plan.parameters().iter().map(|&t| [t, t * t]).collect();
        let native = scene
            .parametric_plot(&sampling, |t| [t, t * t], true)
            .unwrap();
        let host = scene
            .geometry(ManimGeometryOptions::plot_samples(&plan, &points, true).unwrap())
            .unwrap();
        assert_eq!(
            native.local_path_query().unwrap().anchors_and_handles(),
            host.local_path_query().unwrap().anchors_and_handles()
        );
    }

    #[test]
    fn extreme_parameter_ranges_retain_both_renderable_endpoints() {
        for range in [[0.0, 1.0e-300, 1.0e100], [1.0e308, 1.25e308, 1.0e308]] {
            let mut scene = Scene::new();
            let calls = Cell::new(0);
            let sampling = PlotSamplingOptions::parametric(&range).unwrap();
            let curve = scene
                .parametric_plot(
                    &sampling,
                    |t| {
                        calls.set(calls.get() + 1);
                        let alpha = (t - range[0]) / (range[1] - range[0]);
                        [alpha, alpha]
                    },
                    false,
                )
                .unwrap();
            assert_eq!(calls.get(), 2);
            assert_eq!(curve.path_query().unwrap().start().unwrap(), (0.0, 0.0));
            assert_eq!(curve.path_query().unwrap().end().unwrap(), (1.0, 1.0));
            assert_eq!(curve.path_query().unwrap().curve_count(), 1);
            scene.add(&curve).unwrap();
            let session = scene.execution_session().unwrap();
            assert_eq!(session.frame().objects.len(), 1);
            assert_eq!(calls.get(), 2);
        }
    }

    #[test]
    fn under_capacity_tiny_plot_rejects_before_callback_or_scene_mutation() {
        let mut scene = Scene::new();
        let sentinel = scene.sampled_plot(&[[0.0, 0.0], [1.0, 1.0]]).unwrap();
        let before = sentinel.state().unwrap();
        let revision = scene.revision();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        let calls = Cell::new(0);
        let mut sampling = PlotSamplingOptions::parametric(&[0.0, 1.0e-300, 1.0e100]).unwrap();
        sampling.max_samples = 1;
        let result = scene.parametric_plot(
            &sampling,
            |_| {
                calls.set(calls.get() + 1);
                [0.0, 0.0]
            },
            false,
        );
        assert!(matches!(
            result,
            Err(PlotAuthoringError::Preparation(
                PlotPreparationError::SampleLimitExceeded
            ))
        ));
        assert_eq!(calls.get(), 0);
        assert_eq!(scene.revision(), revision);
        assert_eq!(sentinel.state().unwrap(), before);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            resources
        );
    }
}
