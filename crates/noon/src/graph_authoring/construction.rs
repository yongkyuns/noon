//! Graph input planning and one Scene-owned publication. No graph runtime.

use super::{
    GraphAuthoringError, GraphEdge, GraphEdgeEntry, GraphEdgeMobject, GraphEndpoint, GraphOptions,
    GraphTopology, GraphVertexEntry, GraphVertexId, RetainedGraph,
};
use crate::{
    arrow_authoring::{resolve_staged_arrow, stage_prepared_arrow, PreparedArrow, StagedArrow},
    AuthoringError, ManimArrow, ManimArrowOptions, ManimGeometryOptions, Mobject, MobjectFamily,
    Scene,
};
use noon_core::{
    SemanticLocalNodeToken, SemanticMutationTransaction, SemanticNodeCreation, SemanticObjectState,
    SemanticTransactionGraphDeclaration, SemanticTransactionGraphEdgeBinding,
};
use std::{collections::HashMap, hash::Hash, rc::Rc};

struct PlannedVertex<K> {
    key: K,
    id: GraphVertexId,
    options: ManimGeometryOptions,
}

enum PlannedEdgeGeometry {
    Line(ManimGeometryOptions),
    Arrow(ManimArrowOptions),
}

struct PlannedEdge {
    edge: GraphEdge,
    geometry: PlannedEdgeGeometry,
}

enum PreparedEdgeGeometry {
    Line(SemanticObjectState),
    Arrow(PreparedArrow),
}

struct PreparedEdge {
    edge: GraphEdge,
    geometry: PreparedEdgeGeometry,
}

struct StagedVertex<K> {
    key: K,
    id: GraphVertexId,
    node: SemanticLocalNodeToken,
}

enum StagedEdgeGeometry {
    Line {
        family: SemanticLocalNodeToken,
        line: SemanticLocalNodeToken,
    },
    Arrow(StagedArrow),
}

struct StagedEdge {
    edge: GraphEdge,
    geometry: StagedEdgeGeometry,
}

pub(super) fn build_graph<K, V, E>(
    scene: &mut Scene,
    vertices: V,
    edges: E,
    directed: bool,
    options: GraphOptions,
) -> Result<RetainedGraph<K>, GraphAuthoringError>
where
    K: Clone + Eq + Hash,
    V: IntoIterator<Item = (K, (f64, f64))>,
    E: IntoIterator<Item = (K, K)>,
{
    // Validate the complete configuration independently of input cardinality.
    // These are inert requests: no semantic node or path is admitted here.
    vertex_options((0.0, 0.0), &options)?;
    line_options((0.0, 0.0), (1.0, 0.0), &options)?;
    arrow_options((0.0, 0.0), (1.0, 0.0), &options)?;

    let mut topology = GraphTopology::new();
    let mut vertex_lookup = HashMap::new();
    let mut planned_vertices = Vec::new();
    let mut positions = Vec::new();
    for (vertex_index, (key, position)) in vertices.into_iter().enumerate() {
        if vertex_lookup.contains_key(&key) {
            return Err(GraphAuthoringError::DuplicateVertexKey { vertex_index });
        }
        let id = topology.add_vertex();
        let options = vertex_options(position, &options)?;
        vertex_lookup.insert(key.clone(), planned_vertices.len());
        positions.push(position);
        planned_vertices.push(PlannedVertex { key, id, options });
    }

    let mut planned_edges = Vec::new();
    for (edge_index, (start_key, end_key)) in edges.into_iter().enumerate() {
        let start_index =
            *vertex_lookup
                .get(&start_key)
                .ok_or(GraphAuthoringError::UnknownEdgeEndpoint {
                    edge_index,
                    endpoint: GraphEndpoint::Start,
                })?;
        let end_index =
            *vertex_lookup
                .get(&end_key)
                .ok_or(GraphAuthoringError::UnknownEdgeEndpoint {
                    edge_index,
                    endpoint: GraphEndpoint::End,
                })?;
        let start = planned_vertices[start_index].id;
        let end = planned_vertices[end_index].id;
        if start == end {
            return Err(GraphAuthoringError::SelfEdgeUnsupported { edge_index });
        }
        let id = topology.add_edge(start, end, directed)?;
        let edge = topology.edge(id).expect("new graph edge is present");
        let geometry = if directed {
            PlannedEdgeGeometry::Arrow(arrow_options(
                positions[start_index],
                positions[end_index],
                &options,
            )?)
        } else {
            PlannedEdgeGeometry::Line(line_options(
                positions[start_index],
                positions[end_index],
                &options,
            )?)
        };
        planned_edges.push(PlannedEdge { edge, geometry });
    }

    let store_rc = Rc::clone(scene.integration_store());
    let (result, root, staged_vertices, staged_edges) =
        scene.with_semantic_publication(|store, publish| {
            let vertices = planned_vertices
                .into_iter()
                .map(|vertex| Ok((vertex.key, vertex.id, vertex.options.into_state(store)?)))
                .collect::<Result<Vec<_>, AuthoringError>>()?;
            let edges = planned_edges
                .into_iter()
                .map(|edge| {
                    let geometry = match edge.geometry {
                        PlannedEdgeGeometry::Line(options) => {
                            PreparedEdgeGeometry::Line(options.into_state(store)?)
                        }
                        PlannedEdgeGeometry::Arrow(options) => {
                            PreparedEdgeGeometry::Arrow(options.prepare(store)?)
                        }
                    };
                    Ok(PreparedEdge {
                        edge: edge.edge,
                        geometry,
                    })
                })
                .collect::<Result<Vec<_>, AuthoringError>>()?;

            let mut paths = Vec::new();
            for edge in &edges {
                if let PreparedEdgeGeometry::Arrow(arrow) = &edge.geometry {
                    paths.push(arrow.end_tip.clone());
                    if let Some(path) = &arrow.start_tip {
                        paths.push(path.clone());
                    }
                }
            }
            store.with_geometry_paths(paths, |store, handles| {
                let mut transaction = SemanticMutationTransaction::new();
                let root = transaction.create_node(SemanticNodeCreation::family());
                let mut staged_edges = Vec::with_capacity(edges.len());
                let mut handle_index = 0;
                // Equal-priority vertex disks paint above their connecting leaves.
                for edge in edges {
                    let geometry = match edge.geometry {
                        PreparedEdgeGeometry::Line(state) => {
                            let line = transaction.create_node(SemanticNodeCreation::object(state));
                            let family = transaction.create_node(SemanticNodeCreation::family());
                            transaction
                                .add_member(family, line)
                                .add_member(root, family);
                            StagedEdgeGeometry::Line { family, line }
                        }
                        PreparedEdgeGeometry::Arrow(arrow) => {
                            let end_tip = handles[handle_index];
                            handle_index += 1;
                            let start_tip = if arrow.start_tip.is_some() {
                                let handle = handles[handle_index];
                                handle_index += 1;
                                Some(handle)
                            } else {
                                None
                            };
                            let staged =
                                stage_prepared_arrow(&mut transaction, &arrow, end_tip, start_tip);
                            transaction.add_member(root, staged.family);
                            StagedEdgeGeometry::Arrow(staged)
                        }
                    };
                    staged_edges.push(StagedEdge {
                        edge: edge.edge,
                        geometry,
                    });
                }
                debug_assert_eq!(handle_index, handles.len());

                let mut staged_vertices = Vec::with_capacity(vertices.len());
                for (key, id, state) in vertices {
                    let node = transaction.create_node(SemanticNodeCreation::object(state));
                    transaction.add_member(root, node);
                    staged_vertices.push(StagedVertex { key, id, node });
                }
                let graph_vertices = staged_vertices
                    .iter()
                    .map(|vertex| (vertex.id, vertex.node));
                let graph_edges = staged_edges.iter().map(|edge| {
                    let (family, line) = match &edge.geometry {
                        StagedEdgeGeometry::Line { family, line } => (*family, *line),
                        StagedEdgeGeometry::Arrow(arrow) => (arrow.family, arrow.shaft),
                    };
                    SemanticTransactionGraphEdgeBinding::new(
                        edge.edge.id,
                        family.into(),
                        line.into(),
                    )
                });
                transaction.set_graph_declaration(
                    root,
                    SemanticTransactionGraphDeclaration::new(topology, graph_vertices, graph_edges),
                );
                let result = publish(store, transaction)?;
                Ok::<_, AuthoringError>((result, root, staged_vertices, staged_edges))
            })
        })?;

    // Publication has succeeded. All remaining resolutions are invariants of
    // the exact transaction above, not fallible user input. In particular, a
    // post-commit error must not trigger removal of committed Arrow resources.
    let resolve = |token| {
        result
            .resolve(token)
            .expect("created graph token resolves after commit")
    };
    let family = MobjectFamily::from_node(Rc::clone(&store_rc), resolve(root))
        .expect("committed graph root is a family");
    let vertices = staged_vertices
        .into_iter()
        .map(|vertex| GraphVertexEntry {
            key: vertex.key,
            id: vertex.id,
            object: Mobject::from_node(Rc::clone(&store_rc), resolve(vertex.node))
                .expect("committed graph vertex is an object"),
        })
        .collect();
    let edges = staged_edges
        .into_iter()
        .map(|edge| {
            let object = match edge.geometry {
                StagedEdgeGeometry::Line { family, line } => GraphEdgeMobject::Line {
                    family: MobjectFamily::from_node(Rc::clone(&store_rc), resolve(family))
                        .expect("committed edge root is a family"),
                    line: Mobject::from_node(Rc::clone(&store_rc), resolve(line))
                        .expect("committed edge Line is an object"),
                },
                StagedEdgeGeometry::Arrow(arrow) => {
                    let committed = resolve_staged_arrow(arrow, |token| result.resolve(token))
                        .expect("committed Arrow component tokens resolve");
                    GraphEdgeMobject::Arrow(
                        ManimArrow::from_committed(Rc::clone(&store_rc), committed)
                            .expect("committed graph Arrow is structurally valid"),
                    )
                }
            };
            GraphEdgeEntry {
                edge: edge.edge,
                object,
            }
        })
        .collect::<Vec<_>>();
    let edge_lookup = edges
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.edge.id, index))
        .collect();
    Ok(RetainedGraph {
        directed,
        family,
        vertices,
        vertex_lookup,
        edges,
        edge_lookup,
    })
}

fn vertex_options(
    position: (f64, f64),
    options: &GraphOptions,
) -> Result<ManimGeometryOptions, AuthoringError> {
    let mut vertex = ManimGeometryOptions::circle(options.vertex_radius)?;
    vertex.set_translation(position.0, position.1)?;
    let fill = options.vertex_fill;
    vertex.set_fill(
        f64::from(fill.red),
        f64::from(fill.green),
        f64::from(fill.blue),
        f64::from(fill.alpha),
    )?;
    let stroke = options.vertex_stroke;
    vertex.set_stroke(
        f64::from(stroke.red),
        f64::from(stroke.green),
        f64::from(stroke.blue),
        f64::from(stroke.alpha),
    )?;
    vertex.set_stroke_width(options.vertex_stroke_width)?;
    Ok(vertex)
}

fn line_options(
    start: (f64, f64),
    end: (f64, f64),
    options: &GraphOptions,
) -> Result<ManimGeometryOptions, AuthoringError> {
    let mut line = ManimGeometryOptions::line(start.0, start.1, end.0, end.1)?;
    line.disable_fill();
    let color = options.edge_color;
    line.set_stroke(
        f64::from(color.red),
        f64::from(color.green),
        f64::from(color.blue),
        f64::from(color.alpha),
    )?;
    line.set_stroke_width(options.edge_stroke_width)?;
    Ok(line)
}

fn arrow_options(
    start: (f64, f64),
    end: (f64, f64),
    options: &GraphOptions,
) -> Result<ManimArrowOptions, AuthoringError> {
    let mut arrow = ManimArrowOptions::arrow(start.0, start.1, end.0, end.1)?;
    arrow.set_buff(options.directed_edge_buff.unwrap_or(options.vertex_radius))?;
    let color = options.edge_color;
    arrow.set_color(
        f64::from(color.red),
        f64::from(color.green),
        f64::from(color.blue),
        f64::from(color.alpha),
    )?;
    arrow.set_stroke_width(options.edge_stroke_width)?;
    Ok(arrow)
}
