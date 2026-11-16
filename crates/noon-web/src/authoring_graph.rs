//! Opaque Graph/DiGraph handles. Python maps its arbitrary keys to `u32`; this
//! module never interprets graph JSON or owns endpoint/layout geometry.

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::authoring_error::js_error;
#[cfg(target_arch = "wasm32")]
use crate::CanonicalAuthoringSceneContext;

#[cfg(target_arch = "wasm32")]
use noon::integration::graph::{manim_stroke_width, pairs, vertices};
#[cfg(target_arch = "wasm32")]
use noon::integration::graph::{GraphOperation, HostGraph};

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
        self.options.vertex_stroke_width =
            manim_stroke_width("vertex stroke width", width).map_err(js_error)?;
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
        self.options.edge_stroke_width =
            manim_stroke_width("edge stroke width", width).map_err(js_error)?;
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
    inner: HostGraph,
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
        let vertex = self.inner.vertex(key)?;
        Some(crate::WasmAuthoringMobjectHandle::from_semantic_mobject(
            vertex.clone(),
        ))
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    /// Build an opaque native Graph/DiGraph through the owning Scene.
    /// Key mapping belongs to the language facade; topology remains in Rust.
    #[wasm_bindgen(js_name = createGraph)]
    pub fn create_graph(
        &mut self,
        directed: bool,
        vertex_ids: &[u32],
        positions: &[f64],
        edge_pairs: &[u32],
        options: WasmGraphOptions,
    ) -> Result<WasmGraphHandle, JsValue> {
        let vertices = vertices(vertex_ids, positions).map_err(js_error)?;
        let edges = pairs(edge_pairs, "Graph edges").map_err(js_error)?;
        self.create_graph_in_scene(directed, vertices, edges, options.options)
            .map(|inner| WasmGraphHandle { inner })
            .map_err(js_error)
    }

    /// Compute a circular layout before publishing any semantic nodes.
    #[wasm_bindgen(js_name = createCircularGraph)]
    pub fn create_circular_graph(
        &mut self,
        directed: bool,
        vertex_ids: &[u32],
        edge_pairs: &[u32],
        scale: f64,
        center_x: f64,
        center_y: f64,
        options: WasmGraphOptions,
    ) -> Result<WasmGraphHandle, JsValue> {
        let edges = pairs(edge_pairs, "Graph edges").map_err(js_error)?;
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
        self.create_graph_in_scene(directed, vertices, edges, options.options)
            .map(|inner| WasmGraphHandle { inner })
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = graphCopy)]
    pub fn graph_copy(&mut self, graph: &WasmGraphHandle) -> Result<WasmGraphHandle, JsValue> {
        self.copy_graph_in_scene(&graph.inner)
            .map(|inner| WasmGraphHandle { inner })
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = graphAddVertices)]
    pub fn graph_add_vertices(
        &mut self,
        graph: &mut WasmGraphHandle,
        vertex_ids: &[u32],
        positions: &[f64],
    ) -> Result<(), JsValue> {
        self.mutate_graph_in_scene(
            &mut graph.inner,
            GraphOperation::AddVertices(vertices(vertex_ids, positions).map_err(js_error)?),
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = graphAddEdges)]
    pub fn graph_add_edges(
        &mut self,
        graph: &mut WasmGraphHandle,
        edge_pairs: &[u32],
    ) -> Result<(), JsValue> {
        self.mutate_graph_in_scene(
            &mut graph.inner,
            GraphOperation::AddEdges(pairs(edge_pairs, "Graph edges").map_err(js_error)?),
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = graphCircularLayout)]
    pub fn graph_circular_layout(
        &mut self,
        graph: &mut WasmGraphHandle,
        scale: f64,
        center_x: f64,
        center_y: f64,
    ) -> Result<(), JsValue> {
        if !scale.is_finite() || !center_x.is_finite() || !center_y.is_finite() {
            return Err(js_error("circular Graph layout values must be finite"));
        }
        self.mutate_graph_in_scene(
            &mut graph.inner,
            GraphOperation::Circular {
                scale,
                center: (center_x, center_y),
            },
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = graphExplicitLayout)]
    pub fn graph_explicit_layout(
        &mut self,
        graph: &mut WasmGraphHandle,
        positions: &[f64],
    ) -> Result<(), JsValue> {
        let ids = (0..positions.len() / 2)
            .map(|id| id as u32)
            .collect::<Vec<_>>();
        let positions = vertices(&ids, positions)
            .map_err(js_error)?
            .into_iter()
            .map(|(_, point)| point)
            .collect();
        self.mutate_graph_in_scene(&mut graph.inner, GraphOperation::Explicit(positions))
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = graphRemoveVertices)]
    pub fn graph_remove_vertices(
        &mut self,
        graph: &mut WasmGraphHandle,
        vertex_ids: &[u32],
    ) -> Result<(), JsValue> {
        self.mutate_graph_in_scene(
            &mut graph.inner,
            GraphOperation::RemoveVertices(vertex_ids.to_vec()),
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = graphRemoveEdges)]
    pub fn graph_remove_edges(
        &mut self,
        graph: &mut WasmGraphHandle,
        edge_pairs: &[u32],
    ) -> Result<(), JsValue> {
        self.mutate_graph_in_scene(
            &mut graph.inner,
            GraphOperation::RemoveEdges(pairs(edge_pairs, "Graph edges").map_err(js_error)?),
        )
        .map_err(js_error)
    }
}
