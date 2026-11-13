//! Disposable plot input shared by optional language projections.
use super::{coordinate_math_failure, plot_failure, sampling_failure, AuthoringFailure};
use crate::{
    AxesFrame, ManimGeometryOptions, Mobject, PlotSamplingOptions, PlotSamplingPlan,
    RiemannRectangleOptions, RiemannSample,
};
/// Disposable preparation input. It retains no callable, semantic identity or
/// execution owner. A coordinate snapshot, when supplied, is fixed for the whole
/// evaluation, including host functions that perform other authoring work.
pub struct PlotSamplingInput {
    plan: PlotSamplingPlan,
    frame: Option<AxesFrame>,
}

impl PlotSamplingInput {
    pub fn prepare(
        mut options: PlotSamplingOptions,
        discontinuities: Vec<f64>,
        dt: Option<f64>,
        max_samples: Option<u32>,
        frame: Option<AxesFrame>,
    ) -> Result<Self, AuthoringFailure> {
        options.discontinuities = discontinuities;
        if let Some(dt) = dt {
            options.dt = dt;
        }
        if let Some(limit) = max_samples {
            options.max_samples = limit as usize;
        }
        Ok(Self {
            plan: options.plan().map_err(sampling_failure)?,
            frame,
        })
    }

    fn geometry(
        &self,
        mut points: Vec<[f64; 2]>,
        smooth: bool,
    ) -> Result<ManimGeometryOptions, AuthoringFailure> {
        if let Some(frame) = self.frame {
            for point in &mut points {
                *point = frame
                    .coords_to_point(point[0], point[1])
                    .map_err(coordinate_math_failure)?;
            }
        }
        let mut options = ManimGeometryOptions::plot_samples(&self.plan, &points, smooth)
            .map_err(plot_failure)?;
        if self.frame.is_some() {
            let range = self.plan.range();
            options.set_semantic_role(noon_core::SemanticObjectRole::FunctionPlot(
                noon_core::SemanticFunctionPlotRole::new([range[0], range[1]]),
            ));
        }
        Ok(options)
    }

    fn require_count(&self, actual: usize) -> Result<(), AuthoringFailure> {
        let expected = self.plan.parameters().len();
        if actual != expected {
            return Err(sampling_failure(
                crate::PlotPreparationError::SampleCountMismatch { expected, actual },
            ));
        }
        Ok(())
    }
}

impl PlotSamplingInput {
    pub fn parametric(
        range: &[f64],
        discontinuities: Vec<f64>,
        dt: Option<f64>,
        max_samples: Option<u32>,
    ) -> Result<Self, AuthoringFailure> {
        Self::prepare(
            PlotSamplingOptions::parametric(range).map_err(sampling_failure)?,
            discontinuities,
            dt,
            max_samples,
            None,
        )
    }

    /// One language-boundary copy of the already planned evaluation order.
    pub fn parameters(&self) -> Vec<f64> {
        self.plan.parameters().to_vec()
    }

    /// Python evaluates only y values; Rust supplies the corresponding x values.
    pub fn function_samples(
        &self,
        values: &[f64],
        smooth: bool,
    ) -> Result<ManimGeometryOptions, AuthoringFailure> {
        self.require_count(values.len())?;
        let mut points = Vec::new();
        points
            .try_reserve_exact(values.len())
            .map_err(|_| sampling_failure(crate::PlotPreparationError::AllocationFailed))?;
        points.extend(
            self.plan
                .parameters()
                .iter()
                .zip(values)
                .map(|(&x, &y)| [x, y]),
        );
        let mut options = self.geometry(points, smooth)?;
        if self.frame.is_none() {
            let range = self.plan.range();
            options.set_semantic_role(noon_core::SemanticObjectRole::FunctionPlot(
                noon_core::SemanticFunctionPlotRole::scene_coordinates([range[0], range[1]]),
            ));
        }
        Ok(options)
    }

    pub fn parametric_samples(
        &self,
        values: &[f64],
        smooth: bool,
    ) -> Result<ManimGeometryOptions, AuthoringFailure> {
        if !values.len().is_multiple_of(2) {
            return Err(AuthoringFailure::new(
                "invalid_input",
                "plot.point_components",
                "plot samples require x/y component pairs",
            ));
        }
        self.require_count(values.len() / 2)?;
        let mut points = Vec::new();
        points
            .try_reserve_exact(values.len() / 2)
            .map_err(|_| sampling_failure(crate::PlotPreparationError::AllocationFailed))?;
        points.extend_from_slice(values.as_chunks::<2>().0);
        self.geometry(points, smooth)
    }
}

pub fn data_points(values: &[f64]) -> Result<Vec<[f64; 2]>, AuthoringFailure> {
    if !values.len().is_multiple_of(2) {
        return Err(AuthoringFailure::new(
            "invalid_input",
            "plot.point_components",
            "plot samples require x/y component pairs",
        ));
    }
    if values.len() / 2 > noon_geometry::DEFAULT_PLOT_SAMPLE_LIMIT {
        return Err(sampling_failure(
            crate::PlotPreparationError::SampleLimitExceeded,
        ));
    }
    let mut points = Vec::new();
    points
        .try_reserve_exact(values.len() / 2)
        .map_err(|_| sampling_failure(crate::PlotPreparationError::AllocationFailed))?;
    points.extend_from_slice(values.as_chunks::<2>().0);
    Ok(points)
}

pub struct RiemannInput {
    options: RiemannRectangleOptions,
}

impl RiemannInput {
    pub fn into_options(
        mut self,
        graph: &Mobject,
        bounded_graph: Option<&Mobject>,
    ) -> Result<RiemannRectangleOptions, AuthoringFailure> {
        if let Some(bound) = bounded_graph {
            graph.require_same_store(bound)?;
            self.options.bounded_graph = Some(bound.node_id());
        }
        Ok(self.options)
    }
    pub fn new(
        range: &[f64],
        dx: f64,
        sample: u32,
        width_scale_factor: f64,
    ) -> Result<Self, AuthoringFailure> {
        let x_range = match range {
            [] => None,
            [start, end] if start.is_finite() && end.is_finite() => Some([*start, *end]),
            _ => {
                return Err(AuthoringFailure::new(
                    "invalid_input",
                    "riemann.range",
                    "Riemann range requires two finite values",
                ));
            }
        };
        let sample = match sample {
            0 => RiemannSample::Left,
            1 => RiemannSample::Right,
            2 => RiemannSample::Center,
            _ => {
                return Err(AuthoringFailure::new(
                    "invalid_input",
                    "riemann.sample",
                    "sample must be left, right, or center",
                ));
            }
        };
        Ok(Self {
            options: RiemannRectangleOptions {
                x_range,
                dx,
                sample,
                width_scale_factor,
                ..Default::default()
            },
        })
    }

    pub fn set_paint(
        &mut self,
        colors: &[f64],
        stroke_color: &[f64],
        stroke_width: f64,
        fill_opacity: f64,
        show_signed_area: bool,
        blend: bool,
    ) -> Result<(), AuthoringFailure> {
        let colors = super::gradient_colors(colors)?;
        let stroke_colors = super::gradient_colors(stroke_color)?;
        let [stroke_color] = stroke_colors.as_slice() else {
            return Err(AuthoringFailure::new(
                "invalid_input",
                "riemann.stroke",
                "one stroke color is required",
            ));
        };
        self.options.colors = colors;
        self.options.stroke_color = *stroke_color;
        self.options.stroke_width = stroke_width;
        self.options.fill_opacity = fill_opacity;
        self.options.show_signed_area = show_signed_area;
        self.options.blend = blend;
        Ok(())
    }
}
