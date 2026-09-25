//! Explicit-layout Graph/DiGraph proof over ordinary retained semantics.

use crate::{ExecutionSession, GraphOptions, MobjectTarget, Scene, BLUE, GREEN, RED};

pub fn scene() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();

    let options = GraphOptions {
        vertex_radius: 0.21,
        vertex_fill: RED,
        vertex_fill_opacity: 0.35,
        vertex_stroke: BLUE,
        vertex_stroke_width: 0.03,
        edge_color: GREEN,
        edge_stroke_width: 0.06,
        ..GraphOptions::default()
    };
    let mut graph = scene.graph_with_options(
        [("a", (-5.0, -1.0)), ("b", (-3.5, 1.2)), ("c", (-2.0, -1.0))],
        [("a", "b"), ("b", "c"), ("c", "a")],
        options.clone(),
    )?;

    graph.change_layout_positions(&mut scene, &[(-5.0, -1.0), (-3.5, 1.2), (-2.0, -1.0)])?;
    graph.add_vertices(&mut scene, [("d", (-3.5, -2.4))])?;
    graph.add_edges(&mut scene, [("a", "d")])?;

    let mut digraph = scene.digraph_with_options(
        [("u", (2.0, -1.0)), ("v", (3.5, 1.2)), ("w", (5.0, -1.0))],
        [("u", "v"), ("v", "w"), ("w", "u")],
        options,
    )?;

    digraph.change_layout(
        &mut scene,
        crate::GraphLayoutOptions {
            layout: crate::GraphLayout::Circular,
            scale: 1.6,
            center: (3.5, 0.0),
            ..Default::default()
        },
    )?;
    digraph.add_vertices(&mut scene, [("x", (5.0, -2.3))])?;
    digraph.add_edges(&mut scene, [("w", "x")])?;

    scene.add_many(&[
        MobjectTarget::Family(graph.family()),
        MobjectTarget::Family(digraph.family()),
    ])?;
    scene.wait(0.21)?;
    Ok(scene)
}

pub fn session() -> Result<ExecutionSession, String> {
    let scene = scene().map_err(|error| error.to_string())?;
    scene.execution_session().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_graph_gallery_uses_only_ordinary_retained_leaves() {
        let scene = scene().unwrap();
        let store = scene.integration_store().borrow();
        let leaves = store.ordered_leaf_nodes(scene.root()).unwrap();
        // Persistent additions keep the same ordinary retained leaves.
        // Undirected: four Lines + four vertices.
        // Directed: four Arrow shafts + four tips + four vertices.
        assert_eq!(leaves.len(), 20);
        for leaf in leaves {
            assert!(store
                .semantic_object_state_checked(leaf)
                .unwrap()
                .content
                .geometry()
                .is_some());
        }
    }
}
