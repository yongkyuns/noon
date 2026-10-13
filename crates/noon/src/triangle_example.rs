//! Triangle defaults and regular-polygon options through shared Rust geometry.
use crate::{LiveProgram, ManimGeometryOptions, Scene};

pub fn scene() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let requests = [
        (ManimGeometryOptions::triangle()?, -3.5),
        (
            ManimGeometryOptions::regular_polygon(3, 1.5, Some(0.3))?,
            0.0,
        ),
        (ManimGeometryOptions::regular_polygon(3, 0.7, None)?, 3.5),
    ];
    for (mut options, x) in requests {
        options.set_translation(x, 0.0)?;
        options.set_fill_opacity(0.2)?;
        let object = scene.geometry(options)?;
        assert_eq!(object.path_query()?.start_anchors().len(), 3);
        assert_eq!(object.path_query()?.vertex_groups().len(), 1);
        scene.add(&object)?;
    }
    Ok(scene)
}

pub fn program(
) -> Result<LiveProgram<crate::example_scenes::static_hold::Hold>, Box<dyn std::error::Error>> {
    Ok(crate::example_scenes::static_hold::program(scene()?, 0.2)?)
}

#[cfg(test)]
mod tests {
    use crate::{ManimGeometryOptions, Scene};

    #[test]
    fn triangle_defaults_share_regular_polygon_closure_and_queries() {
        let mut scene = Scene::new();
        let triangle = scene
            .geometry(ManimGeometryOptions::triangle().unwrap())
            .unwrap();
        let regular = scene
            .geometry(ManimGeometryOptions::regular_polygon(3, 1.0, None).unwrap())
            .unwrap();
        let a = triangle.path_query().unwrap();
        let b = regular.path_query().unwrap();
        assert_eq!(a.start_anchors(), b.start_anchors());
        assert_eq!(a.end_anchors(), b.end_anchors());
        assert_eq!(a.vertex_groups().len(), 1);
        assert_eq!(a.start_anchors().len(), 3);
        assert_eq!(a.start_anchors()[0], *a.end_anchors().last().unwrap());
        assert_eq!(
            super::scene()
                .unwrap()
                .execution_session()
                .unwrap()
                .frame()
                .objects
                .len(),
            3
        );
    }
}
