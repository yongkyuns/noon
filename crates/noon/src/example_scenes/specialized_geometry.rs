//! Shared specialized geometry construction without a language or transport bridge.

use std::{f64::consts::PI, rc::Rc};

use crate::{ExecutionSession, Mobject, MobjectFamilyMember, Scene};

/// A grid of nine geometry constructors, paired with `specialized_geometry.py`.
pub fn session() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let store = || Rc::clone(scene.integration_store());
    let dot = Mobject::manim_dot(store(), -4.0, 2.0, 0.3)?;
    let mut triangle = Mobject::manim_triangle(store())?;
    triangle.shift(0.0, 2.0)?;
    let mut elbow = Mobject::manim_elbow(store(), 0.8, 0.3)?;
    elbow.shift(4.0, 2.0)?;
    let mut rectangle = Mobject::manim_rounded_rectangle(store(), 2.0, 1.0, 0.2)?;
    rectangle.shift(-4.0, 0.0)?;
    let annular_sector = Mobject::manim_annular_sector(store(), 0.3, 0.9, PI, 0.0, 8, 0.0, 0.0)?;
    let sector = Mobject::manim_sector(store(), 0.9, PI / 2.0, PI / 4.0, 8, 4.0, 0.0)?;
    let annulus = Mobject::manim_annulus(store(), 0.5, 0.9, 8, -4.0, -2.0)?;
    let dashed = Mobject::manim_dashed_line(store(), -1.0, -2.0, 1.0, -2.0, 0.2, 0.5)?;
    let mut underline = Mobject::manim_underline(&rectangle, 0.2)?;
    underline.shift(8.0, -1.3)?;

    let mut objects = [
        dot,
        triangle,
        elbow,
        rectangle,
        annular_sector,
        sector,
        annulus,
        dashed,
        underline,
    ];
    for object in &mut objects {
        object.set_fill(0.0, 0.0, 1.0, 0.35)?;
        object.set_stroke_color(1.0, 1.0, 1.0, 1.0)?;
        object.set_stroke_opacity(1.0)?;
        // Manim width 2 uses a 0.02 scene-unit stroke in the paired Python example.
        object.set_stroke_width(0.02)?;
    }
    scene
        .add_many(
            &objects
                .iter()
                .map(MobjectFamilyMember::Mobject)
                .collect::<Vec<_>>(),
        )
        .map_err(|error| error.to_string())?;
    scene.execution_session().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn specialized_geometry_reaches_the_shared_native_runtime() {
        let mut session = super::session().unwrap();
        assert_eq!(session.frame().objects.len(), 9);
        assert!(session
            .frame()
            .objects
            .iter()
            .all(|object| object.geometry().is_some()));
        session.take_frame_changes();
        session.advance_to(0.0).unwrap();
        assert!(session.take_frame_changes().object_indices().is_empty());
    }
}
