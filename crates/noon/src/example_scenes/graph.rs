//! Explicit-layout Graph/DiGraph proof over ordinary retained semantics.

use crate::{
    AnimationOptions, ContinuationScene, ContinuationStep, GraphOptions, LiveContinuation,
    LiveProgram, MobjectFamily, RateFunction, Scene, ScriptError, Vec2, BLUE, GREEN, RED,
};

pub struct GraphMotion {
    diagrams: MobjectFamily,
    shifted: bool,
}

impl LiveContinuation for GraphMotion {
    const EFFECTS: crate::ContinuationEffects = crate::ContinuationEffects::SceneOnly;
    type Error = ScriptError;

    fn resume(&mut self, scene: &mut ContinuationScene) -> Result<ContinuationStep, ScriptError> {
        if self.shifted {
            return Ok(ContinuationStep::Finished);
        }
        let segment = crate::animation_request::activate(
            &mut scene.live(),
            self.diagrams.animate().shift(Vec2::new(0.0, 0.75)).into(),
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )?;
        self.shifted = true;
        Ok(ContinuationStep::Await(segment))
    }
}

fn scene() -> Result<(Scene, MobjectFamily), Box<dyn std::error::Error>> {
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

    let diagrams = scene.family(&[graph.family().into(), digraph.family().into()])?;
    scene.add_many(&[(&diagrams).into()])?;
    Ok((scene, diagrams))
}

pub fn program() -> Result<LiveProgram<GraphMotion>, String> {
    let (scene, diagrams) = scene().map_err(|error| error.to_string())?;
    scene
        .into_live_program(GraphMotion {
            diagrams,
            shifted: false,
        })
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_graph_gallery_uses_only_ordinary_retained_leaves() {
        let (scene, _) = scene().unwrap();
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

    #[cfg(feature = "prepared-prefix")]
    #[test]
    fn graph_shift_moves_edges_and_vertices_together_and_rewinds() {
        let mut player = program().unwrap().into_playback().unwrap();
        let mut callbacks = crate::RustHostCallbackTable::new();
        let snapshot = |player: &crate::PlaybackProgram<GraphMotion>| {
            let session = player.session().unwrap();
            let frame = session.frame();
            session
                .painter_order()
                .iter()
                .map(|&index| {
                    let index = index as usize;
                    (
                        frame.objects[index].id,
                        frame.render_transform(index).translation,
                    )
                })
                .collect::<Vec<_>>()
        };
        player.prepare_ready(&mut callbacks).unwrap();
        player.advance_to(&mut callbacks, 0.25).unwrap();
        let first = snapshot(&player);
        player.prepare_ready(&mut callbacks).unwrap();
        player.advance_to(&mut callbacks, 0.75).unwrap();
        let second = snapshot(&player);
        assert_eq!(first.len(), 20);
        assert_eq!(second.len(), 20);
        for ((a, first), (b, second)) in first.iter().zip(&second) {
            assert_eq!(a, b);
            let delta = *second - *first - Vec2::new(0.0, 0.375);
            assert!(delta.x.abs() < 1e-5 && delta.y.abs() < 1e-5);
        }
        player.seek(0.25).unwrap();
        assert_eq!(snapshot(&player), first);
    }
}
