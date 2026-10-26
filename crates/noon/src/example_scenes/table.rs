//! One retained plain, math, and composite-entry table scene for every host.

use crate::{
    Color, DecimalFormat, DecimalTable, LatexBackend, MathTable, MathTex, Mobject, MobjectFamily,
    MobjectTable, MobjectTarget, Scene, StrokeCap, StrokeJoin, Table, TableOptions,
};

/// Scene counterpart of the Python fixture that exercises group-valued table
/// labels and a group-valued top-left entry.
pub fn family_labels_session() -> Result<crate::SceneExecution, String> {
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
    scene.into_execution().map_err(|error| error.to_string())
}

/// Plain-text Table counterpart of the ManimCE fixture with retained labels.
pub fn labels_session() -> Result<crate::SceneExecution, String> {
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
    scene.into_execution().map_err(|error| error.to_string())
}

/// Retained counterpart of the ManimCE line-config fixture, including cap and
/// join style values that are carried through Table's sparse layout.
pub fn line_config_session() -> Result<crate::SceneExecution, String> {
    let mut scene = Scene::new();
    let first_circle = scene.circle(0.3).map_err(|error| error.to_string())?;
    let first_square = scene.square(0.5).map_err(|error| error.to_string())?;
    let second_square = scene.square(0.4).map_err(|error| error.to_string())?;
    let second_circle = scene.circle(0.2).map_err(|error| error.to_string())?;
    let table = MobjectTable::from_rows_with_options(
        &mut scene,
        vec![
            vec![first_circle, first_square],
            vec![second_square, second_circle],
        ],
        TableOptions {
            h_buff: 0.7,
            v_buff: 0.45,
            include_outer_lines: true,
            line_stroke_color: Color::RED,
            line_stroke_width: 0.06,
            line_stroke_opacity: 0.6,
            line_stroke_join: StrokeJoin::Bevel,
            line_stroke_cap: StrokeCap::Square,
            cell_alignment_x: -1.0,
            cell_alignment_y: 1.0,
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?
    .into_table();
    scene
        .add_many(&[table.family().into()])
        .map_err(|error| error.to_string())?;
    scene.into_execution().map_err(|error| error.to_string())
}

pub fn session(backend: &mut impl LatexBackend) -> Result<crate::SceneExecution, String> {
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
    // Generic Table entry handles retain text content without a frontend class.
    // Every direct native/WASM host must select the existing content query.
    for entry in plain.get_entries().map_err(|error| error.to_string())? {
        assert!(entry
            .as_target()
            .requires_content_boundary()
            .map_err(|error| error.to_string())?);
    }
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
    let mut execution = scene
        .execution_session()
        .map_err(|error| error.to_string())?;
    for entry in plain.get_entries().map_err(|error| error.to_string())? {
        assert!(scene
            .live(&mut execution)
            .requires_content_boundary(entry.as_target())
            .map_err(|error| error.to_string())?);
    }
    scene
        .into_execution_with_session(execution)
        .map_err(|error| error.to_string())
}

/// DecimalTable counterpart for ManimCE's configurable DecimalNumber entries.
pub fn decimal_format_session(
    backend: &mut impl LatexBackend,
) -> Result<crate::SceneExecution, String> {
    let mut scene = Scene::new();
    let table = DecimalTable::from_rows_with_options(
        &mut scene,
        backend,
        [[1234.5, -2.25]],
        DecimalFormat {
            decimal_places: 2,
            include_sign: true,
            group_with_commas: true,
            ..Default::default()
        },
        TableOptions {
            include_outer_lines: true,
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?
    .into_table();
    scene
        .add_many(&[table.family().into()])
        .map_err(|error| error.to_string())?;
    scene.into_execution().map_err(|error| error.to_string())
}
