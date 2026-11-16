//! Native projections of the shared Scene Graph/DiGraph adapter.
use crate::{context::Context, engine_error, family::FamilyHandle, mobject::MobjectHandle};
use noon::integration::{
    graph::{manim_stroke_width, GraphOperation, HostGraph},
    AuthoringFailure,
};
use pyo3::prelude::*;

#[pyclass(skip_from_py_object, module = "_noon_native")]
pub(crate) struct GraphOptions {
    pub(crate) options: noon::GraphOptions,
}

#[pymethods]
impl GraphOptions {
    #[new]
    fn new() -> Self {
        Self {
            options: noon::GraphOptions::default(),
        }
    }

    fn free(&self) {}

    #[pyo3(name = "setVertexRadius")]
    fn set_vertex_radius(&mut self, radius: f64) {
        self.options.vertex_radius = radius;
    }

    #[pyo3(name = "setVertexFill")]
    fn set_vertex_fill(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), PyErr> {
        self.options.vertex_fill = graph_color(red, green, blue, alpha)?;
        Ok(())
    }

    #[pyo3(name = "setVertexFillOpacity")]
    fn set_vertex_fill_opacity(&mut self, opacity: f64) {
        self.options.vertex_fill_opacity = opacity;
    }

    #[pyo3(name = "setVertexStroke")]
    fn set_vertex_stroke(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), PyErr> {
        self.options.vertex_stroke = graph_color(red, green, blue, alpha)?;
        Ok(())
    }

    #[pyo3(name = "setVertexStrokeWidth")]
    fn set_vertex_stroke_width(&mut self, width: f64) -> Result<(), PyErr> {
        self.options.vertex_stroke_width =
            manim_stroke_width("vertex stroke width", width).map_err(engine_error)?;
        Ok(())
    }

    #[pyo3(name = "setEdgeColor")]
    fn set_edge_color(&mut self, red: f64, green: f64, blue: f64, alpha: f64) -> Result<(), PyErr> {
        self.options.edge_color = graph_color(red, green, blue, alpha)?;
        Ok(())
    }

    #[pyo3(name = "setEdgeStrokeWidth")]
    fn set_edge_stroke_width(&mut self, width: f64) -> Result<(), PyErr> {
        self.options.edge_stroke_width =
            manim_stroke_width("edge stroke width", width).map_err(engine_error)?;
        Ok(())
    }
}

fn graph_color(red: f64, green: f64, blue: f64, alpha: f64) -> PyResult<noon::Color> {
    noon::integration::family_color(true, red, green, blue, alpha)
        .map_err(engine_error)
        .map(|color| color.expect("enabled graph color"))
}

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub(crate) struct GraphHandle {
    pub(crate) inner: HostGraph,
}

#[pymethods]
impl GraphHandle {
    fn family(&self) -> FamilyHandle {
        FamilyHandle::from_semantic_family(self.inner.family().clone())
    }

    fn vertex(&self, key: u32) -> Option<MobjectHandle> {
        self.inner
            .vertex(key)
            .cloned()
            .map(MobjectHandle::from_semantic_mobject)
    }
}

pub(crate) fn create_graph(
    context: &mut Context,
    directed: bool,
    vertices: Vec<(u32, (f64, f64))>,
    edges: Vec<(u32, u32)>,
    options: noon::GraphOptions,
) -> Result<HostGraph, AuthoringFailure> {
    HostGraph::new(&mut context.scene, directed, vertices, edges, options)
        .map_err(|error| AuthoringFailure::unclassified("graph.create", &error))
}

pub(crate) fn copy_graph(
    context: &mut Context,
    graph: &GraphHandle,
) -> Result<HostGraph, AuthoringFailure> {
    graph
        .inner
        .copy_in_scene(&mut context.scene)
        .map_err(|error| AuthoringFailure::unclassified("graph.copy", &error))
}

pub(crate) fn mutate_graph(
    context: &mut Context,
    graph: &mut GraphHandle,
    operation: GraphOperation,
) -> Result<(), AuthoringFailure> {
    graph
        .inner
        .apply(&mut context.scene, operation)
        .map_err(|error| AuthoringFailure::unclassified("graph.mutate", &error))
}
