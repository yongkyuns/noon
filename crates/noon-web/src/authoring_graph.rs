//! Opaque Graph/DiGraph handles. Python maps its arbitrary keys to `u32`; this
//! module never interprets graph JSON or owns endpoint/layout geometry.

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::authoring_error::js_error;
#[cfg(target_arch = "wasm32")]
use crate::CanonicalAuthoringSceneContext;
#[cfg(target_arch = "wasm32")]
use std::collections::HashMap;

pub(crate) enum NativeGraph {
    Undirected(noon::Graph<u32>),
    Directed(noon::DiGraph<u32>),
}

// Native transport fixtures exercise explicit placement; browser bindings expose every operation.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(crate) enum GraphOperation {
    AddVertices(Vec<(u32, (f64, f64))>),
    AddVerticesWithOptions(Vec<(u32, (f64, f64), noon::GraphOptions)>),
    AddVerticesAndEdgesWithOptions {
        vertices: Vec<(u32, (f64, f64), noon::GraphOptions)>,
        edges: Vec<(u32, u32, noon::GraphOptions)>,
    },
    AddEdges(Vec<(u32, u32)>),
    AddEdgesWithOptions(Vec<(u32, u32, noon::GraphOptions)>),
    RemoveVertices(Vec<u32>),
    RemoveEdges(Vec<(u32, u32)>),
    Circular {
        scale: f64,
        scale_y: Option<f64>,
        center: (f64, f64),
    },
    Shell {
        shells: Vec<Vec<usize>>,
        scale: f64,
        center: (f64, f64),
        rotate: Option<f64>,
    },
    Partite {
        partitions: Vec<Vec<usize>>,
        scale: f64,
        scale_y: Option<f64>,
        center: (f64, f64),
        align_horizontal: bool,
    },
    Spiral {
        scale: f64,
        scale_y: Option<f64>,
        center: (f64, f64),
        resolution: f64,
        equidistant: bool,
    },
    Random(noon::GraphLayoutOptions),
    Spring(noon::GraphLayoutOptions),
    KamadaKawai(noon::GraphLayoutOptions),
    Spectral(noon::GraphLayoutOptions),
    Tree(noon::GraphLayoutOptions),
    Explicit(Vec<(f64, f64)>),
}

impl NativeGraph {
    pub(crate) fn family(&self) -> &noon::MobjectFamily {
        match self {
            Self::Undirected(graph) => graph.family(),
            Self::Directed(graph) => graph.family(),
        }
    }

    #[cfg(target_arch = "wasm32")]
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
            (Self::Undirected(graph), GraphOperation::AddVerticesWithOptions(vertices)) => {
                graph.add_vertices_with_options(scene, vertices)?;
            }
            (Self::Directed(graph), GraphOperation::AddVerticesWithOptions(vertices)) => {
                graph.add_vertices_with_options(scene, vertices)?;
            }
            (
                Self::Undirected(graph),
                GraphOperation::AddVerticesAndEdgesWithOptions { vertices, edges },
            ) => {
                graph.add_vertices_and_edges_with_options(scene, vertices, edges)?;
            }
            (
                Self::Directed(graph),
                GraphOperation::AddVerticesAndEdgesWithOptions { vertices, edges },
            ) => {
                graph.add_vertices_and_edges_with_options(scene, vertices, edges)?;
            }
            (Self::Undirected(graph), GraphOperation::AddEdges(edges)) => {
                graph.add_edges(scene, edges)?;
            }
            (Self::Directed(graph), GraphOperation::AddEdges(edges)) => {
                graph.add_edges(scene, edges)?;
            }
            (Self::Undirected(graph), GraphOperation::AddEdgesWithOptions(edges)) => {
                graph.add_edges_with_options(scene, edges)?;
            }
            (Self::Directed(graph), GraphOperation::AddEdgesWithOptions(edges)) => {
                graph.add_edges_with_options(scene, edges)?;
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
            (
                Self::Undirected(graph),
                GraphOperation::Circular {
                    scale,
                    scale_y,
                    center,
                },
            ) => {
                graph.change_layout(
                    scene,
                    noon::GraphLayoutOptions {
                        layout: noon::GraphLayout::Circular,
                        scale,
                        scale_y,
                        center,
                        ..Default::default()
                    },
                )?;
            }
            (
                Self::Directed(graph),
                GraphOperation::Circular {
                    scale,
                    scale_y,
                    center,
                },
            ) => {
                graph.change_layout(
                    scene,
                    noon::GraphLayoutOptions {
                        layout: noon::GraphLayout::Circular,
                        scale,
                        scale_y,
                        center,
                        ..Default::default()
                    },
                )?;
            }
            (
                Self::Undirected(graph),
                GraphOperation::Shell {
                    shells,
                    scale,
                    center,
                    rotate,
                },
            ) => {
                graph.change_layout(
                    scene,
                    noon::GraphLayoutOptions {
                        layout: noon::GraphLayout::Shell,
                        scale,
                        center,
                        shells,
                        rotate,
                        ..Default::default()
                    },
                )?;
            }
            (
                Self::Directed(graph),
                GraphOperation::Shell {
                    shells,
                    scale,
                    center,
                    rotate,
                },
            ) => {
                graph.change_layout(
                    scene,
                    noon::GraphLayoutOptions {
                        layout: noon::GraphLayout::Shell,
                        scale,
                        center,
                        shells,
                        rotate,
                        ..Default::default()
                    },
                )?;
            }
            (
                Self::Undirected(graph),
                GraphOperation::Partite {
                    partitions,
                    scale,
                    scale_y,
                    center,
                    align_horizontal,
                },
            ) => {
                graph.change_layout(
                    scene,
                    noon::GraphLayoutOptions {
                        layout: noon::GraphLayout::Partite,
                        scale,
                        scale_y,
                        center,
                        partitions,
                        align_horizontal,
                        ..Default::default()
                    },
                )?;
            }
            (
                Self::Directed(graph),
                GraphOperation::Partite {
                    partitions,
                    scale,
                    scale_y,
                    center,
                    align_horizontal,
                },
            ) => {
                graph.change_layout(
                    scene,
                    noon::GraphLayoutOptions {
                        layout: noon::GraphLayout::Partite,
                        scale,
                        scale_y,
                        center,
                        partitions,
                        align_horizontal,
                        ..Default::default()
                    },
                )?;
            }
            (
                Self::Undirected(graph),
                GraphOperation::Spiral {
                    scale,
                    scale_y,
                    center,
                    resolution,
                    equidistant,
                },
            ) => {
                graph.change_layout(
                    scene,
                    noon::GraphLayoutOptions {
                        layout: noon::GraphLayout::Spiral,
                        scale,
                        scale_y,
                        center,
                        resolution,
                        equidistant,
                        ..Default::default()
                    },
                )?;
            }
            (
                Self::Directed(graph),
                GraphOperation::Spiral {
                    scale,
                    scale_y,
                    center,
                    resolution,
                    equidistant,
                },
            ) => {
                graph.change_layout(
                    scene,
                    noon::GraphLayoutOptions {
                        layout: noon::GraphLayout::Spiral,
                        scale,
                        scale_y,
                        center,
                        resolution,
                        equidistant,
                        ..Default::default()
                    },
                )?;
            }
            (Self::Undirected(graph), GraphOperation::Spring(options)) => {
                graph.change_layout(scene, options)?;
            }
            (Self::Directed(graph), GraphOperation::Spring(options)) => {
                graph.change_layout(scene, options)?;
            }
            (Self::Undirected(graph), GraphOperation::KamadaKawai(options)) => {
                graph.change_layout(scene, options)?;
            }
            (Self::Directed(graph), GraphOperation::KamadaKawai(mut options)) => {
                options.directed = true;
                graph.change_layout(scene, options)?;
            }
            (Self::Undirected(graph), GraphOperation::Spectral(options)) => {
                graph.change_layout(scene, options)?;
            }
            (Self::Directed(graph), GraphOperation::Spectral(mut options)) => {
                options.directed = true;
                graph.change_layout(scene, options)?;
            }
            (Self::Undirected(graph), GraphOperation::Random(options)) => {
                graph.change_layout(scene, options)?;
            }
            (Self::Directed(graph), GraphOperation::Random(options)) => {
                graph.change_layout(scene, options)?;
            }
            (Self::Undirected(graph), GraphOperation::Tree(options)) => {
                graph.change_layout(scene, options)?;
            }
            (Self::Directed(graph), GraphOperation::Tree(mut options)) => {
                options.directed = true;
                graph.change_layout(scene, options)?;
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
            (Self::Undirected(graph), GraphOperation::AddVerticesWithOptions(vertices)) => {
                graph.add_vertices_live_with_options(live, vertices)?;
            }
            (Self::Directed(graph), GraphOperation::AddVerticesWithOptions(vertices)) => {
                graph.add_vertices_live_with_options(live, vertices)?;
            }
            (
                Self::Undirected(graph),
                GraphOperation::AddVerticesAndEdgesWithOptions { vertices, edges },
            ) => {
                graph.add_vertices_and_edges_live_with_options(live, vertices, edges)?;
            }
            (
                Self::Directed(graph),
                GraphOperation::AddVerticesAndEdgesWithOptions { vertices, edges },
            ) => {
                graph.add_vertices_and_edges_live_with_options(live, vertices, edges)?;
            }
            (Self::Undirected(graph), GraphOperation::AddEdges(edges)) => {
                graph.add_edges_live(live, edges)?;
            }
            (Self::Directed(graph), GraphOperation::AddEdges(edges)) => {
                graph.add_edges_live(live, edges)?;
            }
            (Self::Undirected(graph), GraphOperation::AddEdgesWithOptions(edges)) => {
                graph.add_edges_live_with_options(live, edges)?;
            }
            (Self::Directed(graph), GraphOperation::AddEdgesWithOptions(edges)) => {
                graph.add_edges_live_with_options(live, edges)?;
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
            (
                Self::Undirected(graph),
                GraphOperation::Circular {
                    scale,
                    scale_y,
                    center,
                },
            ) => {
                graph.change_layout_live(
                    live,
                    noon::GraphLayoutOptions {
                        layout: noon::GraphLayout::Circular,
                        scale,
                        scale_y,
                        center,
                        ..Default::default()
                    },
                )?;
            }
            (
                Self::Directed(graph),
                GraphOperation::Circular {
                    scale,
                    scale_y,
                    center,
                },
            ) => {
                graph.change_layout_live(
                    live,
                    noon::GraphLayoutOptions {
                        layout: noon::GraphLayout::Circular,
                        scale,
                        scale_y,
                        center,
                        ..Default::default()
                    },
                )?;
            }
            (
                Self::Undirected(graph),
                GraphOperation::Shell {
                    shells,
                    scale,
                    center,
                    rotate,
                },
            ) => {
                graph.change_layout_live(
                    live,
                    noon::GraphLayoutOptions {
                        layout: noon::GraphLayout::Shell,
                        scale,
                        center,
                        shells,
                        rotate,
                        ..Default::default()
                    },
                )?;
            }
            (
                Self::Directed(graph),
                GraphOperation::Shell {
                    shells,
                    scale,
                    center,
                    rotate,
                },
            ) => {
                graph.change_layout_live(
                    live,
                    noon::GraphLayoutOptions {
                        layout: noon::GraphLayout::Shell,
                        scale,
                        center,
                        shells,
                        rotate,
                        ..Default::default()
                    },
                )?;
            }
            (
                Self::Undirected(graph),
                GraphOperation::Partite {
                    partitions,
                    scale,
                    scale_y,
                    center,
                    align_horizontal,
                },
            ) => {
                graph.change_layout_live(
                    live,
                    noon::GraphLayoutOptions {
                        layout: noon::GraphLayout::Partite,
                        scale,
                        scale_y,
                        center,
                        partitions,
                        align_horizontal,
                        ..Default::default()
                    },
                )?;
            }
            (
                Self::Directed(graph),
                GraphOperation::Partite {
                    partitions,
                    scale,
                    scale_y,
                    center,
                    align_horizontal,
                },
            ) => {
                graph.change_layout_live(
                    live,
                    noon::GraphLayoutOptions {
                        layout: noon::GraphLayout::Partite,
                        scale,
                        scale_y,
                        center,
                        partitions,
                        align_horizontal,
                        ..Default::default()
                    },
                )?;
            }
            (
                Self::Undirected(graph),
                GraphOperation::Spiral {
                    scale,
                    scale_y,
                    center,
                    resolution,
                    equidistant,
                },
            ) => {
                graph.change_layout_live(
                    live,
                    noon::GraphLayoutOptions {
                        layout: noon::GraphLayout::Spiral,
                        scale,
                        scale_y,
                        center,
                        resolution,
                        equidistant,
                        ..Default::default()
                    },
                )?;
            }
            (
                Self::Directed(graph),
                GraphOperation::Spiral {
                    scale,
                    scale_y,
                    center,
                    resolution,
                    equidistant,
                },
            ) => {
                graph.change_layout_live(
                    live,
                    noon::GraphLayoutOptions {
                        layout: noon::GraphLayout::Spiral,
                        scale,
                        scale_y,
                        center,
                        resolution,
                        equidistant,
                        ..Default::default()
                    },
                )?;
            }
            (Self::Undirected(graph), GraphOperation::Spring(options)) => {
                graph.change_layout_live(live, options)?;
            }
            (Self::Directed(graph), GraphOperation::Spring(options)) => {
                graph.change_layout_live(live, options)?;
            }
            (Self::Undirected(graph), GraphOperation::KamadaKawai(options)) => {
                graph.change_layout_live(live, options)?;
            }
            (Self::Directed(graph), GraphOperation::KamadaKawai(mut options)) => {
                options.directed = true;
                graph.change_layout_live(live, options)?;
            }
            (Self::Undirected(graph), GraphOperation::Spectral(options)) => {
                graph.change_layout_live(live, options)?;
            }
            (Self::Directed(graph), GraphOperation::Spectral(mut options)) => {
                options.directed = true;
                graph.change_layout_live(live, options)?;
            }
            (Self::Undirected(graph), GraphOperation::Random(options)) => {
                graph.change_layout_live(live, options)?;
            }
            (Self::Directed(graph), GraphOperation::Random(options)) => {
                graph.change_layout_live(live, options)?;
            }
            (Self::Undirected(graph), GraphOperation::Tree(options)) => {
                graph.change_layout_live(live, options)?;
            }
            (Self::Directed(graph), GraphOperation::Tree(mut options)) => {
                options.directed = true;
                graph.change_layout_live(live, options)?;
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

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn copy_cold(&self) -> Result<Self, noon::GraphAuthoringError> {
        match self {
            Self::Undirected(graph) => graph.copy().map(Self::Undirected),
            Self::Directed(graph) => graph.copy().map(Self::Directed),
        }
    }

    #[cfg(target_arch = "wasm32")]
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
fn grouped_indices(indices: &[u32], sizes: &[u32], name: &str) -> Result<Vec<Vec<usize>>, JsValue> {
    let total = sizes
        .iter()
        .try_fold(0usize, |total, &size| total.checked_add(size as usize));
    if total != Some(indices.len()) {
        return Err(js_error(format!(
            "{name} sizes must cover the grouped vertex indices"
        )));
    }
    let mut offset = 0usize;
    Ok(sizes
        .iter()
        .map(|&size| {
            let end = offset + size as usize;
            let shell = indices[offset..end]
                .iter()
                .map(|&index| index as usize)
                .collect();
            offset = end;
            shell
        })
        .collect())
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

/// Decode sparse per-vertex overrides. Each row contains radius, fill RGBA,
/// fill opacity, stroke RGBA, Manim stroke width and stroke opacity; NaN means inherit the
/// batch's global GraphOptions value. A color is either wholly omitted or has
/// four finite components.
#[cfg(target_arch = "wasm32")]
fn vertex_options_with_overrides(
    base: &noon::GraphOptions,
    overrides: &[f64],
    vertex_count: usize,
) -> Result<Vec<noon::GraphOptions>, JsValue> {
    const ROW_WIDTH: usize = 12;
    if overrides.len() != vertex_count * ROW_WIDTH {
        return Err(js_error(
            "Graph vertex style overrides have an invalid length",
        ));
    }

    overrides
        .chunks_exact(ROW_WIDTH)
        .map(|row| {
            let mut options = base.clone();
            if !row[0].is_nan() {
                options.vertex_radius = row[0];
            }
            if !row[1..5].iter().all(|component| component.is_nan()) {
                options.vertex_fill = graph_color(row[1], row[2], row[3], row[4])?;
            }
            if !row[5].is_nan() {
                options.vertex_fill_opacity = row[5];
            }
            if !row[6..10].iter().all(|component| component.is_nan()) {
                options.vertex_stroke = graph_color(row[6], row[7], row[8], row[9])?;
            }
            if !row[10].is_nan() {
                options.vertex_stroke_width = manim_stroke_width("vertex stroke width", row[10])?;
            }
            if !row[11].is_nan() {
                options.vertex_stroke_opacity = row[11];
            }
            Ok(options)
        })
        .collect()
}

/// Decode sparse per-edge overrides. Each row contains edge RGBA, Manim
/// stroke width and stroke opacity; NaN means inherit the batch's global GraphOptions value.
#[cfg(target_arch = "wasm32")]
fn edge_options_with_overrides(
    base: &noon::GraphOptions,
    overrides: &[f64],
    edge_count: usize,
) -> Result<Vec<noon::GraphOptions>, JsValue> {
    const ROW_WIDTH: usize = 6;
    if overrides.len() != edge_count * ROW_WIDTH {
        return Err(js_error(
            "Graph edge style overrides have an invalid length",
        ));
    }

    overrides
        .chunks_exact(ROW_WIDTH)
        .map(|row| {
            let mut options = base.clone();
            if !row[..4].iter().all(|component| component.is_nan()) {
                options.edge_color = graph_color(row[0], row[1], row[2], row[3])?;
            }
            if !row[4].is_nan() {
                options.edge_stroke_width = manim_stroke_width("edge stroke width", row[4])?;
            }
            if !row[5].is_nan() {
                options.edge_stroke_opacity = row[5];
            }
            Ok(options)
        })
        .collect()
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

    #[wasm_bindgen(js_name = labeledDotRadius)]
    pub fn labeled_dot_radius(width: f64, height: f64, buff: f64) -> f64 {
        noon::GraphVertexStyle::labeled_dot_radius(width, height, buff)
    }

    #[wasm_bindgen(js_name = setVertexRadius)]
    pub fn set_vertex_radius(&mut self, radius: f64) {
        self.options.vertex_radius = radius;
    }

    /// Constructor styles are inert inputs. Shared Rust graph construction
    /// validates all of them before admitting any semantic node or resource.
    #[wasm_bindgen(js_name = setVertexStyleFor)]
    pub fn set_vertex_style_for(&mut self, vertex_index: u32, style: WasmGraphOptions) {
        self.options
            .vertex_styles
            .insert(vertex_index as usize, style.options.vertex_style());
    }

    #[wasm_bindgen(js_name = setEdgeStyleFor)]
    pub fn set_edge_style_for(&mut self, edge_index: u32, style: WasmGraphOptions) {
        self.options
            .edge_styles
            .insert(edge_index as usize, style.options.edge_style());
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

    #[wasm_bindgen(js_name = setVertexStrokeOpacity)]
    pub fn set_vertex_stroke_opacity(&mut self, opacity: f64) {
        self.options.vertex_stroke_opacity = opacity;
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

    #[wasm_bindgen(js_name = setEdgeStrokeOpacity)]
    pub fn set_edge_stroke_opacity(&mut self, opacity: f64) {
        self.options.edge_stroke_opacity = opacity;
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

    /// Project canonical insertion-ordered endpoints for host key adaptation.
    #[wasm_bindgen(js_name = edgeKeys)]
    pub fn edge_keys(&self) -> Result<Vec<u32>, JsValue> {
        let pairs = match &self.inner {
            NativeGraph::Undirected(graph) => graph.edge_keys(),
            NativeGraph::Directed(graph) => graph.edge_keys(),
        }
        .map_err(|error| {
            js_error(crate::authoring_error::AuthoringFailure::unclassified(
                "graph.edge_keys",
                &error,
            ))
        })?;
        Ok(pairs
            .into_iter()
            .flat_map(|(start, end)| [*start, *end])
            .collect())
    }

    /// Resolve one edge to its retained semantic family, including arrow tips.
    #[wasm_bindgen(js_name = edge)]
    pub fn edge(&self, start: u32, end: u32) -> Option<crate::WasmAuthoringFamilyHandle> {
        let edge = match &self.inner {
            NativeGraph::Undirected(graph) => graph.edge(&start, &end),
            NativeGraph::Directed(graph) => graph.edge(&start, &end),
        }?;
        Some(crate::WasmAuthoringFamilyHandle::from_semantic_family(
            edge.family().clone(),
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
        scale_y: Option<f64>,
        center_x: f64,
        center_y: f64,
        options: WasmGraphOptions,
    ) -> Result<WasmGraphHandle, JsValue> {
        let edges = pairs(edge_pairs, "Graph edges")?;
        let positions = noon::GraphLayoutOptions {
            layout: noon::GraphLayout::Circular,
            scale,
            scale_y,
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

    /// Compute a deterministic spectral layout in shared Rust before publishing.
    #[wasm_bindgen(js_name = liveCreateSpectralGraph)]
    pub fn live_create_spectral_graph(
        &mut self,
        directed: bool,
        vertex_ids: &[u32],
        edge_pairs: &[u32],
        edge_weights: &[f64],
        scale: f64,
        scale_y: Option<f64>,
        center_x: f64,
        center_y: f64,
        options: WasmGraphOptions,
    ) -> Result<WasmGraphHandle, JsValue> {
        let edges = pairs(edge_pairs, "Graph edges")?;
        let vertex_indices = vertex_ids
            .iter()
            .enumerate()
            .map(|(index, &id)| (id, index))
            .collect::<HashMap<_, _>>();
        let edge_indices = edges
            .iter()
            .map(|&(start, end)| {
                let start = vertex_indices
                    .get(&start)
                    .copied()
                    .ok_or_else(|| js_error("Graph edge references an unknown vertex"))?;
                let end = vertex_indices
                    .get(&end)
                    .copied()
                    .ok_or_else(|| js_error("Graph edge references an unknown vertex"))?;
                Ok((start, end))
            })
            .collect::<Result<Vec<_>, JsValue>>()?;
        let positions = noon::GraphLayoutOptions {
            layout: noon::GraphLayout::Spectral,
            scale,
            scale_y,
            center: (center_x, center_y),
            directed,
            edge_weights: Some(edge_weights.to_vec()),
            ..Default::default()
        }
        .positions(vertex_ids.len(), &edge_indices)
        .map_err(|error| {
            js_error(crate::authoring_error::AuthoringFailure::unclassified(
                "graph.spectral_layout",
                &error,
            ))
        })?;
        let vertices = vertex_ids.iter().copied().zip(positions).collect();
        self.create_live_graph(directed, vertices, edges, options.options)
            .map(|inner| WasmGraphHandle { inner })
            .map_err(js_error)
    }

    /// Compute a Kamada-Kawai layout in shared Rust before publishing nodes.
    #[wasm_bindgen(js_name = liveCreateKamadaKawaiGraph)]
    pub fn live_create_kamada_kawai_graph(
        &mut self,
        directed: bool,
        vertex_ids: &[u32],
        edge_pairs: &[u32],
        edge_weights: &[f64],
        distances: &[f64],
        initial_positions: &[f64],
        scale: f64,
        scale_y: Option<f64>,
        center_x: f64,
        center_y: f64,
        options: WasmGraphOptions,
    ) -> Result<WasmGraphHandle, JsValue> {
        let edges = pairs(edge_pairs, "Graph edges")?;
        let vertex_indices = vertex_ids
            .iter()
            .enumerate()
            .map(|(index, &id)| (id, index))
            .collect::<HashMap<_, _>>();
        let edge_indices = edges
            .iter()
            .map(|&(start, end)| {
                let start = vertex_indices
                    .get(&start)
                    .copied()
                    .ok_or_else(|| js_error("Graph edge references an unknown vertex"))?;
                let end = vertex_indices
                    .get(&end)
                    .copied()
                    .ok_or_else(|| js_error("Graph edge references an unknown vertex"))?;
                Ok((start, end))
            })
            .collect::<Result<Vec<_>, JsValue>>()?;
        let position_ids = (0..vertex_ids.len() as u32).collect::<Vec<_>>();
        let initial_positions = if initial_positions.is_empty() {
            None
        } else {
            Some(
                vertices(&position_ids, initial_positions)?
                    .into_iter()
                    .map(|(_, position)| position)
                    .collect(),
            )
        };
        let distances = (!distances.is_empty()).then(|| distances.to_vec());
        let positions = noon::GraphLayoutOptions {
            layout: noon::GraphLayout::KamadaKawai,
            scale,
            scale_y,
            center: (center_x, center_y),
            directed,
            edge_weights: Some(edge_weights.to_vec()),
            kamada_kawai_distances: distances,
            kamada_kawai_positions: initial_positions,
            ..Default::default()
        }
        .positions(vertex_ids.len(), &edge_indices)
        .map_err(|error| {
            js_error(crate::authoring_error::AuthoringFailure::unclassified(
                "graph.kamada_kawai_layout",
                &error,
            ))
        })?;
        let vertices = vertex_ids.iter().copied().zip(positions).collect();
        self.create_live_graph(directed, vertices, edges, options.options)
            .map(|inner| WasmGraphHandle { inner })
            .map_err(js_error)
    }

    /// Compute a NetworkX-compatible 2D shell layout in the shared Rust graph
    /// implementation before publishing semantic nodes.
    #[wasm_bindgen(js_name = liveCreateShellGraph)]
    pub fn live_create_shell_graph(
        &mut self,
        directed: bool,
        vertex_ids: &[u32],
        edge_pairs: &[u32],
        shell_vertex_indices: &[u32],
        shell_sizes: &[u32],
        scale: f64,
        center_x: f64,
        center_y: f64,
        rotate: Option<f64>,
        options: WasmGraphOptions,
    ) -> Result<WasmGraphHandle, JsValue> {
        let edges = pairs(edge_pairs, "Graph edges")?;
        let shells = grouped_indices(shell_vertex_indices, shell_sizes, "Graph shell")?;
        let positions = noon::GraphLayoutOptions {
            layout: noon::GraphLayout::Shell,
            scale,
            center: (center_x, center_y),
            shells,
            rotate,
            ..Default::default()
        }
        .positions(vertex_ids.len(), &[])
        .map_err(|error| {
            js_error(crate::authoring_error::AuthoringFailure::unclassified(
                "graph.shell_layout",
                &error,
            ))
        })?;
        let vertices = vertex_ids.iter().copied().zip(positions).collect();
        self.create_live_graph(directed, vertices, edges, options.options)
            .map(|inner| WasmGraphHandle { inner })
            .map_err(js_error)
    }

    /// Compute a NetworkX-compatible multipartite layout before publishing nodes.
    #[wasm_bindgen(js_name = liveCreatePartiteGraph)]
    pub fn live_create_partite_graph(
        &mut self,
        directed: bool,
        vertex_ids: &[u32],
        edge_pairs: &[u32],
        partition_vertex_indices: &[u32],
        partition_sizes: &[u32],
        scale: f64,
        scale_y: Option<f64>,
        center_x: f64,
        center_y: f64,
        align_horizontal: bool,
        options: WasmGraphOptions,
    ) -> Result<WasmGraphHandle, JsValue> {
        let edges = pairs(edge_pairs, "Graph edges")?;
        let partitions =
            grouped_indices(partition_vertex_indices, partition_sizes, "Graph partition")?;
        let positions = noon::GraphLayoutOptions {
            layout: noon::GraphLayout::Partite,
            scale,
            scale_y,
            center: (center_x, center_y),
            partitions,
            align_horizontal,
            ..Default::default()
        }
        .positions(vertex_ids.len(), &[])
        .map_err(|error| {
            js_error(crate::authoring_error::AuthoringFailure::unclassified(
                "graph.partite_layout",
                &error,
            ))
        })?;
        let vertices = vertex_ids.iter().copied().zip(positions).collect();
        self.create_live_graph(directed, vertices, edges, options.options)
            .map(|inner| WasmGraphHandle { inner })
            .map_err(js_error)
    }

    /// Compute a NetworkX-compatible 2D spiral layout before publishing nodes.
    #[wasm_bindgen(js_name = liveCreateSpiralGraph)]
    pub fn live_create_spiral_graph(
        &mut self,
        directed: bool,
        vertex_ids: &[u32],
        edge_pairs: &[u32],
        scale: f64,
        scale_y: Option<f64>,
        center_x: f64,
        center_y: f64,
        resolution: f64,
        equidistant: bool,
        options: WasmGraphOptions,
    ) -> Result<WasmGraphHandle, JsValue> {
        let edges = pairs(edge_pairs, "Graph edges")?;
        let positions = noon::GraphLayoutOptions {
            layout: noon::GraphLayout::Spiral,
            scale,
            scale_y,
            center: (center_x, center_y),
            resolution,
            equidistant,
            ..Default::default()
        }
        .positions(vertex_ids.len(), &[])
        .map_err(|error| {
            js_error(crate::authoring_error::AuthoringFailure::unclassified(
                "graph.spiral_layout",
                &error,
            ))
        })?;
        let vertices = vertex_ids.iter().copied().zip(positions).collect();
        self.create_live_graph(directed, vertices, edges, options.options)
            .map(|inner| WasmGraphHandle { inner })
            .map_err(js_error)
    }

    /// Transform prepared NetworkX random positions with shared Manim graph semantics.
    #[wasm_bindgen(js_name = liveCreateRandomGraph)]
    pub fn live_create_random_graph(
        &mut self,
        directed: bool,
        vertex_ids: &[u32],
        edge_pairs: &[u32],
        random_positions: &[f64],
        scale: f64,
        scale_y: Option<f64>,
        options: WasmGraphOptions,
    ) -> Result<WasmGraphHandle, JsValue> {
        let edges = pairs(edge_pairs, "Graph edges")?;
        let position_ids = (0..random_positions.len() / 2)
            .map(|index| index as u32)
            .collect::<Vec<_>>();
        let random_positions = vertices(&position_ids, random_positions)?
            .into_iter()
            .map(|(_, position)| position)
            .collect();
        let positions = noon::GraphLayoutOptions {
            layout: noon::GraphLayout::Random,
            scale,
            scale_y,
            random_positions: Some(random_positions),
            ..Default::default()
        }
        .positions(vertex_ids.len(), &[])
        .map_err(|error| {
            js_error(crate::authoring_error::AuthoringFailure::unclassified(
                "graph.random_layout",
                &error,
            ))
        })?;
        let vertices = vertex_ids.iter().copied().zip(positions).collect();
        self.create_live_graph(directed, vertices, edges, options.options)
            .map(|inner| WasmGraphHandle { inner })
            .map_err(js_error)
    }

    /// Compute Manim's deterministic tree placement before publishing graph nodes.
    #[wasm_bindgen(js_name = liveCreateTreeGraph)]
    pub fn live_create_tree_graph(
        &mut self,
        directed: bool,
        vertex_ids: &[u32],
        edge_pairs: &[u32],
        root_index: u32,
        orientation_up: bool,
        spacing_x: Option<f64>,
        spacing_y: Option<f64>,
        scale: f64,
        scale_y: Option<f64>,
        options: WasmGraphOptions,
    ) -> Result<WasmGraphHandle, JsValue> {
        let edges = pairs(edge_pairs, "Graph edges")?;
        let vertex_indices = vertex_ids
            .iter()
            .enumerate()
            .map(|(index, &id)| (id, index))
            .collect::<HashMap<_, _>>();
        let edge_indices = edges
            .iter()
            .map(|&(start, end)| {
                let start = vertex_indices
                    .get(&start)
                    .copied()
                    .ok_or_else(|| js_error("Graph edge references an unknown vertex"))?;
                let end = vertex_indices
                    .get(&end)
                    .copied()
                    .ok_or_else(|| js_error("Graph edge references an unknown vertex"))?;
                Ok((start, end))
            })
            .collect::<Result<Vec<_>, JsValue>>()?;
        let vertex_spacing = match (spacing_x, spacing_y) {
            (Some(x), Some(y)) => Some((x, y)),
            (None, None) => None,
            _ => return Err(js_error("tree vertex spacing requires both x and y values")),
        };
        let positions = noon::GraphLayoutOptions {
            layout: noon::GraphLayout::Tree,
            scale,
            scale_y,
            tree_root: Some(root_index as usize),
            tree_orientation_up: orientation_up,
            tree_vertex_spacing: vertex_spacing,
            directed,
            ..Default::default()
        }
        .positions(vertex_ids.len(), &edge_indices)
        .map_err(|error| {
            js_error(crate::authoring_error::AuthoringFailure::unclassified(
                "graph.tree_layout",
                &error,
            ))
        })?;
        let vertices = vertex_ids.iter().copied().zip(positions).collect();
        self.create_live_graph(directed, vertices, edges, options.options)
            .map(|inner| WasmGraphHandle { inner })
            .map_err(js_error)
    }

    /// Prepare NumPy-seeded initial positions in Python, then let shared Rust
    /// run the NetworkX Fruchterman-Reingold integration.
    #[wasm_bindgen(js_name = liveCreateSpringGraph)]
    pub fn live_create_spring_graph(
        &mut self,
        directed: bool,
        vertex_ids: &[u32],
        edge_pairs: &[u32],
        edge_weights: &[f64],
        initial_positions: &[f64],
        fixed_vertex_indices: &[u32],
        fixed_is_set: bool,
        scale: f64,
        scale_y: Option<f64>,
        center_x: f64,
        center_y: f64,
        k: Option<f64>,
        iterations: u32,
        threshold: f64,
        rescale: bool,
        options: WasmGraphOptions,
    ) -> Result<WasmGraphHandle, JsValue> {
        let edges = pairs(edge_pairs, "Graph edges")?;
        let vertex_indices = vertex_ids
            .iter()
            .enumerate()
            .map(|(index, &id)| (id, index))
            .collect::<HashMap<_, _>>();
        let edge_indices = edges
            .iter()
            .map(|&(start, end)| {
                let start = vertex_indices
                    .get(&start)
                    .copied()
                    .ok_or_else(|| js_error("Graph edge references an unknown vertex"))?;
                let end = vertex_indices
                    .get(&end)
                    .copied()
                    .ok_or_else(|| js_error("Graph edge references an unknown vertex"))?;
                Ok((start, end))
            })
            .collect::<Result<Vec<_>, JsValue>>()?;
        let position_ids = (0..vertex_ids.len())
            .map(|index| index as u32)
            .collect::<Vec<_>>();
        let initial_positions = vertices(&position_ids, initial_positions)?
            .into_iter()
            .map(|(_, position)| position)
            .collect();
        let spring_fixed = if fixed_is_set {
            Some(
                fixed_vertex_indices
                    .iter()
                    .map(|&index| index as usize)
                    .collect(),
            )
        } else if fixed_vertex_indices.is_empty() {
            None
        } else {
            return Err(js_error("spring fixed indices require fixed to be set"));
        };
        let positions = noon::GraphLayoutOptions {
            layout: noon::GraphLayout::Spring,
            scale,
            scale_y,
            center: (center_x, center_y),
            iterations: iterations as usize,
            threshold,
            spring_k: k,
            edge_weights: Some(edge_weights.to_vec()),
            spring_positions: Some(initial_positions),
            spring_fixed,
            spring_rescale: rescale,
            directed,
            ..Default::default()
        }
        .positions(vertex_ids.len(), &edge_indices)
        .map_err(|error| {
            js_error(crate::authoring_error::AuthoringFailure::unclassified(
                "graph.spring_layout",
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

    /// Add vertices and their optional per-key appearance in one retained
    /// graph publication. Style rows encode radius, fill RGBA, fill opacity,
    /// stroke RGBA, and Manim stroke width; NaN components inherit `options`.
    #[wasm_bindgen(js_name = liveGraphAddVerticesWithOptions)]
    pub fn live_graph_add_vertices_with_options(
        &mut self,
        graph: &mut WasmGraphHandle,
        vertex_ids: &[u32],
        positions: &[f64],
        style_overrides: &[f64],
        options: WasmGraphOptions,
    ) -> Result<(), JsValue> {
        let vertices = vertices(vertex_ids, positions)?;
        let styles =
            vertex_options_with_overrides(&options.options, style_overrides, vertices.len())?;
        let styled_vertices = vertices
            .into_iter()
            .zip(styles)
            .map(|((id, position), options)| (id, position, options))
            .collect();
        self.mutate_live_graph(
            &mut graph.inner,
            GraphOperation::AddVerticesWithOptions(styled_vertices),
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

    /// Add edges and their optional per-edge appearance in one retained graph
    /// publication. Style rows encode edge RGBA and Manim stroke width; NaN
    /// components inherit `options`.
    #[wasm_bindgen(js_name = liveGraphAddEdgesWithOptions)]
    pub fn live_graph_add_edges_with_options(
        &mut self,
        graph: &mut WasmGraphHandle,
        edge_pairs: &[u32],
        style_overrides: &[f64],
        options: WasmGraphOptions,
    ) -> Result<(), JsValue> {
        let edges = pairs(edge_pairs, "Graph edges")?;
        let styles = edge_options_with_overrides(&options.options, style_overrides, edges.len())?;
        let styled_edges = edges
            .into_iter()
            .zip(styles)
            .map(|((start, end), options)| (start, end, options))
            .collect();
        self.mutate_live_graph(
            &mut graph.inner,
            GraphOperation::AddEdgesWithOptions(styled_edges),
        )
        .map_err(js_error)
    }

    /// Add previously missing vertices and their edges through one semantic
    /// transaction. Vertex and edge style rows use the same sparse layouts as
    /// the corresponding individual topology-edit operations.
    #[wasm_bindgen(js_name = liveGraphAddVerticesAndEdgesWithOptions)]
    pub fn live_graph_add_vertices_and_edges_with_options(
        &mut self,
        graph: &mut WasmGraphHandle,
        vertex_ids: &[u32],
        positions: &[f64],
        vertex_style_overrides: &[f64],
        vertex_options: WasmGraphOptions,
        edge_pairs: &[u32],
        edge_style_overrides: &[f64],
        edge_options: WasmGraphOptions,
    ) -> Result<(), JsValue> {
        let vertices = vertices(vertex_ids, positions)?;
        let vertex_styles = vertex_options_with_overrides(
            &vertex_options.options,
            vertex_style_overrides,
            vertices.len(),
        )?;
        let styled_vertices = vertices
            .into_iter()
            .zip(vertex_styles)
            .map(|((id, position), options)| (id, position, options))
            .collect();

        let edges = pairs(edge_pairs, "Graph edges")?;
        let edge_styles =
            edge_options_with_overrides(&edge_options.options, edge_style_overrides, edges.len())?;
        let styled_edges = edges
            .into_iter()
            .zip(edge_styles)
            .map(|((start, end), options)| (start, end, options))
            .collect();

        self.mutate_live_graph(
            &mut graph.inner,
            GraphOperation::AddVerticesAndEdgesWithOptions {
                vertices: styled_vertices,
                edges: styled_edges,
            },
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = liveGraphCircularLayout)]
    pub fn live_graph_circular_layout(
        &mut self,
        graph: &mut WasmGraphHandle,
        scale: f64,
        scale_y: Option<f64>,
        center_x: f64,
        center_y: f64,
    ) -> Result<(), JsValue> {
        if !scale.is_finite()
            || scale_y.is_some_and(|value| !value.is_finite())
            || !center_x.is_finite()
            || !center_y.is_finite()
        {
            return Err(js_error("circular Graph layout values must be finite"));
        }
        self.mutate_live_graph(
            &mut graph.inner,
            GraphOperation::Circular {
                scale,
                scale_y,
                center: (center_x, center_y),
            },
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = liveGraphShellLayout)]
    pub fn live_graph_shell_layout(
        &mut self,
        graph: &mut WasmGraphHandle,
        shell_vertex_indices: &[u32],
        shell_sizes: &[u32],
        scale: f64,
        center_x: f64,
        center_y: f64,
        rotate: Option<f64>,
    ) -> Result<(), JsValue> {
        if !scale.is_finite()
            || !center_x.is_finite()
            || !center_y.is_finite()
            || rotate.is_some_and(|value| !value.is_finite())
        {
            return Err(js_error("shell Graph layout values must be finite"));
        }
        self.mutate_live_graph(
            &mut graph.inner,
            GraphOperation::Shell {
                shells: grouped_indices(shell_vertex_indices, shell_sizes, "Graph shell")?,
                scale,
                center: (center_x, center_y),
                rotate,
            },
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = liveGraphPartiteLayout)]
    pub fn live_graph_partite_layout(
        &mut self,
        graph: &mut WasmGraphHandle,
        partition_vertex_indices: &[u32],
        partition_sizes: &[u32],
        scale: f64,
        scale_y: Option<f64>,
        center_x: f64,
        center_y: f64,
        align_horizontal: bool,
    ) -> Result<(), JsValue> {
        if !scale.is_finite()
            || scale_y.is_some_and(|value| !value.is_finite())
            || !center_x.is_finite()
            || !center_y.is_finite()
        {
            return Err(js_error("partite Graph layout values must be finite"));
        }
        self.mutate_live_graph(
            &mut graph.inner,
            GraphOperation::Partite {
                partitions: grouped_indices(
                    partition_vertex_indices,
                    partition_sizes,
                    "Graph partition",
                )?,
                scale,
                scale_y,
                center: (center_x, center_y),
                align_horizontal,
            },
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = liveGraphSpiralLayout)]
    pub fn live_graph_spiral_layout(
        &mut self,
        graph: &mut WasmGraphHandle,
        scale: f64,
        scale_y: Option<f64>,
        center_x: f64,
        center_y: f64,
        resolution: f64,
        equidistant: bool,
    ) -> Result<(), JsValue> {
        if !scale.is_finite()
            || scale_y.is_some_and(|value| !value.is_finite())
            || !center_x.is_finite()
            || !center_y.is_finite()
        {
            return Err(js_error("spiral Graph layout values must be finite"));
        }
        self.mutate_live_graph(
            &mut graph.inner,
            GraphOperation::Spiral {
                scale,
                scale_y,
                center: (center_x, center_y),
                resolution,
                equidistant,
            },
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = liveGraphSpectralLayout)]
    pub fn live_graph_spectral_layout(
        &mut self,
        graph: &mut WasmGraphHandle,
        edge_weights: &[f64],
        scale: f64,
        scale_y: Option<f64>,
        center_x: f64,
        center_y: f64,
    ) -> Result<(), JsValue> {
        if !scale.is_finite()
            || scale_y.is_some_and(|value| !value.is_finite())
            || !center_x.is_finite()
            || !center_y.is_finite()
        {
            return Err(js_error("spectral Graph layout values must be finite"));
        }
        self.mutate_live_graph(
            &mut graph.inner,
            GraphOperation::Spectral(noon::GraphLayoutOptions {
                layout: noon::GraphLayout::Spectral,
                scale,
                scale_y,
                center: (center_x, center_y),
                edge_weights: Some(edge_weights.to_vec()),
                ..Default::default()
            }),
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = liveGraphKamadaKawaiLayout)]
    pub fn live_graph_kamada_kawai_layout(
        &mut self,
        graph: &mut WasmGraphHandle,
        edge_weights: &[f64],
        distances: &[f64],
        initial_positions: &[f64],
        scale: f64,
        scale_y: Option<f64>,
        center_x: f64,
        center_y: f64,
    ) -> Result<(), JsValue> {
        let position_ids = (0..initial_positions.len() / 2)
            .map(|index| index as u32)
            .collect::<Vec<_>>();
        let initial_positions = if initial_positions.is_empty() {
            None
        } else {
            Some(
                vertices(&position_ids, initial_positions)?
                    .into_iter()
                    .map(|(_, position)| position)
                    .collect(),
            )
        };
        let distances = (!distances.is_empty()).then(|| distances.to_vec());
        let directed = matches!(&graph.inner, NativeGraph::Directed(_));
        if !scale.is_finite()
            || scale_y.is_some_and(|value| !value.is_finite())
            || !center_x.is_finite()
            || !center_y.is_finite()
        {
            return Err(js_error("Kamada-Kawai Graph layout values must be finite"));
        }
        self.mutate_live_graph(
            &mut graph.inner,
            GraphOperation::KamadaKawai(noon::GraphLayoutOptions {
                layout: noon::GraphLayout::KamadaKawai,
                scale,
                scale_y,
                center: (center_x, center_y),
                directed,
                edge_weights: Some(edge_weights.to_vec()),
                kamada_kawai_distances: distances,
                kamada_kawai_positions: initial_positions,
                ..Default::default()
            }),
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = liveGraphSpringLayout)]
    pub fn live_graph_spring_layout(
        &mut self,
        graph: &mut WasmGraphHandle,
        edge_weights: &[f64],
        initial_positions: &[f64],
        fixed_vertex_indices: &[u32],
        fixed_is_set: bool,
        scale: f64,
        scale_y: Option<f64>,
        center_x: f64,
        center_y: f64,
        k: Option<f64>,
        iterations: u32,
        threshold: f64,
        rescale: bool,
    ) -> Result<(), JsValue> {
        let position_ids = (0..initial_positions.len() / 2)
            .map(|index| index as u32)
            .collect::<Vec<_>>();
        let initial_positions = vertices(&position_ids, initial_positions)?
            .into_iter()
            .map(|(_, position)| position)
            .collect();
        let spring_fixed = if fixed_is_set {
            Some(
                fixed_vertex_indices
                    .iter()
                    .map(|&index| index as usize)
                    .collect(),
            )
        } else if fixed_vertex_indices.is_empty() {
            None
        } else {
            return Err(js_error("spring fixed indices require fixed to be set"));
        };
        self.mutate_live_graph(
            &mut graph.inner,
            GraphOperation::Spring(noon::GraphLayoutOptions {
                layout: noon::GraphLayout::Spring,
                scale,
                scale_y,
                center: (center_x, center_y),
                iterations: iterations as usize,
                threshold,
                spring_k: k,
                edge_weights: Some(edge_weights.to_vec()),
                spring_positions: Some(initial_positions),
                spring_fixed,
                spring_rescale: rescale,
                ..Default::default()
            }),
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = liveGraphRandomLayout)]
    pub fn live_graph_random_layout(
        &mut self,
        graph: &mut WasmGraphHandle,
        random_positions: &[f64],
        scale: f64,
        scale_y: Option<f64>,
    ) -> Result<(), JsValue> {
        let position_ids = (0..random_positions.len() / 2)
            .map(|index| index as u32)
            .collect::<Vec<_>>();
        let random_positions = vertices(&position_ids, random_positions)?
            .into_iter()
            .map(|(_, position)| position)
            .collect();
        self.mutate_live_graph(
            &mut graph.inner,
            GraphOperation::Random(noon::GraphLayoutOptions {
                layout: noon::GraphLayout::Random,
                scale,
                scale_y,
                random_positions: Some(random_positions),
                ..Default::default()
            }),
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = liveGraphTreeLayout)]
    pub fn live_graph_tree_layout(
        &mut self,
        graph: &mut WasmGraphHandle,
        root_index: u32,
        orientation_up: bool,
        spacing_x: Option<f64>,
        spacing_y: Option<f64>,
        scale: f64,
        scale_y: Option<f64>,
    ) -> Result<(), JsValue> {
        let vertex_spacing = match (spacing_x, spacing_y) {
            (Some(x), Some(y)) => Some((x, y)),
            (None, None) => None,
            _ => return Err(js_error("tree vertex spacing requires both x and y values")),
        };
        self.mutate_live_graph(
            &mut graph.inner,
            GraphOperation::Tree(noon::GraphLayoutOptions {
                layout: noon::GraphLayout::Tree,
                scale,
                scale_y,
                tree_root: Some(root_index as usize),
                tree_orientation_up: orientation_up,
                tree_vertex_spacing: vertex_spacing,
                ..Default::default()
            }),
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
