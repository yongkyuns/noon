//! One retained plain, math, and composite-entry table scene for every host.

use crate::{
    ExecutionSession, LatexBackend, MathTable, MathTex, MobjectFamily, MobjectTable, Scene, Table,
    TableOptions,
};

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
    let plus = scene
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
    let composite = MobjectTable::from_target_rows_with_options(
        &mut scene,
        vec![vec![(&group).into(), (&y).into()]],
        options,
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
