//! Opaque Graph/DiGraph handles. Python maps its arbitrary keys to `u32`; this
//! module never interprets graph JSON or owns endpoint/layout geometry.

use wasm_bindgen::prelude::*;

use crate::CanonicalAuthoringSceneContext;
use crate::authoring_error::js_error;

pub(crate) enum NativeGraph {
    Undirected(noon::Graph<u32>),
    Directed(noon::DiGraph<u32>),
}

pub(crate) enum GraphOperation {
    AddVertices(Vec<(u32, (f64, f64))>),
    AddEdges(Vec<(u32, u32)>),
    Circular { scale: f64, center: (f64, f64) },
    Explicit(Vec<(f64, f64)>),
}

impl NativeGraph {
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
}
