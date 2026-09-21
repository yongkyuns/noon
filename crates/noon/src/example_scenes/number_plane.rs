//! One retained coordinate grid, curve and marker, paired with number_plane.py.
use crate::{ExecutionSession, ManimNumberPlaneOptions, Scene, SemanticPaint, BLUE, GREEN, YELLOW};

#[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
use crate::Text;

pub fn scene() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let mut options = ManimNumberPlaneOptions {
        x_range: [-3.0, 3.0, 1.0],
        y_range: [-2.0, 2.0, 1.0],
        x_length: Some(8.0),
        y_length: Some(4.5),
        faded_line_ratio: 2,
        ..Default::default()
    };
    options.background_line_style.stroke = Some(SemanticPaint::Solid(GREEN));
    options.background_line_style.stroke_width = 0.01;
    let plane = scene.number_plane(&options)?;
    let frame = plane.authored_frame()?;
    let sampling = crate::PlotSamplingOptions::axes(frame.x().range(), Some(&[-3.0, 3.0, 0.05]))?;
    let mut curve = scene.geometry(crate::ManimGeometryOptions::axes_function_plot(
        frame,
        &sampling,
        |x| 0.35 * x * x - 1.0,
        true,
    )?)?;
    curve.set_color(BLUE.red.into(), BLUE.green.into(), BLUE.blue.into(), 1.0)?;
    let point = frame.coords_to_point(1.0, -0.65)?;
    let roundtrip = frame.point_to_coords(point)?;
    assert!((roundtrip[0] - 1.0).abs() < 1e-6 && (roundtrip[1] + 0.65).abs() < 1e-6);
    let mut marker = scene.geometry(crate::ManimGeometryOptions::dot(point[0], point[1], 0.08)?)?;
    marker.set_color(
        YELLOW.red.into(),
        YELLOW.green.into(),
        YELLOW.blue.into(),
        1.0,
    )?;
    let content = scene.family(&[plane.family().into(), (&curve).into(), (&marker).into()])?;
    content.scale(0.85, 0.85)?;
    content.shift(0.0, -0.1)?;
    #[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
    {
        let mut title =
            scene.text(Text::new("NumberPlane: a shared coordinate grid").with_font_size(28.0))?;
        title.shift(0.0, 3.0)?;
        let mut caption = scene.text(
            Text::new("Major lines + subdivisions | y = 0.35 x² - 1 | point (1, -0.65)")
                .with_font_size(20.0),
        )?;
        caption.shift(0.0, -3.0)?;
        scene.add_many(&[(&content).into(), (&title).into(), (&caption).into()])?;
    }
    #[cfg(not(all(feature = "native-text", feature = "bundled-fonts")))]
    scene.add_many(&[(&content).into()])?;
    Ok(scene)
}

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let scene = scene()?;
        Ok(scene.execution_session()?)
    };
    build().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_demo_is_static_and_seek_does_not_rebuild_resources() {
        let scene = scene().unwrap();
        let revision = scene.revision();
        let (leaves, resources) = {
            let store = scene.integration_store().borrow();
            let roots = store.semantic_family_members_checked(scene.root()).unwrap();
            let text_count = if cfg!(all(feature = "native-text", feature = "bundled-fonts")) { 2 } else { 0 };
            assert_eq!(roots.len(), 1 + text_count);
            let content = store.semantic_family_members_checked(roots[0]).unwrap();
            assert_eq!(content.len(), 3, "plane, curve and marker remain distinct children");
            let plane = store.semantic_family_members_checked(content[0]).unwrap();
            assert_eq!(plane.len(), 4, "faded, major, x-axis and y-axis families");
            // Eleven vertical and seven horizontal half-unit lines; six are
            // major and twelve faded, including the two lines at zero.
            assert_eq!(store.semantic_family_members_checked(plane[0]).unwrap().len(), 12);
            assert_eq!(store.semantic_family_members_checked(plane[1]).unwrap().len(), 6);
            for axis in &plane[2..] {
                let members = store.semantic_family_members_checked(*axis).unwrap();
                assert_eq!(members.len(), 2, "shaft and empty tick family");
                assert!(store.semantic_family_members_checked(members[1]).unwrap().is_empty());
            }
            let leaves = store.ordered_leaf_nodes(scene.root()).unwrap();
            let geometry_count = leaves.iter().filter(|&&node| {
                store.semantic_object_state_checked(node).unwrap().content.geometry().is_some()
            }).count();
            assert_eq!(geometry_count, 22, "18 grid lines, two axes, curve and marker");
            assert_eq!(leaves.len(), geometry_count + text_count);
            (leaves, store.geometry_resources().stats())
        };
        let mut session = scene.execution_session().unwrap();
        assert!(!session.has_required_callbacks());
        // Check renderer projection stability separately from the exact semantic
        // member contract: retained text may have additional projection rows.
        let projected_count = session.frame().objects.len();
        assert!(projected_count > 0);
        session.take_renderer_publication();
        for time in [0.25, 0.5, 0.75, 1.0, 0.0] {
            session.seek(time).unwrap();
            assert_eq!(session.frame().objects.len(), projected_count);
            assert!(!session.has_required_callbacks());
            assert_eq!(scene.revision(), revision);
            let store = scene.integration_store().borrow();
            assert_eq!(store.ordered_leaf_nodes(scene.root()).unwrap(), leaves);
            assert_eq!(store.geometry_resources().stats(), resources);
        }
    }
}
