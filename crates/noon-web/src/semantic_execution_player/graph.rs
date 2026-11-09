use crate::authoring_error::AuthoringFailure;
use crate::authoring_graph::NativeGraph;

use super::SemanticExecutionPlayer;

impl SemanticExecutionPlayer {
    pub(crate) fn live_create_graph(
        &mut self,
        directed: bool,
        vertices: Vec<(u32, (f64, f64))>,
        edges: Vec<(u32, u32)>,
        options: noon::GraphOptions,
    ) -> Result<NativeGraph, AuthoringFailure> {
        let mut live = self.source.live();
        let graph = if directed {
            NativeGraph::Directed(
                noon::DiGraph::with_options_live(&mut live, vertices, edges, options)
                    .map_err(|error| AuthoringFailure::unclassified("graph.create", &error))?,
            )
        } else {
            NativeGraph::Undirected(
                noon::Graph::with_options_live(&mut live, vertices, edges, options)
                    .map_err(|error| AuthoringFailure::unclassified("graph.create", &error))?,
            )
        };
        Ok(graph)
    }

    pub(crate) fn live_mutate_graph(
        &mut self,
        graph: &mut NativeGraph,
        operation: crate::authoring_graph::GraphOperation,
    ) -> Result<(), AuthoringFailure> {
        graph
            .apply_live(&mut self.source.live(), operation)
            .map_err(|error| AuthoringFailure::unclassified("graph.mutate", &error))
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_copy_graph(
        &mut self,
        graph: &NativeGraph,
    ) -> Result<NativeGraph, AuthoringFailure> {
        graph
            .copy_live(&mut self.source.live())
            .map_err(|error| AuthoringFailure::unclassified("graph.copy", &error))
    }
}
