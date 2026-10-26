//! ComplexPlane's matching retained grid, curve and marker scene.
use super::number_plane;
use crate::Scene;

pub fn scene() -> Result<Scene, Box<dyn std::error::Error>> {
    number_plane::scene_with_tips(false)
}

pub fn session() -> Result<crate::SceneExecution, String> {
    scene()
        .and_then(|scene| scene.into_execution().map_err(Into::into))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complex_plane_fixture_matches_the_tipless_python_example() {
        let scene = scene().unwrap();
        let store = scene.integration_store().borrow();
        let roots = store.semantic_family_members_checked(scene.root()).unwrap();
        let text_count = if cfg!(all(feature = "native-text", feature = "bundled-fonts")) {
            2
        } else {
            0
        };
        assert_eq!(roots.len(), 1 + text_count);
        let content = store.semantic_family_members_checked(roots[0]).unwrap();
        assert_eq!(content.len(), 3, "plane, curve and marker remain distinct");
        let plane = store.semantic_family_members_checked(content[0]).unwrap();
        assert_eq!(plane.len(), 4, "faded, major, x-axis and y-axis families");
        for axis in &plane[2..] {
            let members = store.semantic_family_members_checked(*axis).unwrap();
            assert_eq!(
                members.len(),
                2,
                "tipless axes retain shaft and tick groups"
            );
        }
        let geometry_count = store
            .ordered_leaf_nodes(scene.root())
            .unwrap()
            .iter()
            .filter(|&&node| {
                store
                    .semantic_object_state_checked(node)
                    .unwrap()
                    .content
                    .geometry()
                    .is_some()
            })
            .count();
        drop(store);
        assert_eq!(geometry_count, 22);
        assert_eq!(
            scene.execution_session().unwrap().frame().objects.len(),
            22 + text_count
        );
    }
}
