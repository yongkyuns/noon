use crate::authoring_error::AuthoringFailure;
use crate::authoring_graph::NativeGraph;

use super::SemanticExecutionPlayer;

impl SemanticExecutionPlayer {
    pub(crate) fn live_create_graph(
        &mut self,
        directed: bool,
        vertices: Vec<(u32, (f64, f64))>,
        edges: Vec<(u32, u32)>,
    ) -> Result<NativeGraph, AuthoringFailure> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        let mut live = noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        );
        let graph = if directed {
            NativeGraph::Directed(
                noon::DiGraph::new_live(&mut live, vertices, edges)
                    .map_err(|error| AuthoringFailure::unclassified("graph.create", &error))?,
            )
        } else {
            NativeGraph::Undirected(
                noon::Graph::new_live(&mut live, vertices, edges)
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
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        graph
            .apply_live(
                &mut noon::LiveSession::new(
                    &semantics,
                    self.semantic_root
                        .expect("live semantic store has one scene root"),
                    &mut self.session,
                ),
                operation,
            )
            .map_err(|error| AuthoringFailure::unclassified("graph.mutate", &error))
    }
}
