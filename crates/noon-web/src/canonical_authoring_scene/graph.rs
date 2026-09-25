use crate::authoring_error::AuthoringFailure;
use crate::authoring_graph::{GraphOperation, NativeGraph};

use super::{CanonicalAuthoringScene, PlayerOwnership};

impl CanonicalAuthoringScene {
    pub(crate) fn live_create_graph(
        &mut self,
        directed: bool,
        vertices: Vec<(u32, (f64, f64))>,
        edges: Vec<(u32, u32)>,
    ) -> Result<NativeGraph, AuthoringFailure> {
        match &mut self.player_ownership {
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_create_graph(directed, vertices, edges),
            PlayerOwnership::Unstarted => {
                Err("live Graph construction requires an active canonical session".into())
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
            PlayerOwnership::Unstarted => {
                Err("live Graph mutation requires an active canonical session".into())
            }
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
    ) -> Result<NativeGraph, AuthoringFailure> {
        self.inner.live_create_graph(directed, vertices, edges)
    }

    pub(crate) fn mutate_live_graph(
        &mut self,
        graph: &mut NativeGraph,
        operation: GraphOperation,
    ) -> Result<(), AuthoringFailure> {
        self.inner.live_mutate_graph(graph, operation)
    }
}
