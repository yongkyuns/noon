//! Opaque Graph/DiGraph handles. Python maps its arbitrary keys to `u32`; this
//! module never interprets graph JSON or owns endpoint/layout geometry.

use std::collections::HashMap;
use wasm_bindgen::prelude::*;

use crate::authoring_error::js_error;
use crate::CanonicalAuthoringSceneContext;

pub(crate) enum NativeGraph {
    Undirected(noon::Graph<u32>),
    Directed(noon::DiGraph<u32>),
}

pub(crate) enum GraphOperation {
    AddVertices(Vec<(u32, (f64, f64))>),
    AddEdges(Vec<(u32, u32)>),
    RemoveVertices(Vec<u32>),
    RemoveEdges(Vec<(u32, u32)>),
    Circular { scale: f64, center: (f64, f64) },
    Explicit(Vec<(f64, f64)>),
}

impl NativeGraph {
    fn family(&self) -> &noon::MobjectFamily {
        match self {
            Self::Undirected(graph) => graph.family(),
            Self::Directed(graph) => graph.family(),
        }
    }

    pub(crate) fn apply_live(
        &mut self,
        live: &mut noon::LiveSession<'_>,
        operation: GraphOperation,
    ) -> Result<(), noon::GraphAuthoringError> {
        match (self, operation) {
            (Self::Undirected(graph), GraphOperation::AddVertices(vertices)) => {
                graph.add_vertices_live(live, vertices)?;
            }
            (Self::Directed(graph), GraphOperation::AddVertices(vertices)) => {
                graph.add_vertices_live(live, vertices)?;
            }
            (Self::Undirected(graph), GraphOperation::AddEdges(edges)) => {
                graph.add_edges_live(live, edges)?;
            }
            (Self::Directed(graph), GraphOperation::AddEdges(edges)) => {
                graph.add_edges_live(live, edges)?;
            }
            (Self::Undirected(graph), GraphOperation::RemoveVertices(vertices)) => {
                graph.remove_vertices_live(live, vertices)?;
            }
            (Self::Directed(graph), GraphOperation::RemoveVertices(vertices)) => {
                graph.remove_vertices_live(live, vertices)?;
            }
            (Self::Undirected(graph), GraphOperation::RemoveEdges(edges)) => {
                graph.remove_edges_live(live, edges)?;
            }
            (Self::Directed(graph), GraphOperation::RemoveEdges(edges)) => {
                graph.remove_edges_live(live, edges)?;
            }
            (Self::Undirected(graph), GraphOperation::Circular { scale, center }) => {
                graph.change_layout_live(
                    live,
                    noon::GraphLayoutOptions {
                        layout: noon::GraphLayout::Circular,
                        scale,
                        center,
                        ..Default::default()
                    },
                )?;
            }
            (Self::Directed(graph), GraphOperation::Circular { scale, center }) => {
                graph.change_layout_live(
                    live,
                    noon::GraphLayoutOptions {
                        layout: noon::GraphLayout::Circular,
                        scale,
                        center,
                        ..Default::default()
                    },
                )?;
            }
            (Self::Undirected(graph), GraphOperation::Explicit(positions)) => {
                graph.change_layout_positions_live(live, &positions)?;
            }
            (Self::Directed(graph), GraphOperation::Explicit(positions)) => {
                graph.change_layout_positions_live(live, &positions)?;
            }
        }
        Ok(())
    }

    fn snapshot_graph(
        graph: &noon::Graph<u32>,
    ) -> Result<(Vec<(u32, (f64, f64))>, Vec<(u32, u32)>), noon::GraphAuthoringError> {
        let mut keys = HashMap::new();
        let mut vertices = Vec::new();
        for &key in graph.vertex_keys() {
            let vertex = graph.vertex(&key).expect("graph key resolves to a vertex");
            let translation = vertex.state()?.transform.translation;
            keys.insert(graph.vertex_id(&key).expect("graph key has an ID"), key);
            vertices.push((key, (translation.x, translation.y)));
        }
        let edges = graph
            .edge_mobjects()
            .map(|(edge, _)| (keys[&edge.start], keys[&edge.end]))
            .collect();
        Ok((vertices, edges))
    }

    fn snapshot_digraph(
        graph: &noon::DiGraph<u32>,
    ) -> Result<(Vec<(u32, (f64, f64))>, Vec<(u32, u32)>), noon::GraphAuthoringError> {
        let mut keys = HashMap::new();
        let mut vertices = Vec::new();
        for &key in graph.vertex_keys() {
            let vertex = graph.vertex(&key).expect("graph key resolves to a vertex");
            let translation = vertex.state()?.transform.translation;
            keys.insert(graph.vertex_id(&key).expect("graph key has an ID"), key);
            vertices.push((key, (translation.x, translation.y)));
        }
        let edges = graph
            .edge_mobjects()
            .map(|(edge, _)| (keys[&edge.start], keys[&edge.end]))
            .collect();
        Ok((vertices, edges))
    }

    pub(crate) fn copy_live(
        &self,
        live: &mut noon::LiveSession<'_>,
    ) -> Result<Self, noon::GraphAuthoringError> {
        match self {
            Self::Undirected(graph) => {
                let (vertices, edges) = Self::snapshot_graph(graph)?;
                noon::Graph::new_live(live, vertices, edges).map(Self::Undirected)
            }
            Self::Directed(graph) => {
                let (vertices, edges) = Self::snapshot_digraph(graph)?;
                noon::DiGraph::new_live(live, vertices, edges).map(Self::Directed)
            }
        }
    }
}

fn pairs(values: &[u32], name: &str) -> Result<Vec<(u32, u32)>, JsValue> {
    let chunks = values.chunks_exact(2);
    if !chunks.remainder().is_empty() {
        return Err(js_error(format!("{name} requires pairs of vertex IDs")));
    }
    Ok(chunks.map(|pair| (pair[0], pair[1])).collect())
}

fn vertices(ids: &[u32], positions: &[f64]) -> Result<Vec<(u32, (f64, f64))>, JsValue> {
    if positions.len() != ids.len() * 2 {
        return Err(js_error(
            "Graph positions require exactly two finite values per vertex",
        ));
    }
    ids.iter()
        .zip(positions.chunks_exact(2))
        .map(|(&id, position)| {
            let (x, y) = (position[0], position[1]);
            if !x.is_finite() || !y.is_finite() {
                return Err(js_error("Graph positions must be finite"));
            }
            Ok((id, (x, y)))
        })
        .collect()
}

#[wasm_bindgen]
pub struct WasmGraphHandle {
    inner: NativeGraph,
}

#[wasm_bindgen]
impl WasmGraphHandle {
    #[wasm_bindgen(js_name = family)]
    pub fn family(&self) -> crate::WasmAuthoringFamilyHandle {
        crate::WasmAuthoringFamilyHandle::from_semantic_family(self.inner.family().clone())
    }
}

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    /// Build an opaque native Graph/DiGraph through the active live player.
    /// Key mapping belongs to the language facade; topology remains in Rust.
    #[wasm_bindgen(js_name = liveCreateGraph)]
    pub fn live_create_graph(
        &mut self,
        directed: bool,
        vertex_ids: &[u32],
        positions: &[f64],
        edge_pairs: &[u32],
    ) -> Result<WasmGraphHandle, JsValue> {
        let vertices = vertices(vertex_ids, positions)?;
        let edges = pairs(edge_pairs, "Graph edges")?;
        self.create_live_graph(directed, vertices, edges)
            .map(|inner| WasmGraphHandle { inner })
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = liveGraphCopy)]
    pub fn live_graph_copy(&mut self, graph: &WasmGraphHandle) -> Result<WasmGraphHandle, JsValue> {
        self.copy_live_graph(&graph.inner)
            .map(|inner| WasmGraphHandle { inner })
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = liveGraphAddVertices)]
    pub fn live_graph_add_vertices(
        &mut self,
        graph: &mut WasmGraphHandle,
        vertex_ids: &[u32],
        positions: &[f64],
    ) -> Result<(), JsValue> {
        self.mutate_live_graph(
            &mut graph.inner,
            GraphOperation::AddVertices(vertices(vertex_ids, positions)?),
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = liveGraphAddEdges)]
    pub fn live_graph_add_edges(
        &mut self,
        graph: &mut WasmGraphHandle,
        edge_pairs: &[u32],
    ) -> Result<(), JsValue> {
        self.mutate_live_graph(
            &mut graph.inner,
            GraphOperation::AddEdges(pairs(edge_pairs, "Graph edges")?),
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = liveGraphCircularLayout)]
    pub fn live_graph_circular_layout(
        &mut self,
        graph: &mut WasmGraphHandle,
        scale: f64,
        center_x: f64,
        center_y: f64,
    ) -> Result<(), JsValue> {
        if !scale.is_finite() || !center_x.is_finite() || !center_y.is_finite() {
            return Err(js_error("circular Graph layout values must be finite"));
        }
        self.mutate_live_graph(
            &mut graph.inner,
            GraphOperation::Circular {
                scale,
                center: (center_x, center_y),
            },
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = liveGraphExplicitLayout)]
    pub fn live_graph_explicit_layout(
        &mut self,
        graph: &mut WasmGraphHandle,
        positions: &[f64],
    ) -> Result<(), JsValue> {
        let ids = (0..positions.len() / 2)
            .map(|id| id as u32)
            .collect::<Vec<_>>();
        let positions = vertices(&ids, positions)?
            .into_iter()
            .map(|(_, point)| point)
            .collect();
        self.mutate_live_graph(&mut graph.inner, GraphOperation::Explicit(positions))
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = liveGraphRemoveVertices)]
    pub fn live_graph_remove_vertices(
        &mut self,
        graph: &mut WasmGraphHandle,
        vertex_ids: &[u32],
    ) -> Result<(), JsValue> {
        self.mutate_live_graph(
            &mut graph.inner,
            GraphOperation::RemoveVertices(vertex_ids.to_vec()),
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = liveGraphRemoveEdges)]
    pub fn live_graph_remove_edges(
        &mut self,
        graph: &mut WasmGraphHandle,
        edge_pairs: &[u32],
    ) -> Result<(), JsValue> {
        self.mutate_live_graph(
            &mut graph.inner,
            GraphOperation::RemoveEdges(pairs(edge_pairs, "Graph edges")?),
        )
        .map_err(js_error)
    }
}
