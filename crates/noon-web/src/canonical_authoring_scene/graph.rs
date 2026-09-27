use crate::authoring_error::AuthoringFailure;
use crate::authoring_graph::{GraphOperation, NativeGraph};

use super::{CanonicalAuthoringScene, PlayerOwnership};

impl CanonicalAuthoringScene {
    pub(crate) fn live_create_graph(
        &mut self,
        directed: bool,
        vertices: Vec<(u32, (f64, f64))>,
        edges: Vec<(u32, u32)>,
        options: noon::GraphOptions,
    ) -> Result<NativeGraph, AuthoringFailure> {
        match &mut self.player_ownership {
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_create_graph(directed, vertices, edges, options),
            PlayerOwnership::Unstarted => {
                let graph = if directed {
                    noon::DiGraph::with_options(&mut self.scene, vertices, edges, options)
                        .map(NativeGraph::Directed)
                } else {
                    noon::Graph::with_options(&mut self.scene, vertices, edges, options)
                        .map(NativeGraph::Undirected)
                };
                graph.map_err(|error| AuthoringFailure::unclassified("graph.create", &error))
            }
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    pub(crate) fn live_mutate_graph(
        &mut self,
        graph: &mut NativeGraph,
        operation: GraphOperation,
    ) -> Result<(), AuthoringFailure> {
        match &mut self.player_ownership {
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_mutate_graph(graph, operation),
            PlayerOwnership::Unstarted => graph
                .apply(&mut self.scene, operation)
                .map_err(|error| AuthoringFailure::unclassified("graph.mutate", &error)),
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    pub(crate) fn live_copy_graph(
        &mut self,
        graph: &NativeGraph,
    ) -> Result<NativeGraph, AuthoringFailure> {
        match &mut self.player_ownership {
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => {
                self.active_live_player()?.live_copy_graph(graph)
            }
            PlayerOwnership::Unstarted => graph
                .copy_cold()
                .map_err(|error| AuthoringFailure::unclassified("graph.copy", &error)),
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }
}

impl super::wasm::CanonicalAuthoringSceneContext {
    pub(crate) fn create_live_graph(
        &mut self,
        directed: bool,
        vertices: Vec<(u32, (f64, f64))>,
        edges: Vec<(u32, u32)>,
        options: noon::GraphOptions,
    ) -> Result<NativeGraph, AuthoringFailure> {
        self.inner
            .live_create_graph(directed, vertices, edges, options)
    }

    pub(crate) fn mutate_live_graph(
        &mut self,
        graph: &mut NativeGraph,
        operation: GraphOperation,
    ) -> Result<(), AuthoringFailure> {
        self.inner.live_mutate_graph(graph, operation)
    }

    pub(crate) fn copy_live_graph(
        &mut self,
        graph: &NativeGraph,
    ) -> Result<NativeGraph, AuthoringFailure> {
        self.inner.live_copy_graph(graph)
    }
}
