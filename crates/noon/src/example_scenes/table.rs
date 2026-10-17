//! One retained plain, math, and composite-entry table scene for every host.

use crate::{
    Color, ExecutionSession, LatexBackend, MathTable, MathTex, Mobject, MobjectFamily,
    MobjectTable, MobjectTarget, Scene, Table, TableOptions,
};

/// Scene counterpart of the Python fixture that exercises group-valued table
/// labels and a group-valued top-left entry.
pub fn family_labels_session() -> Result<ExecutionSession, String> {
    enum Shape {
        Circle(f64),
        Square(f64),
    }

    fn shape(scene: &mut Scene, shape: Shape) -> Result<Mobject, String> {
        match shape {
            Shape::Circle(radius) => scene.circle(radius),
            Shape::Square(side) => scene.square(side),
        }
        .map_err(|error| error.to_string())
    }

    fn family(
        scene: &mut Scene,
        first: Shape,
        second: Shape,
        offset_x: f64,
    ) -> Result<MobjectFamily, String> {
        let first = shape(scene, first)?;
        let mut second = shape(scene, second)?;
        second
            .shift(offset_x, 0.0)
            .map_err(|error| error.to_string())?;
        scene
            .family(&[(&first).into(), (&second).into()])
            .map_err(|error| error.to_string())
    }

    let mut scene = Scene::new();
    let row_labels = [
        family(&mut scene, Shape::Circle(0.12), Shape::Square(0.16), 0.3)?,
        family(&mut scene, Shape::Square(0.14), Shape::Circle(0.1), 0.28)?,
    ];
    let column_labels = [
        family(&mut scene, Shape::Circle(0.11), Shape::Square(0.13), 0.26)?,
        family(&mut scene, Shape::Square(0.12), Shape::Circle(0.09), 0.24)?,
    ];
    let top_left_entry = family(&mut scene, Shape::Circle(0.18), Shape::Square(0.12), 0.32)?;
    let entries = [
        scene.circle(0.3).map_err(|error| error.to_string())?,
        scene.square(0.42).map_err(|error| error.to_string())?,
        scene.square(0.36).map_err(|error| error.to_string())?,
        scene.circle(0.22).map_err(|error| error.to_string())?,
    ];
    let table = MobjectTable::from_target_rows_with_composite_labels_and_top_left_entry(
        &mut scene,
        vec![
            vec![
                MobjectTarget::from(&entries[0]),
                MobjectTarget::from(&entries[1]),
            ],
            vec![
                MobjectTarget::from(&entries[2]),
                MobjectTarget::from(&entries[3]),
            ],
        ],
        row_labels.iter().map(MobjectTarget::from).collect(),
        column_labels.iter().map(MobjectTarget::from).collect(),
        MobjectTarget::from(&top_left_entry),
        TableOptions {
            h_buff: 0.7,
            v_buff: 0.45,
            include_outer_lines: true,
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?
    .into_table();
    scene
        .add_many(&[table.family().into()])
        .map_err(|error| error.to_string())?;
    scene.execution_session().map_err(|error| error.to_string())
}

/// Plain-text Table counterpart of the ManimCE fixture with retained labels.
pub fn labels_session() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let row_labels = [
        scene.circle(0.15).map_err(|error| error.to_string())?,
        scene.square(0.4).map_err(|error| error.to_string())?,
    ];
    let column_labels = [scene.circle(0.25).map_err(|error| error.to_string())?];
    let top_left_entry = scene.square(0.3).map_err(|error| error.to_string())?;
    let table = Table::from_rows_with_composite_labels(
        &mut scene,
        [[" "], [" "]],
        Some(row_labels.iter().map(MobjectTarget::from).collect()),
        Some(column_labels.iter().map(MobjectTarget::from).collect()),
        Some(MobjectTarget::from(&top_left_entry)),
        TableOptions {
            h_buff: 0.7,
            v_buff: 0.45,
            include_outer_lines: true,
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?;
    scene
        .add_many(&[table.family().into()])
        .map_err(|error| error.to_string())?;
    scene.execution_session().map_err(|error| error.to_string())
}

pub fn session(backend: &mut impl LatexBackend) -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let options = TableOptions {
        include_outer_lines: true,
        ..Default::default()
    };
    let plain = Table::from_rows_with_options(
        &mut scene,
        [["plain", "native"], ["text", "table"]],
        options,
    )
    .map_err(|error| error.to_string())?;
    plain
        .family()
        .shift(-3.0, 1.4)
        .map_err(|error| error.to_string())?;
    let math = MathTable::from_rows_with_options(
        &mut scene,
        backend,
        [["x^2", "y"], [r"\alpha", r"\frac{1}{2}"]],
        options,
    )
    .map_err(|error| error.to_string())?
    .into_table();
    math.family()
        .shift(3.0, 1.4)
        .map_err(|error| error.to_string())?;

    let x = scene
        .math_tex(
            MathTex::new("x").map_err(|error| error.to_string())?,
            backend,
        )
        .map_err(|error| error.to_string())?;
    let mut plus = scene
        .math_tex(
            MathTex::new("+").map_err(|error| error.to_string())?,
            backend,
        )
        .map_err(|error| error.to_string())?;
    plus.shift(0.5, 0.0).map_err(|error| error.to_string())?;
    let group = MobjectFamily::create(
        std::rc::Rc::clone(scene.integration_store()),
        &[(&x).into(), (&plus).into()],
    )
    .map_err(|error| error.to_string())?;
    let y = scene
        .math_tex(
            MathTex::new("y").map_err(|error| error.to_string())?,
            backend,
        )
        .map_err(|error| error.to_string())?;
    let row_label = scene.text("r").map_err(|error| error.to_string())?;
    let column_labels = ["a", "b"]
        .into_iter()
        .map(|label| scene.text(label).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let top_left_entry = scene.text("i").map_err(|error| error.to_string())?;
    let composite_options = TableOptions {
        include_inner_lines: false,
        line_stroke_color: Color::YELLOW,
        line_stroke_width: 0.06,
        line_stroke_opacity: 0.75,
        ..options
    };
    let composite = MobjectTable::from_target_rows_with_labels_and_top_left_entry(
        &mut scene,
        vec![vec![(&group).into(), (&y).into()]],
        vec![row_label],
        column_labels,
        top_left_entry,
        composite_options,
    )
    .map_err(|error| error.to_string())?
    .into_table();
    composite
        .family()
        .shift(0.0, -2.0)
        .map_err(|error| error.to_string())?;
    scene
        .add_many(&[
            plain.family().into(),
            math.family().into(),
            composite.family().into(),
        ])
        .map_err(|error| error.to_string())?;
    scene.execution_session().map_err(|error| error.to_string())
}
