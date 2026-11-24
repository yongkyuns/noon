//! Native transport for the shared, disposable Riemann options and snapshot.
use crate::{engine_error, family::FamilyHandle, mobject::MobjectHandle};
use noon::{
    integration::{coordinate_failure, RiemannInput},
    ManimAxes, RiemannRectangleOptions, RiemannRectanglePlan,
};
use pyo3::prelude::*;

#[pyclass(unsendable, module = "_noon_native")]
pub struct RiemannOptions(pub(crate) Option<RiemannInput>);

#[pymethods]
impl RiemannOptions {
    #[pyo3(name = "setPaint")]
    fn set_paint(
        &mut self,
        colors: Vec<f64>,
        stroke_color: Vec<f64>,
        stroke_width: f64,
        fill_opacity: f64,
        show_signed_area: bool,
        blend: bool,
    ) -> PyResult<()> {
        self.0
            .as_mut()
            .ok_or_else(|| engine_error("Riemann options have already been consumed"))?
            .set_paint(
                &colors,
                &stroke_color,
                stroke_width,
                fill_opacity,
                show_signed_area,
                blend,
            )
            .map_err(engine_error)
    }

    fn free(&mut self) {
        self.0 = None;
    }
}

impl RiemannOptions {
    pub(crate) fn take(
        &mut self,
        graph: &MobjectHandle,
        bound: Option<&MobjectHandle>,
    ) -> PyResult<RiemannRectangleOptions> {
        self.0
            .take()
            .ok_or_else(|| engine_error("Riemann options have already been consumed"))?
            .into_options(&graph.handle, bound.map(|handle| &handle.handle))
            .map_err(engine_error)
    }
}

#[pyclass(unsendable, module = "_noon_native")]
pub struct RiemannPlan(pub(crate) RiemannRectanglePlan);

fn provided(values: &[f64]) -> Option<&[f64]> {
    (!values.is_empty()).then_some(values)
}

impl RiemannPlan {
    pub(crate) fn publish_on_scene(
        &self,
        scene: &mut noon::Scene,
        top: &[f64],
        baseline: &[f64],
    ) -> PyResult<FamilyHandle> {
        scene
            .publish_riemann_plan(&self.0, provided(top), provided(baseline))
            .map(FamilyHandle::from_semantic_family)
            .map_err(coordinate_failure)
            .map_err(engine_error)
    }
}

#[pymethods]
impl RiemannPlan {
    fn starts(&self) -> Vec<f64> {
        self.0.starts().to_vec()
    }

    fn samples(&self) -> Vec<f64> {
        self.0.samples().collect()
    }

    fn publish(&self, top: Vec<f64>, baseline: Vec<f64>) -> PyResult<FamilyHandle> {
        self.0
            .publish(provided(&top), provided(&baseline))
            .map(FamilyHandle::from_semantic_family)
            .map_err(coordinate_failure)
            .map_err(engine_error)
    }

    fn free(&self) {}
}

pub(crate) fn axes(handle: &FamilyHandle) -> PyResult<ManimAxes> {
    ManimAxes::from_family(handle.family.clone())
        .map_err(coordinate_failure)
        .map_err(engine_error)
}

pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<RiemannOptions>()?;
    m.add_class::<RiemannPlan>()
}
