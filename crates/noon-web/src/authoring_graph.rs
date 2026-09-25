//! Opaque Graph/DiGraph handles. Python maps its arbitrary keys to `u32`; this
//! module never interprets graph JSON or owns endpoint/layout geometry.

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::authoring_error::js_error;
#[cfg(target_arch = "wasm32")]
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
    pub(crate) fn family(&self) -> &noon::MobjectFamily {
        match self {
            Self::Undirected(graph) => graph.family(),
            Self::Directed(graph) => graph.family(),
        }
    }

    pub(crate) fn apply(
        &mut self,
        scene: &mut noon::Scene,
        operation: GraphOperation,
    ) -> Result<(), noon::GraphAuthoringError> {
        match (self, operation) {
            (Self::Undirected(graph), GraphOperation::AddVertices(vertices)) => {
                graph.add_vertices(scene, vertices)?;
            }
            (Self::Directed(graph), GraphOperation::AddVertices(vertices)) => {
                graph.add_vertices(scene, vertices)?;
            }
            (Self::Undirected(graph), GraphOperation::AddEdges(edges)) => {
                graph.add_edges(scene, edges)?;
            }
            (Self::Directed(graph), GraphOperation::AddEdges(edges)) => {
                graph.add_edges(scene, edges)?;
            }
            (Self::Undirected(graph), GraphOperation::RemoveVertices(vertices)) => {
                graph.remove_vertices(scene, vertices)?;
            }
            (Self::Directed(graph), GraphOperation::RemoveVertices(vertices)) => {
                graph.remove_vertices(scene, vertices)?;
            }
            (Self::Undirected(graph), GraphOperation::RemoveEdges(edges)) => {
                graph.remove_edges(scene, edges)?;
            }
            (Self::Directed(graph), GraphOperation::RemoveEdges(edges)) => {
                graph.remove_edges(scene, edges)?;
            }
            (Self::Undirected(graph), GraphOperation::Circular { scale, center }) => {
                graph.change_layout(
                    scene,
                    noon::GraphLayoutOptions {
                        layout: noon::GraphLayout::Circular,
                        scale,
                        center,
                        ..Default::default()
                    },
                )?;
            }
            (Self::Directed(graph), GraphOperation::Circular { scale, center }) => {
                graph.change_layout(
                    scene,
                    noon::GraphLayoutOptions {
                        layout: noon::GraphLayout::Circular,
                        scale,
                        center,
                        ..Default::default()
                    },
                )?;
            }
            (Self::Undirected(graph), GraphOperation::Explicit(positions)) => {
                graph.change_layout_positions(scene, &positions)?;
            }
            (Self::Directed(graph), GraphOperation::Explicit(positions)) => {
                graph.change_layout_positions(scene, &positions)?;
            }
        }
        Ok(())
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

    pub(crate) fn copy_cold(&self) -> Result<Self, noon::GraphAuthoringError> {
        match self {
            Self::Undirected(graph) => graph.copy().map(Self::Undirected),
            Self::Directed(graph) => graph.copy().map(Self::Directed),
        }
    }

    pub(crate) fn copy_live(
        &self,
        live: &mut noon::LiveSession<'_>,
    ) -> Result<Self, noon::GraphAuthoringError> {
        match self {
            Self::Undirected(graph) => graph.copy_live(live).map(Self::Undirected),
            Self::Directed(graph) => graph.copy_live(live).map(Self::Directed),
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn pairs(values: &[u32], name: &str) -> Result<Vec<(u32, u32)>, JsValue> {
    let chunks = values.chunks_exact(2);
    if !chunks.remainder().is_empty() {
        return Err(js_error(format!("{name} requires pairs of vertex IDs")));
    }
    Ok(chunks.map(|pair| (pair[0], pair[1])).collect())
}

#[cfg(target_arch = "wasm32")]
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

#[cfg(target_arch = "wasm32")]
fn manim_stroke_width(name: &str, width: f64) -> Result<f64, JsValue> {
    if !width.is_finite() || width < 0.0 {
        return Err(js_error(format!(
            "{name} must be a finite non-negative number"
        )));
    }
    Ok(width * noon::integration::MANIM_CAIRO_LINE_WIDTH_MULTIPLE)
}

/// Typed global Graph/DiGraph appearance. Python converts public colors and
/// names; this boundary validates components and converts Manim stroke widths
/// before shared GraphOptions builds the retained graph.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct WasmGraphOptions {
    options: noon::GraphOptions,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl WasmGraphOptions {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            options: noon::GraphOptions::default(),
        }
    }

    #[wasm_bindgen(js_name = setVertexRadius)]
    pub fn set_vertex_radius(&mut self, radius: f64) {
        self.options.vertex_radius = radius;
    }

    #[wasm_bindgen(js_name = setVertexFill)]
    pub fn set_vertex_fill(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), JsValue> {
        self.options.vertex_fill = graph_color(red, green, blue, alpha)?;
        Ok(())
    }

    #[wasm_bindgen(js_name = setVertexFillOpacity)]
    pub fn set_vertex_fill_opacity(&mut self, opacity: f64) {
        self.options.vertex_fill_opacity = opacity;
    }

    #[wasm_bindgen(js_name = setVertexStroke)]
    pub fn set_vertex_stroke(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), JsValue> {
        self.options.vertex_stroke = graph_color(red, green, blue, alpha)?;
        Ok(())
    }

    #[wasm_bindgen(js_name = setVertexStrokeWidth)]
    pub fn set_vertex_stroke_width(&mut self, width: f64) -> Result<(), JsValue> {
        self.options.vertex_stroke_width = manim_stroke_width("vertex stroke width", width)?;
        Ok(())
    }

    #[wasm_bindgen(js_name = setEdgeColor)]
    pub fn set_edge_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), JsValue> {
        self.options.edge_color = graph_color(red, green, blue, alpha)?;
        Ok(())
    }

    #[wasm_bindgen(js_name = setEdgeStrokeWidth)]
    pub fn set_edge_stroke_width(&mut self, width: f64) -> Result<(), JsValue> {
        self.options.edge_stroke_width = manim_stroke_width("edge stroke width", width)?;
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
fn graph_color(red: f64, green: f64, blue: f64, alpha: f64) -> Result<noon::Color, JsValue> {
    crate::authoring_mobject::family_color(true, red, green, blue, alpha)
        .map_err(js_error)
        .map(|color| color.expect("enabled graph color"))
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct WasmGraphHandle {
    inner: NativeGraph,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl WasmGraphHandle {
    #[wasm_bindgen(js_name = family)]
    pub fn family(&self) -> crate::WasmAuthoringFamilyHandle {
        crate::WasmAuthoringFamilyHandle::from_semantic_family(self.inner.family().clone())
    }

    /// Resolve one language key without traversing or copying the graph family.
    pub fn vertex(&self, key: u32) -> Option<crate::WasmAuthoringMobjectHandle> {
        let vertex = match &self.inner {
            NativeGraph::Undirected(graph) => graph.vertex(&key),
            NativeGraph::Directed(graph) => graph.vertex(&key),
        }?;
        Some(crate::WasmAuthoringMobjectHandle::from_semantic_mobject(
            vertex.clone(),
        ))
    }
}

#[cfg(target_arch = "wasm32")]
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
        options: WasmGraphOptions,
    ) -> Result<WasmGraphHandle, JsValue> {
        let vertices = vertices(vertex_ids, positions)?;
        let edges = pairs(edge_pairs, "Graph edges")?;
        self.create_live_graph(directed, vertices, edges, options.options)
            .map(|inner| WasmGraphHandle { inner })
            .map_err(js_error)
    }

    /// Compute a circular layout before publishing any semantic nodes.
    #[wasm_bindgen(js_name = liveCreateCircularGraph)]
    pub fn live_create_circular_graph(
        &mut self,
        directed: bool,
        vertex_ids: &[u32],
        edge_pairs: &[u32],
        scale: f64,
        center_x: f64,
        center_y: f64,
        options: WasmGraphOptions,
    ) -> Result<WasmGraphHandle, JsValue> {
        let edges = pairs(edge_pairs, "Graph edges")?;
        let positions = noon::GraphLayoutOptions {
            layout: noon::GraphLayout::Circular,
            scale,
            center: (center_x, center_y),
            ..Default::default()
        }
        .positions(vertex_ids.len(), &[])
        .map_err(|error| {
            js_error(crate::authoring_error::AuthoringFailure::unclassified(
                "graph.layout",
                &error,
            ))
        })?;
        let vertices = vertex_ids.iter().copied().zip(positions).collect();
        self.create_live_graph(directed, vertices, edges, options.options)
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
