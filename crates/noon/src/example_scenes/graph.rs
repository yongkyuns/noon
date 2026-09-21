//! Explicit-layout Graph/DiGraph proof over ordinary retained semantics.

use crate::{ExecutionSession, MobjectTarget, Scene};

pub fn scene() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();

    let graph = scene.graph(
        [
            ("a", (-5.0, -1.0)),
            ("b", (-3.5, 1.2)),
            ("c", (-2.0, -1.0)),
        ],
        [("a", "b"), ("b", "c"), ("c", "a")],
    )?;

    let digraph = scene.digraph(
        [
            ("u", (2.0, -1.0)),
            ("v", (3.5, 1.2)),
            ("w", (5.0, -1.0)),
        ],
        [("u", "v"), ("v", "w"), ("w", "u")],
    )?;

    scene.add_many(&[
        MobjectTarget::Family(graph.family()),
        MobjectTarget::Family(digraph.family()),
    ])?;
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
        // Undirected: three Lines + three vertices.
        // Directed: three Arrow shafts + three tips + three vertices.
        assert_eq!(leaves.len(), 15);
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
