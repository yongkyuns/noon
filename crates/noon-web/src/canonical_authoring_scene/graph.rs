use crate::authoring_error::AuthoringFailure;
use noon::integration::graph::{GraphOperation, HostGraph};

use super::CanonicalAuthoringScene;

impl CanonicalAuthoringScene {
    pub(crate) fn create_graph(
        &mut self,
        directed: bool,
        vertices: Vec<(u32, (f64, f64))>,
        edges: Vec<(u32, u32)>,
        options: noon::GraphOptions,
    ) -> Result<HostGraph, AuthoringFailure> {
        HostGraph::new(
            self.local_authoring_scene()?,
            directed,
            vertices,
            edges,
            options,
        )
        .map_err(|error| AuthoringFailure::unclassified("graph.create", &error))
    }

    pub(crate) fn mutate_graph(
        &mut self,
        graph: &mut HostGraph,
        operation: GraphOperation,
    ) -> Result<(), AuthoringFailure> {
        graph
            .apply(self.local_authoring_scene()?, operation)
            .map_err(|error| AuthoringFailure::unclassified("graph.mutate", &error))
    }

    pub(crate) fn copy_graph(&mut self, graph: &HostGraph) -> Result<HostGraph, AuthoringFailure> {
        graph
            .copy_in_scene(self.local_authoring_scene()?)
            .map_err(|error| AuthoringFailure::unclassified("graph.copy", &error))
    }
}

impl super::wasm::CanonicalAuthoringSceneContext {
    pub(crate) fn create_graph_in_scene(
        &mut self,
        directed: bool,
        vertices: Vec<(u32, (f64, f64))>,
        edges: Vec<(u32, u32)>,
        options: noon::GraphOptions,
    ) -> Result<HostGraph, AuthoringFailure> {
        self.inner.create_graph(directed, vertices, edges, options)
    }

    pub(crate) fn mutate_graph_in_scene(
        &mut self,
        graph: &mut HostGraph,
        operation: GraphOperation,
    ) -> Result<(), AuthoringFailure> {
        self.inner.mutate_graph(graph, operation)
    }

    pub(crate) fn copy_graph_in_scene(
        &mut self,
        graph: &HostGraph,
    ) -> Result<HostGraph, AuthoringFailure> {
        self.inner.copy_graph(graph)
    }
}
