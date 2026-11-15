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
        NativeGraph::new(self.graph_scene()?, directed, vertices, edges, options)
            .map_err(|error| AuthoringFailure::unclassified("graph.create", &error))
    }

    pub(crate) fn live_mutate_graph(
        &mut self,
        graph: &mut NativeGraph,
        operation: GraphOperation,
    ) -> Result<(), AuthoringFailure> {
        graph
            .apply(self.graph_scene()?, operation)
            .map_err(|error| AuthoringFailure::unclassified("graph.mutate", &error))
    }

    pub(crate) fn live_copy_graph(
        &mut self,
        graph: &NativeGraph,
    ) -> Result<NativeGraph, AuthoringFailure> {
        graph
            .copy_in_scene(self.graph_scene()?)
            .map_err(|error| AuthoringFailure::unclassified("graph.copy", &error))
    }

    fn graph_scene(&mut self) -> Result<&mut noon::Scene, AuthoringFailure> {
        match &mut self.player_ownership {
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => {
                Ok(self.active_live_player()?.scene_mut())
            }
            PlayerOwnership::Unstarted => Ok(&mut self.scene),
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
