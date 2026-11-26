//! Native counterpart of the Python beveled cube crease demo.
use super::cube_sketch_wireframe::{orbit, CubeOrbit};
use crate::{Color, LiveProgram, ScriptError, SemanticVec3, SketchStroke};

const VERTICES: [[f64; 3]; 12] = [
    [-1.0, -1.0, -1.0],
    [1.0, -1.0, -1.0],
    [1.0, 1.0, -1.0],
    [-1.0, 1.0, -1.0],
    [-1.0, -1.0, 0.65],
    [1.0, -1.0, 0.65],
    [1.0, 1.0, 0.65],
    [-1.0, 1.0, 0.65],
    [-0.65, -0.65, 1.0],
    [0.65, -0.65, 1.0],
    [0.65, 0.65, 1.0],
    [-0.65, 0.65, 1.0],
];
const FACES: [[u32; 3]; 20] = [
    [0, 3, 2],
    [0, 2, 1],
    [0, 1, 5],
    [0, 5, 4],
    [1, 2, 6],
    [1, 6, 5],
    [2, 3, 7],
    [2, 7, 6],
    [3, 0, 4],
    [3, 4, 7],
    [4, 5, 9],
    [4, 9, 8],
    [5, 6, 10],
    [5, 10, 9],
    [6, 7, 11],
    [6, 11, 10],
    [7, 4, 8],
    [7, 8, 11],
    [8, 9, 10],
    [8, 10, 11],
];

pub fn program() -> Result<LiveProgram<CubeOrbit>, ScriptError> {
    beveled_cube(false)
}

pub fn silhouette_program() -> Result<LiveProgram<CubeOrbit>, ScriptError> {
    beveled_cube(true)
}

fn beveled_cube(silhouette: bool) -> Result<LiveProgram<CubeOrbit>, ScriptError> {
    let create = if silhouette {
        crate::Scene::polyhedron_sketch_silhouette
    } else {
        crate::Scene::polyhedron_sketch_creases
    };
    orbit(|scene| {
        create(
            scene,
            &VERTICES.map(|[x, y, z]| SemanticVec3::new(x, y, z)),
            &FACES,
            Color::from_hex(0x236B8E),
            Color::from_hex(0xF7D996),
            0.05,
            SketchStroke::new(1.1, 27).expect("valid crease sketch"),
        )
    })
}

#[cfg(all(test, feature = "prepared-prefix"))]
#[test]
fn mesh_sketch_creases_and_silhouette_keep_geometry_when_scrubbed() {
    for (demo, silhouette) in [(program(), false), (silhouette_program(), true)] {
        let mut player = demo.unwrap().into_playback().unwrap();
        let mut callbacks = crate::RustHostCallbackTable::new();
        while player.prepare_until(&mut callbacks, 3.1).unwrap() {}
        assert_eq!(player.end_time(), Some(3.0));

        let start = player.session().unwrap().frame().clone();
        assert_eq!(
            start
                .objects
                .iter()
                .filter(|object| object.style.sketch_stroke.is_some())
                .count(),
            20
        );
        assert_eq!(
            start
                .objects
                .iter()
                .filter(|object| {
                    object.spatial.as_deref().is_some_and(|spatial| {
                        matches!(
                            spatial.material,
                            noon_core::SemanticSpatialMaterial::SketchSilhouette(_)
                        )
                    })
                })
                .count(),
            if silhouette { 20 } else { 0 }
        );
        player.advance_to(&mut callbacks, 1.3).unwrap();
        let mid = player.session().unwrap().frame().clone();
        assert_ne!(mid, start);
        assert!(mid
            .objects
            .iter()
            .zip(&start.objects)
            .all(|(current, original)| {
                original.style.sketch_stroke.is_none() || current == original
            }));
        player.advance_to(&mut callbacks, 2.75).unwrap();
        assert_ne!(player.session().unwrap().frame(), &mid);
        player.seek(1.3).unwrap();
        assert_eq!(player.session().unwrap().frame(), &mid);
        player.seek(0.0).unwrap();
        assert_eq!(player.session().unwrap().frame(), &start);
    }
}
