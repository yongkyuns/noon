//! Native counterpart of the Python sketch path demo.
use crate::{
    AnimationOptions, Create, LiveProgram, ManimGeometryOptions, RateFunction, Scene, SceneScript,
    ScriptError, Vec2, VectorPath, TEAL, YELLOW,
};

pub fn program() -> Result<LiveProgram<SceneScript>, ScriptError> {
    Scene::script(|scene| async move {
        let curve = VectorPath::new().move_to(Vec2::new(-3.7, -0.7)).cubic_to(
            Vec2::new(-3.4, 2.0),
            Vec2::new(-1.35, -2.0),
            Vec2::new(-0.65, 0.75),
        );
        let mut options = ManimGeometryOptions::path(curve)?;
        options.disable_fill();
        options.set_stroke_width(0.05)?;
        options.set_sketch_stroke(YELLOW, 1.3, 17)?;
        let curve = scene.geometry(options)?;

        let mut options = ManimGeometryOptions::path(VectorPath::new())?;
        options.set_stroke_width(0.04)?;
        options.set_sketch_stroke(TEAL, 1.0, 8)?;
        let mut ring = scene.geometry(options)?;
        for [start, rest @ ..] in [
            [(0.55, -1.2), (3.6, -1.2), (3.6, 1.2), (0.55, 1.2)],
            [(1.35, -0.53), (1.35, 0.53), (2.8, 0.53), (2.8, -0.53)],
        ] {
            ring.start_new_path_f64(start)?;
            for point in rest {
                ring.add_line_to_f64(point)?;
            }
            ring.close_path()?;
        }
        scene.set_fill(&ring, TEAL, 0.38)?;
        scene
            .play_with(
                (Create::new(&curve), Create::new(&ring)),
                AnimationOptions::new()
                    .run_time(2.4)
                    .rate_func(RateFunction::Linear),
            )
            .await?;
        scene.wait(0.6).await
    })
}

#[cfg(all(test, feature = "prepared-prefix"))]
#[test]
fn sketch_paths_keep_geometry_and_fill_when_scrubbed() {
    use crate::{GeometryRef, RustHostCallbackTable};

    let mut player = program().unwrap().into_playback().unwrap();
    let mut callbacks = RustHostCallbackTable::new();
    for _ in 0..8 {
        if player.end_time().is_some() {
            break;
        }
        player.prepare_until(&mut callbacks, 3.1).unwrap();
    }
    assert_eq!(player.end_time(), Some(3.0));

    player.advance_to(&mut callbacks, 1.2).unwrap();
    let frame = player.session().unwrap().frame();
    assert_eq!(frame.objects.len(), 2);
    for (index, object) in frame.objects.iter().enumerate() {
        assert_eq!(frame.reveal(index), 0.5);
        assert!(matches!(
            object.geometry(),
            Some(GeometryRef::VectorPath(_))
        ));
        assert!(object.style.sketch_stroke.is_some());
    }
    let rows = frame.objects.clone();
    player.advance_to(&mut callbacks, 2.7).unwrap();
    assert_eq!(player.session().unwrap().frame().reveal(1), 1.0);
    player.seek(0.6).unwrap();
    assert_eq!(player.session().unwrap().frame().reveal(0), 0.25);
    player.seek(1.2).unwrap();
    let frame = player.session().unwrap().frame();
    assert_eq!(frame.objects, rows);
    assert_eq!([frame.reveal(0), frame.reveal(1)], [0.5, 0.5]);
}
