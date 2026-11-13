//! Shared Graph and DiGraph dispatch for hosts with numeric vertex identities.

pub enum HostGraph {
    Undirected(crate::Graph<u32>),
    Directed(crate::DiGraph<u32>),
}

pub enum GraphOperation {
    AddVertices(Vec<(u32, (f64, f64))>),
    AddEdges(Vec<(u32, u32)>),
    RemoveVertices(Vec<u32>),
    RemoveEdges(Vec<(u32, u32)>),
    Circular { scale: f64, center: (f64, f64) },
    Explicit(Vec<(f64, f64)>),
}

impl HostGraph {
    pub fn new(
        scene: &mut crate::Scene,
        directed: bool,
        vertices: Vec<(u32, (f64, f64))>,
        edges: Vec<(u32, u32)>,
        options: crate::GraphOptions,
    ) -> Result<Self, crate::GraphAuthoringError> {
        if directed {
            crate::DiGraph::with_options(scene, vertices, edges, options).map(Self::Directed)
        } else {
            crate::Graph::with_options(scene, vertices, edges, options).map(Self::Undirected)
        }
    }

    pub fn family(&self) -> &crate::MobjectFamily {
        match self {
            Self::Undirected(graph) => graph.family(),
            Self::Directed(graph) => graph.family(),
        }
    }

    pub fn vertex(&self, key: u32) -> Option<&crate::Mobject> {
        match self {
            Self::Undirected(graph) => graph.vertex(&key),
            Self::Directed(graph) => graph.vertex(&key),
        }
    }

    pub fn apply(
        &mut self,
        scene: &mut crate::Scene,
        operation: GraphOperation,
    ) -> Result<(), crate::GraphAuthoringError> {
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
                    crate::GraphLayoutOptions {
                        layout: crate::GraphLayout::Circular,
                        scale,
                        center,
                        ..Default::default()
                    },
                )?;
            }
            (Self::Directed(graph), GraphOperation::Circular { scale, center }) => {
                graph.change_layout(
                    scene,
                    crate::GraphLayoutOptions {
                        layout: crate::GraphLayout::Circular,
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

    pub fn copy_in_scene(
        &self,
        scene: &mut crate::Scene,
    ) -> Result<Self, crate::GraphAuthoringError> {
        match self {
            Self::Undirected(graph) => graph.copy_in_scene(scene).map(Self::Undirected),
            Self::Directed(graph) => graph.copy_in_scene(scene).map(Self::Directed),
        }
    }
}
pub fn pairs(
    values: &[u32],
    name: &str,
) -> Result<Vec<(u32, u32)>, crate::integration::AuthoringFailure> {
    let chunks = values.chunks_exact(2);
    if !chunks.remainder().is_empty() {
        return Err(format!("{name} requires pairs of vertex IDs").into());
    }
    Ok(chunks.map(|pair| (pair[0], pair[1])).collect())
}

pub fn vertices(
    ids: &[u32],
    positions: &[f64],
) -> Result<Vec<(u32, (f64, f64))>, crate::integration::AuthoringFailure> {
    if positions.len() != ids.len() * 2 {
        return Err("Graph positions require exactly two finite values per vertex".into());
    }
    ids.iter()
        .zip(positions.chunks_exact(2))
        .map(|(&id, position)| {
            let (x, y) = (position[0], position[1]);
            if !x.is_finite() || !y.is_finite() {
                return Err("Graph positions must be finite".into());
            }
            Ok((id, (x, y)))
        })
        .collect()
}

pub fn manim_stroke_width(
    name: &str,
    width: f64,
) -> Result<f64, crate::integration::AuthoringFailure> {
    if !width.is_finite() || width < 0.0 {
        return Err(format!("{name} must be a finite non-negative number").into());
    }
    Ok(width * crate::integration::MANIM_CAIRO_LINE_WIDTH_MULTIPLE)
}
