//! Explicit-layout Graph/DiGraph proof over ordinary retained semantics.

use crate::{
    ExecutionSession, GraphOptions, Mobject, MobjectFamily, MobjectTarget, Scene, StyleUpdate,
    BLUE, GREEN, PURPLE, RED, YELLOW,
};
#[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
use crate::{Text, BLACK};

fn graph_scene_family(
    scene: &mut Scene,
    graph: &MobjectFamily,
    label: Option<&Mobject>,
) -> Result<MobjectFamily, crate::AuthoringError> {
    let mut members = vec![MobjectTarget::Family(graph)];
    if let Some(label) = label {
        members.push(MobjectTarget::Object(label));
    }
    scene.family(&members)
}

pub fn scene() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();

    let options = GraphOptions {
        vertex_radius: 0.21,
        vertex_fill: RED,
        vertex_fill_opacity: 0.35,
        vertex_stroke: BLUE,
        vertex_stroke_width: 0.03,
        vertex_stroke_opacity: 0.45,
        edge_color: GREEN,
        edge_stroke_width: 0.06,
        edge_stroke_opacity: 0.55,
        ..GraphOptions::default()
    };
    let mut graph_options = options.clone();
    graph_options.vertex_radius_overrides.insert(1, 0.28);
    let mut graph = scene.graph_with_options(
        [("a", (-5.0, -1.0)), ("b", (-3.5, 1.2)), ("c", (-2.0, -1.0))],
        [("a", "b"), ("b", "c"), ("c", "a")],
        graph_options,
    )?;

    graph.change_layout_positions(&mut scene, &[(-5.0, -1.0), (-3.5, 1.2), (-2.0, -1.0)])?;
    #[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
    let graph_label = {
        let mut label = scene.text(Text::new("a").with_font_size(48.0).color(BLACK))?;
        label.shift(-5.0, -1.0)?;
        label
    };
    let mut added_vertex_options = options.clone();
    added_vertex_options.vertex_radius = 0.16;
    added_vertex_options.vertex_fill = YELLOW;
    added_vertex_options.vertex_stroke_width = 0.05;
    added_vertex_options.vertex_stroke_opacity = 0.2;
    let mut added_edge_options = options.clone();
    added_edge_options.edge_color = PURPLE;
    added_edge_options.edge_stroke_width = 0.03;
    added_edge_options.edge_stroke_opacity = 0.3;
    graph.add_vertices_and_edges_with_options(
        &mut scene,
        [("d", (-3.5, -2.4), added_vertex_options)],
        [("a", "d", added_edge_options)],
    )?;
    let mut standalone_vertex_options = options.clone();
    standalone_vertex_options.vertex_radius = 0.16;
    standalone_vertex_options.vertex_fill = YELLOW;
    standalone_vertex_options.vertex_stroke_width = 0.05;
    standalone_vertex_options.vertex_stroke_opacity = 0.2;
    graph
        .add_vertices_with_options(&mut scene, [("e", (-5.5, -2.4), standalone_vertex_options)])?;
    graph.vertex(&"b").unwrap().clone().set_style(StyleUpdate {
        fill_color: Some(YELLOW),
        ..Default::default()
    })?;
    graph
        .edge(&"a", &"b")
        .unwrap()
        .family()
        .set_style(StyleUpdate {
            fill_color: Some(PURPLE),
            stroke_color: Some(PURPLE),
            stroke_opacity: Some(0.3),
            ..Default::default()
        })?;
    graph.vertex(&"b").unwrap().clone().set_style(StyleUpdate {
        stroke_opacity: Some(0.2),
        ..Default::default()
    })?;

    let mut digraph_options = options.clone();
    digraph_options.vertex_radius_overrides.insert(1, 0.28);
    let mut digraph = scene.digraph_with_options(
        [("u", (2.0, -1.0)), ("v", (3.5, 1.2)), ("w", (5.0, -1.0))],
        [("u", "v"), ("v", "w"), ("w", "u")],
        digraph_options,
    )?;
    digraph
        .vertex(&"v")
        .unwrap()
        .clone()
        .set_style(StyleUpdate {
            fill_color: Some(YELLOW),
            ..Default::default()
        })?;
    digraph
        .edge(&"v", &"w")
        .unwrap()
        .family()
        .set_style(StyleUpdate {
            fill_color: Some(PURPLE),
            stroke_color: Some(PURPLE),
            stroke_opacity: Some(0.3),
            ..Default::default()
        })?;
    digraph
        .vertex(&"v")
        .unwrap()
        .clone()
        .set_style(StyleUpdate {
            stroke_opacity: Some(0.2),
            ..Default::default()
        })?;

    digraph.change_layout(
        &mut scene,
        crate::GraphLayoutOptions {
            layout: crate::GraphLayout::Circular,
            scale: 1.6,
            scale_y: Some(1.0),
            center: (3.5, 0.0),
            ..Default::default()
        },
    )?;
    let mut added_vertex_options = options.clone();
    added_vertex_options.vertex_fill = YELLOW;
    added_vertex_options.vertex_stroke = YELLOW;
    let mut added_edge_options = options.clone();
    added_edge_options.edge_color = PURPLE;
    added_edge_options.edge_stroke_width = 0.03;
    digraph.add_vertices_and_edges_with_options(
        &mut scene,
        [("x", (5.0, -2.3), added_vertex_options)],
        [("w", "x", added_edge_options)],
    )?;
    let mut standalone_vertex_options = options.clone();
    standalone_vertex_options.vertex_radius = 0.16;
    standalone_vertex_options.vertex_fill = YELLOW;
    standalone_vertex_options.vertex_stroke = YELLOW;
    digraph
        .add_vertices_with_options(&mut scene, [("y", (2.0, -2.3), standalone_vertex_options)])?;

    let mut centered = scene.graph(
        [("left", (4.4, -1.0)), ("right", (1.6, -1.0))],
        [("left", "right")],
    )?;
    centered.change_layout(
        &mut scene,
        crate::GraphLayoutOptions {
            layout: crate::GraphLayout::Circular,
            scale: 1.4,
            scale_y: Some(0.8),
            center: (3.0, -1.0),
            ..Default::default()
        },
    )?;

    let mut spiral = scene.graph_with_options(
        [
            ("p0", (0.0, -2.6)),
            ("p1", (0.0, -2.6)),
            ("p2", (0.0, -2.6)),
        ],
        [("p0", "p1"), ("p1", "p2")],
        options.clone(),
    )?;
    spiral.change_layout(
        &mut scene,
        crate::GraphLayoutOptions {
            layout: crate::GraphLayout::Spiral,
            scale: 0.9,
            scale_y: Some(0.55),
            center: (0.0, -2.6),
            resolution: 0.35,
            equidistant: true,
            ..Default::default()
        },
    )?;

    let mut partite = scene.graph_with_options(
        [("q0", (3.5, 2.6)), ("q1", (3.5, 2.6)), ("q2", (3.5, 2.6))],
        [("q0", "q1"), ("q1", "q2")],
        options.clone(),
    )?;
    partite.change_layout(
        &mut scene,
        crate::GraphLayoutOptions {
            layout: crate::GraphLayout::Partite,
            scale: 0.9,
            scale_y: Some(0.55),
            center: (3.5, 2.6),
            partitions: vec![vec![0], vec![1, 2]],
            align_horizontal: true,
            ..Default::default()
        },
    )?;

    let spring_options = crate::GraphLayoutOptions {
        layout: crate::GraphLayout::Spring,
        scale: 0.65,
        scale_y: Some(0.35),
        center: (-3.5, 2.6),
        spring_k: Some(1.0 / 3.0_f64.sqrt()),
        edge_weights: Some(vec![2.0, 0.75]),
        spring_positions: Some(vec![
            (0.3745401188473625, 0.9507143064099162),
            (0.7319939418114051, 0.5986584841970366),
            (0.15601864044243652, 0.15599452033620265),
        ]),
        ..Default::default()
    };
    let spring_positions = spring_options.positions(3, &[(0, 1), (1, 2)])?;
    let mut spring = scene.graph_with_options(
        [
            ("r0", spring_positions[0]),
            ("r1", spring_positions[1]),
            ("r2", spring_positions[2]),
        ],
        [("r0", "r1"), ("r1", "r2")],
        options.clone(),
    )?;
    spring.change_layout(&mut scene, spring_options)?;
    let spring_digraph_options = crate::GraphLayoutOptions {
        layout: crate::GraphLayout::Spring,
        scale: 0.5,
        center: (5.5, 2.6),
        spring_positions: Some(vec![
            (0.07630828937395717, 0.7799187922401146),
            (0.4384092314408935, 0.7234651778309412),
        ]),
        directed: true,
        ..Default::default()
    };
    let spring_digraph_positions = spring_digraph_options.positions(2, &[(0, 1)])?;
    let spring_digraph = scene.digraph_with_options(
        [
            ("t0", spring_digraph_positions[0]),
            ("t1", spring_digraph_positions[1]),
        ],
        [("t0", "t1")],
        options.clone(),
    )?;

    let random_options = crate::GraphLayoutOptions {
        layout: crate::GraphLayout::Random,
        scale: 1.0,
        scale_y: Some(0.5),
        // NetworkX adds this center to its uniform samples before Manim's
        // wrapper applies the `2 * scale * (position - 0.5)` transform.
        random_positions: Some(vec![
            (2.2946650981903076, -0.7694132328033447),
            (2.1915206909179688, -1.2320996522903442),
            (2.786985397338867, -0.6436665058135986),
        ]),
        ..Default::default()
    };
    let random_positions = random_options.positions(3, &[(0, 1), (1, 2)])?;
    let random_graph = scene.graph_with_options(
        [
            ("m0", random_positions[0]),
            ("m1", random_positions[1]),
            ("m2", random_positions[2]),
        ],
        [("m0", "m1"), ("m1", "m2")],
        options.clone(),
    )?;
    let mut random_graph = random_graph;
    random_graph.change_layout(&mut scene, random_options)?;

    let tree_edges = [(0, 1), (0, 2), (1, 3), (1, 4)];
    let tree_options = crate::GraphLayoutOptions {
        layout: crate::GraphLayout::Tree,
        scale: 0.65,
        scale_y: Some(0.35),
        tree_root: Some(0),
        ..Default::default()
    };
    let tree_positions = tree_options.positions(5, &tree_edges)?;
    let tree_graph = scene.graph_with_options(
        [
            ("z0", tree_positions[0]),
            ("z1", tree_positions[1]),
            ("z2", tree_positions[2]),
            ("z3", tree_positions[3]),
            ("z4", tree_positions[4]),
        ],
        [("z0", "z1"), ("z0", "z2"), ("z1", "z3"), ("z1", "z4")],
        options.clone(),
    )?;
    let mut tree_graph = tree_graph;
    tree_graph.change_layout(&mut scene, tree_options)?;

    let spectral_edges = [(0, 1), (1, 2), (2, 3)];
    let spectral_options = crate::GraphLayoutOptions {
        layout: crate::GraphLayout::Spectral,
        scale: 0.35,
        scale_y: Some(0.22),
        center: (-5.5, 3.2),
        edge_weights: Some(vec![2.0, 0.75, 1.5]),
        ..Default::default()
    };
    let spectral_positions = spectral_options.positions(4, &spectral_edges)?;
    let mut spectral_graph = scene.graph_with_options(
        [
            ("g0", spectral_positions[0]),
            ("g1", spectral_positions[1]),
            ("g2", spectral_positions[2]),
            ("g3", spectral_positions[3]),
        ],
        [("g0", "g1"), ("g1", "g2"), ("g2", "g3")],
        options.clone(),
    )?;
    spectral_graph.change_layout(&mut scene, spectral_options)?;

    let kamada_kawai_edges = [(0, 1), (1, 2)];
    let kamada_kawai_options = crate::GraphLayoutOptions {
        layout: crate::GraphLayout::KamadaKawai,
        scale: 0.35,
        scale_y: Some(0.22),
        center: (-5.5, -2.6),
        edge_weights: Some(vec![2.0, 0.75]),
        ..Default::default()
    };
    let kamada_kawai_positions = kamada_kawai_options.positions(3, &kamada_kawai_edges)?;
    let mut kamada_kawai_graph = scene.graph_with_options(
        [
            ("k0", kamada_kawai_positions[0]),
            ("k1", kamada_kawai_positions[1]),
            ("k2", kamada_kawai_positions[2]),
        ],
        [("k0", "k1"), ("k1", "k2")],
        options.clone(),
    )?;
    kamada_kawai_graph.change_layout(&mut scene, kamada_kawai_options)?;

    let kamada_kawai_digraph_options = crate::GraphLayoutOptions {
        layout: crate::GraphLayout::KamadaKawai,
        scale: 0.3,
        center: (5.5, -2.6),
        edge_weights: Some(vec![1.75]),
        directed: true,
        ..Default::default()
    };
    let kamada_kawai_digraph_positions = kamada_kawai_digraph_options.positions(2, &[(0, 1)])?;
    let mut kamada_kawai_digraph = scene.digraph_with_options(
        [
            ("du", kamada_kawai_digraph_positions[0]),
            ("dv", kamada_kawai_digraph_positions[1]),
        ],
        [("du", "dv")],
        options.clone(),
    )?;
    kamada_kawai_digraph.change_layout(&mut scene, kamada_kawai_digraph_options)?;

    let mut shells = scene.graph_with_options(
        [("s0", (0.0, 2.6)), ("s1", (0.0, 2.6)), ("s2", (0.0, 2.6))],
        [("s0", "s1"), ("s1", "s2")],
        options,
    )?;
    shells.change_layout(
        &mut scene,
        crate::GraphLayoutOptions {
            layout: crate::GraphLayout::Shell,
            scale: 0.9,
            center: (0.0, 2.6),
            shells: vec![vec![0, 1], vec![2]],
            ..Default::default()
        },
    )?;

    // Python's standard `vertex_type=Circle` selects radius 1, no fill, and
    // Manim's four-unit white outline while keeping the same retained circles.
    let circle_options = GraphOptions {
        vertex_radius: 1.0,
        vertex_fill_opacity: 0.0,
        vertex_stroke_width: 0.04,
        ..GraphOptions::default()
    };
    let circle_vertices = scene.graph_with_options(
        [("circle-left", (-1.4, 0.0)), ("circle-right", (1.4, 0.0))],
        [("circle-left", "circle-right")],
        circle_options,
    )?;

    #[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
    let graph_display = graph_scene_family(&mut scene, graph.family(), Some(&graph_label))?;
    #[cfg(not(all(feature = "native-text", feature = "bundled-fonts")))]
    let graph_display = graph_scene_family(&mut scene, graph.family(), None)?;
    let display_families = [
        graph_display,
        graph_scene_family(&mut scene, digraph.family(), None)?,
        graph_scene_family(&mut scene, centered.family(), None)?,
        graph_scene_family(&mut scene, spiral.family(), None)?,
        graph_scene_family(&mut scene, partite.family(), None)?,
        graph_scene_family(&mut scene, shells.family(), None)?,
        graph_scene_family(&mut scene, spring.family(), None)?,
        graph_scene_family(&mut scene, spring_digraph.family(), None)?,
        graph_scene_family(&mut scene, random_graph.family(), None)?,
        graph_scene_family(&mut scene, tree_graph.family(), None)?,
        graph_scene_family(&mut scene, spectral_graph.family(), None)?,
        graph_scene_family(&mut scene, kamada_kawai_graph.family(), None)?,
        graph_scene_family(&mut scene, kamada_kawai_digraph.family(), None)?,
        graph_scene_family(&mut scene, circle_vertices.family(), None)?,
    ];
    let scene_roots = display_families
        .iter()
        .map(MobjectTarget::Family)
        .collect::<Vec<_>>();
    scene.add_many(&scene_roots)?;
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
        // Undirected: four Lines + five vertices.
        // Directed: four Arrow shafts + four tips + five vertices.
        // Spring and random Graphs each add two Lines + three vertices.
        // Tree Graph adds four Lines + five vertices.
        // Spring DiGraph: one Arrow shaft + tip + two vertices.
        // Spectral Graph: three Lines + four vertices.
        // Kamada-Kawai Graph: two Lines + three vertices.
        // Kamada-Kawai DiGraph: one Arrow, its tip, and two vertices.
        // Circle-vertex Graph: one Line and two stroked circle vertices.
        // The paired labeled Graph example contributes one retained Text leaf.
        assert_eq!(leaves.len(), 83);
        for leaf in leaves {
            let content = store.semantic_object_state_checked(leaf).unwrap().content;
            assert!(content.geometry().is_some() || content.text().is_some());
        }
    }
}
